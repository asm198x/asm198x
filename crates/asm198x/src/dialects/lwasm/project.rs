//! lwasm's conditional evaluator, the phase seam of #486: the `CondEval` that
//! folds each conditional head and lowers each live line against the `equ`
//! environment as it stands. It is its own file so the pass-dependent
//! evaluation reads apart from the line parse and the addressing-mode
//! encoding in the parent module.

use std::collections::BTreeMap;

use super::super::macros;
use super::super::mos6502::{self, fold_const, split_first_word};
use super::{
    AsmError, Expr, InlineSource, Node, Operation, ParseState, Statement, StructEffect, parse_op,
    parse_program, pragma_is_on, pragma_named, struct_line, value,
};

// ---------------------------------------------------------------------------
// Conditional evaluation — lwasm's `CondEval` (the adoption recipe's steps 1
// and 3, `decisions/conditional-assembly-framework.md`).
//
// Why this dialect needs a real evaluator rather than a fold in the walk: an
// `equ` decides lwasm's **addressing mode**, and the mode decides the
// instruction's *size*. `sym equ $10` gives `96 10` (direct, two bytes) and
// `sym equ $1234` gives `b6 12 34` (extended, three). Real lwasm refuses
// `lda sym` outright when that `equ` sits in an untaken branch, so a binding
// made while parsing both branches would silently choose direct where the
// reference errors. Each live line therefore re-parses here, against the
// environment as it actually stands — which is ACME's model unchanged.
// ---------------------------------------------------------------------------

/// lwasm's conditional evaluator: the `equ` environment, threaded through the
/// walk so a later direct/extended choice sees only what a taken branch bound.
pub(super) struct LwasmEval {
    pub(super) env: BTreeMap<String, i64>,
    /// What the live directives so far left for the lines after them. This is
    /// the copy that decides the emitted bytes.
    pub(super) state: ParseState,
}

impl crate::ast::CondEval for LwasmEval {
    fn condition_diagnostic(&self, head: &str) -> Option<Operation> {
        let (word, _) = split_first_word(head.trim());
        let word = word.to_ascii_lowercase();
        matches!(word.as_str(), "ifp1" | "ifp2").then(|| Operation::Diagnose {
            severity: crate::engine::DiagSeverity::Warning,
            message: format!("Not supported {}", word.to_ascii_uppercase()),
        })
    }

    /// Fold one conditional head. Every numeric form compares against zero;
    /// `ifdef`/`ifndef` test the environment for a name.
    fn eval(&self, head: &str, line: u32) -> Result<bool, AsmError> {
        let line = line as usize;
        let (word, args) = split_first_word(head.trim());
        let word = word.to_ascii_lowercase();
        let args = args.trim();
        if word == "ifstr" {
            return eval_ifstr(args, line);
        }
        if word == "ifp1" || word == "ifp2" {
            return Ok(true);
        }
        if word == "ifpragma" || word == "ifopt" {
            let name = args
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            // Both spellings refuse a name they do not know, where `opt`
            // itself passes over one — measured: `opt zzz` assembles and
            // `ifopt zzz` does not.
            let (index, want_on) = pragma_named(&name).ok_or_else(|| {
                AsmError::new(line, format!("unrecognized pragma string `{name}`"))
            })?;
            return Ok(pragma_is_on(&self.state.pragmas, index) == want_on);
        }
        if word == "ifdef" || word == "ifndef" {
            let name = args
                .split_whitespace()
                .next()
                .ok_or_else(|| AsmError::new(line, format!("`{word}` needs a name")))?;
            let defined = self.env.contains_key(name);
            return Ok(if word == "ifdef" { defined } else { !defined });
        }
        if args.is_empty() {
            return Err(AsmError::new(line, format!("`{word}` needs a condition")));
        }
        let value = fold_const(&value(args, line)?, &self.env, line).map_err(|_| {
            AsmError::new(
                line,
                format!(
                    "`{args}` must be a constant here — lwasm folds a condition against the \
                     `equ` values above it"
                ),
            )
        })?;
        Ok(match word.as_str() {
            "if" | "ifne" => value != 0,
            "ifeq" => value == 0,
            "ifgt" => value > 0,
            "ifge" => value >= 0,
            "iflt" => value < 0,
            "ifle" => value <= 0,
            _ => {
                return Err(AsmError::new(
                    line,
                    format!("internal error: `{head}` is not a conditional head"),
                ));
            }
        })
    }

    /// Lower one **live** line, re-parsing its operation against the current
    /// environment so the direct/extended choice sees the live bindings.
    fn lower(&mut self, node: &Node, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        if let Some(crate::ast::Item::Native(native)) = &node.item
            && let Some(inline) = native.as_any().downcast_ref::<InlineSource>()
        {
            if let Some(label) = &node.label {
                out.push(Statement {
                    line,
                    file: node.span.file,
                    label: Some(label.qualified.clone()),
                    op: None,
                    operand_span: None,
                    xor_mask: 0,
                    instruction_set: None,
                    extension_set: None,
                });
            }
            let program = parse_program(&inline.0, macros::Expand::Yes)?;
            let start = out.len();
            crate::ast::evaluate(self, &program.nodes, true, out)?;
            for statement in &mut out[start..] {
                statement.line = line;
                statement.file = node.span.file;
                statement.operand_span = Some(node.span.clone());
            }
            return Ok(());
        }
        let label = node.label.as_ref().map(|s| s.qualified.clone());
        if let Some(effect) = struct_line(
            label.as_deref(),
            &node.source,
            &mut self.state,
            &self.env,
            line,
        )? {
            let at = |op, label| Statement {
                line,
                file: node.span.file,
                label,
                op: Some(op),
                operand_span: None,
                xor_mask: 0,
                instruction_set: None,
                extension_set: None,
            };
            match effect {
                StructEffect::Nothing => {}
                // The offsets are constants, so they are bound as constants —
                // `pt.x` reads as one with no instance anywhere.
                StructEffect::Closed { name, def } => {
                    for (member, offset) in &def.members {
                        let sym = format!("{name}.{member}");
                        self.env.insert(sym.clone(), *offset);
                        out.push(at(Operation::Equ(Expr::Num(*offset)), Some(sym)));
                    }
                }
                // An instance is room with names on it: the label lands where
                // the room starts, and each member's name lands at its offset
                // into it.
                StructEffect::Instance { def } => {
                    let base = label.clone().expect("an instance has a label");
                    out.push(at(
                        Operation::Bytes(vec![Expr::Num(0); def.size as usize]),
                        Some(base.clone()),
                    ));
                    for (member, offset) in &def.members {
                        out.push(at(
                            Operation::Equ(Expr::Bin(
                                crate::engine::BinOp::Add,
                                Box::new(Expr::Sym(base.clone())),
                                Box::new(Expr::Num(*offset)),
                            )),
                            Some(format!("{base}.{member}")),
                        ));
                    }
                }
            }
            return Ok(());
        }
        // A walk-resolved payload keeps what the walk built: an `includebin`'s
        // bytes cannot be rebuilt here, because resolving one needs the loader
        // the walk had and this does not. Everything else re-parses.
        let op = match &node.item {
            Some(crate::ast::Item::Binary(payload)) => Some(Operation::Binary(payload.clone())),
            Some(crate::ast::Item::Include { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `include \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves includes automatically)"
                    ),
                ));
            }
            Some(crate::ast::Item::Incbin { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `includebin \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves binary inclusions automatically)"
                    ),
                ));
            }
            _ if node.source.is_empty() => None,
            _ => parse_op(&node.source, &self.env, &mut self.state, line)?,
        };
        if let (Some(sym), Some(Operation::Equ(e) | Operation::Set(e))) = (node.label.as_ref(), &op)
            && let Ok(v) = fold_const(e, &self.env, line)
        {
            self.env.insert(sym.qualified.clone(), v);
        }
        out.push(Statement {
            line,
            file: node.span.file,
            label: node.label.as_ref().map(|s| s.qualified.clone()),
            op,
            operand_span: node.operand_span.clone(),
            xor_mask: 0,
            instruction_set: None,
            extension_set: None,
        });
        Ok(())
    }
}

/// Evaluate lwasm's twelve `ifstr` operators. `p` compares prefixes, `s`
/// suffixes, and an initial `i` makes the comparison ASCII-insensitive.
fn eval_ifstr(operand: &str, line: usize) -> Result<bool, AsmError> {
    let parts = mos6502::split_top_level(operand, ',');
    let op = parts
        .first()
        .map(|p| p.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let insensitive = op.starts_with('i');
    let base = op.strip_prefix('i').unwrap_or(&op);
    let (left, right) = match base {
        "eq" | "ne" => {
            if parts.len() != 3 {
                return Err(AsmError::new(line, "`ifstr` comparison needs two strings"));
            }
            (ifstr_arg(parts[1]), ifstr_arg(parts[2]))
        }
        "peq" | "pne" | "seq" | "sne" => {
            if parts.len() != 4 {
                return Err(AsmError::new(
                    line,
                    "`ifstr` prefix/suffix comparison needs a length and two strings",
                ));
            }
            let n = ifstr_arg(parts[1]).parse::<usize>().unwrap_or(0);
            let a = ifstr_arg(parts[2]);
            let b = ifstr_arg(parts[3]);
            if base.starts_with('p') {
                (a.chars().take(n).collect(), b.chars().take(n).collect())
            } else {
                (
                    a.chars()
                        .rev()
                        .take(n)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect(),
                    b.chars()
                        .rev()
                        .take(n)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect(),
                )
            }
        }
        _ => {
            return Err(AsmError::new(
                line,
                format!("unknown `ifstr` operator `{op}`"),
            ));
        }
    };
    let equal = if insensitive {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    };
    Ok(if base.ends_with("ne") { !equal } else { equal })
}

fn ifstr_arg(raw: &str) -> String {
    let text = raw.trim();
    if text.len() >= 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\'')))
    {
        text[1..text.len() - 1].to_string()
    } else {
        text.to_string()
    }
}
