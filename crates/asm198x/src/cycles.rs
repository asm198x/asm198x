//! Cycle accounting shared by the listing tail and the budget assertion
//! (#497): per-label straight-line costs computed from the capture, and the
//! `; asm198x: cycles(<label>) <= N` magic comment that fails the build.
//!
//! The magic comment is asm198x-only behaviour that stays source-compatible:
//! every reference assembler reads the line as the comment it is, so the
//! bytes never change; only this assembler acts on it. A comment written as
//! an assertion is held to assertion standards — a malformed spelling or an
//! unknown label is an error, never silently skimmed as prose, because an
//! assertion that checked nothing would fake the assurance it exists to give.

use crate::contract::AssemblyResult;
use crate::engine::{AsmError, CycleCoverage};

/// One label's straight-line account: the emitted bytes and honest cycle
/// range from the label's offset to the next labelled offset (or the end).
///
/// "Straight-line" is the static-analysis contract: this sums what sits
/// between two labels in address order. It does not follow control flow, so a
/// loop body counts once — Emu198x executes; this measures.
pub(crate) struct LabelCost {
    pub(crate) name: String,
    /// Section-relative offset of the label.
    pub(crate) start: u64,
    /// Emitted bytes attributed to lines in the span.
    pub(crate) bytes: u64,
    /// Cycle cost with every conditional extra unspent.
    pub(crate) min: u64,
    /// Cycle cost with every page-cross and branch-taken extra spent.
    pub(crate) max: u64,
}

/// Every label whose straight-line span contains at least one captured
/// instruction, in address order. Aliased labels (two names, one offset)
/// each account the same span — to the next *greater* labelled offset.
pub(crate) fn label_costs(result: &AssemblyResult, addr_unit: u64) -> Vec<LabelCost> {
    label_costs_debug(&result.debug, addr_unit)
}

/// [`label_costs`] over the engine-side record, for the engine's own check.
pub(crate) fn label_costs_debug(
    debug: &crate::engine::DebugData,
    addr_unit: u64,
) -> Vec<LabelCost> {
    let mut labels: Vec<(u64, &str)> = debug
        .symbols
        .iter()
        .filter_map(|s| match &s.kind {
            debug198x::SymbolKind::Label { offset, .. }
            | debug198x::SymbolKind::Entry { offset, .. } => Some((*offset, s.name.as_str())),
            debug198x::SymbolKind::Const { .. } => None,
        })
        .collect();
    labels.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
    let mut out = Vec::new();
    for &(start, name) in &labels {
        let end = labels
            .iter()
            .map(|&(o, _)| o)
            .find(|&o| o > start)
            .unwrap_or(u64::MAX);
        let in_span = |offset: u64| offset >= start && offset < end;
        let mut cost = (0u64, 0u64);
        let mut any = false;
        for c in debug.cycles.iter().filter(|c| in_span(c.offset)) {
            any = true;
            cost.0 += u64::from(c.base);
            cost.1 += u64::from(c.base) + u64::from(c.page_cross) + u64::from(c.branch_taken);
        }
        if any {
            let bytes: u64 = debug
                .lines
                .iter()
                .filter(|l| in_span(l.offset))
                .map(|l| l.length * addr_unit)
                .sum();
            out.push(LabelCost {
                name: name.to_string(),
                start,
                bytes,
                min: cost.0,
                max: cost.1,
            });
        }
    }
    out
}

/// Check every `; asm198x: cycles(<label>) <= N` assertion in `sources`
/// against the capture. `sources` pairs each file's display name with its
/// full text — the root alone for a single-file assemble, every file of a
/// multi-file one. Run by the engine itself at the end of every flat
/// assemble, so an assertion fails the build wherever assembly happens —
/// CLI, library, or a later surface — never only in one front-end.
///
/// # Errors
///
/// A routine over budget (naming routine, budget, and actual worst case); a
/// budget naming an unknown label; a malformed `asm198x:` comment; or any
/// budget at all where the capture is not [`CycleCoverage::Full`] — a lower
/// bound cannot prove a ceiling, so Partial refuses too.
pub(crate) fn check_cycle_budgets(
    sources: &[(&str, &str)],
    debug: &crate::engine::DebugData,
) -> Result<(), AsmError> {
    let costs = label_costs_debug(debug, 1);
    for (file, text) in sources {
        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            let Some(comment) = raw.split_once(';').map(|(_, c)| c.trim()) else {
                continue;
            };
            let Some(directive) = comment.strip_prefix("asm198x:").map(str::trim) else {
                continue;
            };
            let budget = parse_budget(directive).ok_or_else(|| {
                AsmError::new(
                    line,
                    format!(
                        "malformed asm198x assertion `{directive}` in {file} \
                         (expected `cycles(<label>) <= <n>`)"
                    ),
                )
            })?;
            match debug.cycle_coverage {
                CycleCoverage::Full => {}
                CycleCoverage::Partial => {
                    return Err(AsmError::new(
                        line,
                        format!(
                            "cannot check `cycles({})`: some of this dialect's \
                             instructions carry no cycle data, so figures are \
                             lower bounds and cannot prove a ceiling",
                            budget.0
                        ),
                    ));
                }
                CycleCoverage::None => {
                    return Err(AsmError::new(
                        line,
                        format!(
                            "cannot check `cycles({})`: no cycle data for this \
                             CPU (backfill pending)",
                            budget.0
                        ),
                    ));
                }
            }
            let (label, limit) = budget;
            let cost = costs.iter().find(|c| c.name == label).ok_or_else(|| {
                AsmError::new(
                    line,
                    format!("`cycles({label})` names no label with instructions in this program"),
                )
            })?;
            if cost.max > limit {
                return Err(AsmError::new(
                    line,
                    format!(
                        "`{label}` exceeds its cycle budget: {limit} allowed, \
                         {} worst case (straight-line to the next label)",
                        cost.max
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Parse `cycles(<label>) <= <n>`; anything else is `None`, which the caller
/// reports as the error it is.
fn parse_budget(directive: &str) -> Option<(String, u64)> {
    let rest = directive.strip_prefix("cycles(")?;
    let (label, rest) = rest.split_once(')')?;
    let label = label.trim();
    if label.is_empty()
        || !label
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return None;
    }
    let rest = rest.trim_start();
    let n = rest.strip_prefix("<=")?.trim();
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((label.to_string(), n.parse().ok()?))
}
