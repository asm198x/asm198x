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
    pub(crate) section: debug198x::SectionId,
    pub(crate) name: String,
    /// Section-relative offset of the label.
    pub(crate) start: u64,
    /// Emitted bytes attributed to lines in the span.
    pub(crate) bytes: u64,
    /// Cycle cost with every conditional extra unspent.
    pub(crate) min: u64,
    /// Worst nominal cycle cost, or no finite maximum for a wait.
    pub(crate) max: Option<u64>,
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
    let mut labels: Vec<(u64, &str, debug198x::SectionId)> = debug
        .symbols
        .iter()
        .filter_map(|s| match &s.kind {
            debug198x::SymbolKind::Label {
                section, offset, ..
            }
            | debug198x::SymbolKind::Entry {
                section, offset, ..
            } => Some((*offset, s.name.as_str(), *section)),
            debug198x::SymbolKind::Const { .. } => None,
        })
        .collect();
    labels.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
    let mut out = Vec::new();
    for &(start, name, section) in &labels {
        let end = labels
            .iter()
            .map(|&(o, _, _)| o)
            .find(|&o| o > start)
            .unwrap_or(u64::MAX);
        let in_span = |offset: u64| offset >= start && offset < end;
        let mut cost = (0u64, Some(0u64));
        let mut any = false;
        for c in debug.cycles.iter().filter(|c| in_span(c.offset)) {
            any = true;
            let (min, max) = c.range();
            cost.0 += min;
            cost.1 = cost.1.zip(max).map(|(a, b)| a + b);
        }
        if any {
            let bytes: u64 = debug
                .lines
                .iter()
                .filter(|l| in_span(l.offset))
                .map(|l| l.length * addr_unit)
                .sum();
            out.push(LabelCost {
                section,
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

/// Logical spans of one native source segment share its physical routine ranges.
/// Qualified and unmapped sections retain independent accounting.
pub(crate) fn native_label_costs(
    sections: &[crate::contract::SectionDebug],
    unit: u64,
) -> Vec<LabelCost> {
    let mut groups: Vec<Vec<&crate::contract::SectionDebug>> = Vec::new();
    let mut names = std::collections::BTreeMap::new();
    for section in sections {
        let unqualified = section.section.space.is_none()
            && section.debug.symbols.iter().all(|symbol| {
                !matches!(
                    &symbol.kind,
                    debug198x::SymbolKind::Label { space: Some(_), .. }
                        | debug198x::SymbolKind::Entry { space: Some(_), .. }
                )
            });
        if section.file_offset.is_some() && unqualified {
            let next = groups.len();
            let index = *names.entry(section.section.name.as_str()).or_insert(next);
            if index == groups.len() {
                groups.push(Vec::new());
            }
            groups[index].push(section);
        } else {
            groups.push(vec![section]);
        }
    }
    let mut out = Vec::new();
    for group in groups {
        if group.len() == 1 {
            out.extend(label_costs_debug(&group[0].debug, unit));
            continue;
        }
        // Work in byte positions, then restore the label's logical coordinate.
        let mut physical = crate::engine::DebugData::default();
        let mut origins = std::collections::BTreeMap::new();
        let mut valid = true;
        for span in group {
            let Some(base) = span.file_offset else {
                valid = false;
                break;
            };
            origins.insert(span.section.id, base);
            let position = |offset: u64| offset.checked_mul(unit)?.checked_add(base);
            for symbol in &span.debug.symbols {
                let mut symbol = symbol.clone();
                if let debug198x::SymbolKind::Label {
                    offset, section, ..
                }
                | debug198x::SymbolKind::Entry {
                    offset, section, ..
                } = &mut symbol.kind
                {
                    let Some(at) = position(*offset) else {
                        valid = false;
                        break;
                    };
                    *offset = at;
                    *section = span.section.id;
                    physical.symbols.push(symbol);
                }
            }
            for line in &span.debug.lines {
                let (Some(offset), Some(length)) =
                    (position(line.offset), line.length.checked_mul(unit))
                else {
                    valid = false;
                    break;
                };
                let mut line = line.clone();
                line.offset = offset;
                line.length = length;
                physical.lines.push(line);
            }
            for cycle in &span.debug.cycles {
                let Some(offset) = position(cycle.offset) else {
                    valid = false;
                    break;
                };
                let mut cycle = *cycle;
                cycle.offset = offset;
                physical.cycles.push(cycle);
            }
        }
        if !valid || unit == 0 {
            continue;
        }
        for mut cost in label_costs_debug(&physical, 1) {
            if let Some(offset) = origins
                .get(&cost.section)
                .and_then(|base| cost.start.checked_sub(*base))
            {
                cost.start = offset / unit;
                out.push(cost);
            }
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
    areas: &[crate::engine::AreaUsage],
) -> Result<(), AsmError> {
    let costs = label_costs_debug(debug, 1);
    check_budgets(sources, &costs, debug.cycle_coverage, areas)
}

/// Check an already section-aware account from a native driver. Keeping the
/// assertion parser here gives native and flat assembly identical semantics.
pub(crate) fn check_budgets(
    sources: &[(&str, &str)],
    costs: &[LabelCost],
    coverage: CycleCoverage,
    areas: &[crate::engine::AreaUsage],
) -> Result<(), AsmError> {
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
                         (expected `cycles(<label>) <= <n>` or `free(<area>) >= <n>`)"
                    ),
                )
            })?;
            let (label, limit) = match budget {
                Budget::Cycles(label, limit) => (label, limit),
                Budget::Free(area, min) => {
                    let usage = areas.iter().find(|a| a.name == area).ok_or_else(|| {
                        AsmError::new(
                            line,
                            format!(
                                "`free({area})` names no memory area in this layout \
                                 (areas: {})",
                                areas
                                    .iter()
                                    .map(|a| a.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        )
                    })?;
                    if u64::from(usage.free) < min {
                        return Err(AsmError::new(
                            line,
                            format!(
                                "area `{area}` breaks its space budget: {min} unit(s) of \
                                 headroom required, {} free ({} used of {})",
                                usage.free, usage.used, usage.size
                            ),
                        ));
                    }
                    continue;
                }
            };
            match coverage {
                CycleCoverage::Full => {}
                CycleCoverage::Partial => {
                    return Err(AsmError::new(
                        line,
                        format!(
                            "cannot check `cycles({})`: some of this dialect's \
                             instructions carry no cycle data, so figures are \
                             lower bounds and cannot prove a ceiling",
                            label
                        ),
                    ));
                }
                CycleCoverage::None => {
                    return Err(AsmError::new(
                        line,
                        format!(
                            "cannot check `cycles({})`: no cycle data for this \
                             CPU (backfill pending)",
                            label
                        ),
                    ));
                }
            }
            let cost = costs.iter().find(|c| c.name == label).ok_or_else(|| {
                AsmError::new(
                    line,
                    format!("`cycles({label})` names no label with instructions in this program"),
                )
            })?;
            let max = cost.max.ok_or_else(|| AsmError::new(line, format!(
                "cannot check `cycles({label})`: the routine contains an unbounded wait;                  its minimum of {} cycles cannot prove a ceiling", cost.min
            )))?;
            if max > limit {
                return Err(AsmError::new(
                    line,
                    format!(
                        "`{label}` exceeds its cycle budget: {limit} allowed, \
                         {} worst case (straight-line to the next label)",
                        max
                    ),
                )
                .with_code(crate::contract::Code::CycleBudgetExceeded));
            }
        }
    }
    Ok(())
}

/// One parsed assertion: a cycle ceiling on a routine, or a space floor on a
/// memory area (#499).
enum Budget {
    Cycles(String, u64),
    Free(String, u64),
}

/// Parse `cycles(<label>) <= <n>` or `free(<area>) >= <n>`; anything else is
/// `None`, which the caller reports as the error it is. `<n>` is decimal or
/// `$hex`.
fn parse_budget(directive: &str) -> Option<Budget> {
    let (kind, rest, op) = match directive.split_once('(') {
        Some(("cycles", rest)) => ("cycles", rest, "<="),
        Some(("free", rest)) => ("free", rest, ">="),
        _ => return None,
    };
    let (name, rest) = rest.split_once(')')?;
    let name = name.trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ' ')
    {
        return None;
    }
    let n = rest.trim_start().strip_prefix(op)?.trim();
    let value = if let Some(hex) = n.strip_prefix('$') {
        u64::from_str_radix(hex, 16).ok()?
    } else if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
        return None;
    } else {
        n.parse().ok()?
    };
    Some(match kind {
        "cycles" => Budget::Cycles(name.to_string(), value),
        _ => Budget::Free(name.to_string(), value),
    })
}
