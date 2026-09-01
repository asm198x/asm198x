//! rgbasm's conditional evaluator (#486's phase split): the `CondEval` the
//! `ast::evaluate` walk drives, folding each condition and `REPT` count against
//! the constants as a taken branch left them, and lowering each live line
//! through the parent's line parser. It is its own file so the phase layer of
//! `decisions/conditionals-in-multipass-dialects.md` can be read apart from
//! the operand grammar and the text layer. Moved verbatim from the parent.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    AsmError, DEF_MARK, Expr, Node, Operation, Statement, fold_const, parse_op, pc_ds_parts,
    rs_definition, split_first_word, split_top_level, value,
};

// ---------------------------------------------------------------------------
// Conditional evaluation — rgbasm's `CondEval`.
//
// `ast::lower` rejects an `Item::Conditional`, so assembly runs through
// `ast::evaluate`. Each live line re-parses against the constants as they
// actually stand, because rgbasm threads them into `parse_op` and SM83's
// `ldh [$FF40],a` selection reads them — a constant bound inside an untaken
// branch could otherwise change an instruction.
//
// What does **not** re-parse is a line the walk handled as a directive.
// `SECTION` has no form `parse_op` could rebuild, so a blanket re-parse answers
// `unknown instruction \`SECTION\`` on line 1 of every file. Those keep the
// walk's item through `lower_item_ref`.
// ---------------------------------------------------------------------------

/// rgbasm's conditional evaluator: the constant environment, threaded through
/// the walk so a condition folds against what a taken branch bound.
pub(super) struct RgbasmEval {
    pub(super) set: &'static isa::InstructionSet,
    pub(super) consts: BTreeMap<String, i64>,
    pub(super) defined: BTreeSet<String>,
    pub(super) global: String,
    pub(super) rs: i64,
}

/// Symbols RGBASM itself defines before reading the first source line. The
/// compatibility surface is measured against RGBDS 1.0.3, the reference tool
/// recorded by the verdict corpus, so its version tuple is observable source
/// input just as `_RS`'s initial zero is.
pub(super) fn initial_symbols() -> (BTreeMap<String, i64>, BTreeSet<String>) {
    let consts = BTreeMap::from([
        ("_RS".to_string(), 0),
        ("__RGBDS_MAJOR__".to_string(), 1),
        ("__RGBDS_MINOR__".to_string(), 0),
        ("__RGBDS_PATCH__".to_string(), 3),
    ]);
    let defined = consts.keys().cloned().collect();
    (consts, defined)
}

type LoweredRs = Option<(Option<String>, Option<Operation>)>;

impl crate::ast::CondEval for RgbasmEval {
    fn eval(&self, head: &str, line: u32) -> Result<bool, AsmError> {
        let line = line as usize;
        let (word, args) = split_first_word(head.trim());
        let args = args.trim();
        if args.is_empty() {
            return Err(AsmError::new(line, format!("`{word}` needs a condition")));
        }
        let expr = bind_defined(value(args, line)?, &self.defined);
        let v = fold_const(&expr, &self.consts, line).map_err(|_| {
            AsmError::new(
                line,
                format!(
                    "`{args}` must be a constant here — rgbasm folds a condition against the \
                     values above it, and refuses a forward reference"
                ),
            )
        })?;
        Ok(v != 0)
    }

    /// `REPT n` names no loop variable; `FOR` does and is not adopted yet.
    fn iteration(&self, head: &str, line: u32) -> Result<crate::ast::Iteration, AsmError> {
        let line = line as usize;
        let (_, args) = split_first_word(head.trim());
        let n = fold_const(&value(args.trim(), line)?, &self.consts, line)?;
        Ok(crate::ast::Iteration::Times(n))
    }

    fn lower(&mut self, node: &Node, out: &mut Vec<Statement>) -> Result<(), AsmError> {
        let line = node.span.line as usize;
        if let Some((label, args)) = pc_ds_parts(&node.source) {
            let op = parse_live_ds(args, &self.consts, line)?;
            let label = label.map(|name| {
                if name.starts_with('.') && !self.global.is_empty() {
                    format!("{}{name}", self.global)
                } else {
                    self.global = name.to_string();
                    name.to_string()
                }
            });
            out.push(Statement {
                line,
                file: node.span.file,
                label,
                op: Some(op),
                operand_span: node.operand_span.clone(),
                xor_mask: 0,
                instruction_set: None,
                extension_set: None,
            });
            return Ok(());
        }
        if let Some((label, op)) = self.lower_rs(&node.source, line)? {
            out.push(Statement {
                line,
                file: node.span.file,
                label,
                op,
                operand_span: node.operand_span.clone(),
                xor_mask: 0,
                instruction_set: None,
                extension_set: None,
            });
            return Ok(());
        }
        if let Some(sym) = node.label.as_ref()
            && !sym.name.starts_with('.')
        {
            self.global = sym.qualified.clone();
        }
        if let Some(sym) = node.label.as_ref() {
            self.defined.insert(sym.qualified.clone());
        }
        let op = match &node.item {
            // Walk-handled: keep what it built rather than rebuilding it.
            Some(crate::ast::Item::Binary(_)) => Some(crate::ast::lower_item_ref(
                node.item.as_ref().expect("matched"),
            )?),
            Some(crate::ast::Item::Include { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `INCLUDE \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves includes automatically)"
                    ),
                ));
            }
            Some(crate::ast::Item::Incbin { request }) => {
                return Err(AsmError::at(
                    node.span.clone(),
                    format!(
                        "cannot resolve `INCBIN \"{request}\"` here — the single-source \
                         API assembles one file; use the multi-file entry point \
                         (the CLI resolves binary inclusions automatically)"
                    ),
                ));
            }
            // The walk made nothing of this line, so neither does assembly —
            // a bare `SECTION "a", ROM0` is the case, and re-parsing it would
            // answer `unknown instruction` for a line the reference accepts.
            None => None,
            Some(it) => match parse_op(
                self.set,
                &expand_rs_symbol(&node.source, self.rs),
                &self.consts,
                &self.global,
                line,
            ) {
                Ok(op) => op,
                // A directive the walk handled and the line parser cannot
                // rebuild — `SECTION "a", ROM0[0]`, which carries an origin.
                Err(e) => Some(crate::ast::lower_item_ref(it).map_err(|_| e)?),
            },
        };
        let op = op.map(|op| {
            crate::ast::map_syms(op, &mut |name| match name.strip_prefix(DEF_MARK) {
                Some(symbol) => Expr::Num(i64::from(self.defined.contains(symbol))),
                None => self
                    .consts
                    .get(&name)
                    .copied()
                    .map_or(Expr::Sym(name), Expr::Num),
            })
        });
        if let (Some(sym), Some(Operation::Equ(e))) = (node.label.as_ref(), &op)
            && let Ok(v) = fold_const(e, &self.consts, line)
        {
            self.consts.insert(sym.qualified.clone(), v);
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

fn parse_live_ds(
    args: &str,
    consts: &BTreeMap<String, i64>,
    line: usize,
) -> Result<Operation, AsmError> {
    let parts = split_top_level(args, ',');
    if parts.is_empty() || parts.len() > 2 {
        return Err(AsmError::new(line, "`ds` needs a count and optional fill"));
    }
    let count = bind_known(value(parts[0], line)?, consts);
    let fill = match parts.get(1) {
        None => 0,
        Some(v) => {
            let n = fold_const(&value(v, line)?, consts, line)?;
            u8::try_from(n & 0xFF).unwrap_or(0)
        }
    };
    Ok(Operation::Fill { count, value: fill })
}

fn bind_known(expr: Expr, consts: &BTreeMap<String, i64>) -> Expr {
    let one = |e: Box<Expr>| Box::new(bind_known(*e, consts));
    match expr {
        Expr::Sym(name) => consts
            .get(&name)
            .copied()
            .map_or(Expr::Sym(name), Expr::Num),
        Expr::Lo(e) => Expr::Lo(one(e)),
        Expr::Hi(e) => Expr::Hi(one(e)),
        Expr::Bank(e) => Expr::Bank(one(e)),
        Expr::Neg(e) => Expr::Neg(one(e)),
        Expr::BitNot(e) => Expr::BitNot(one(e)),
        Expr::FixRound(e) => Expr::FixRound(one(e)),
        Expr::TrailingZeros(e) => Expr::TrailingZeros(one(e)),
        Expr::LogNot(e) => Expr::LogNot(one(e)),
        Expr::Bin(op, left, right) => Expr::Bin(op, one(left), one(right)),
        Expr::Num(_) | Expr::Pc => expr,
    }
}

fn bind_defined(expr: Expr, defined: &BTreeSet<String>) -> Expr {
    let one = |e: Box<Expr>| Box::new(bind_defined(*e, defined));
    match expr {
        Expr::Sym(name) => match name.strip_prefix(DEF_MARK) {
            Some(symbol) => Expr::Num(i64::from(defined.contains(symbol))),
            None => Expr::Sym(name),
        },
        Expr::Lo(e) => Expr::Lo(one(e)),
        Expr::Hi(e) => Expr::Hi(one(e)),
        Expr::Bank(e) => Expr::Bank(one(e)),
        Expr::Neg(e) => Expr::Neg(one(e)),
        Expr::BitNot(e) => Expr::BitNot(one(e)),
        Expr::FixRound(e) => Expr::FixRound(one(e)),
        Expr::TrailingZeros(e) => Expr::TrailingZeros(one(e)),
        Expr::LogNot(e) => Expr::LogNot(one(e)),
        Expr::Bin(op, left, right) => Expr::Bin(op, one(left), one(right)),
        Expr::Num(_) | Expr::Pc => expr,
    }
}

impl RgbasmEval {
    fn lower_rs(&mut self, source: &str, line: usize) -> Result<LoweredRs, AsmError> {
        let (word, args) = split_first_word(source.trim());
        if word.eq_ignore_ascii_case("rsreset") {
            if !args.trim().is_empty() {
                return Err(AsmError::new(line, "`RSRESET` takes no arguments"));
            }
            self.set_rs(0);
            return Ok(Some((None, None)));
        }
        if word.eq_ignore_ascii_case("rsset") {
            let value = fold_const(&value(args.trim(), line)?, &self.consts, line)?;
            if value < 0 {
                return Err(AsmError::new(line, "RS counter must not be negative"));
            }
            self.set_rs(value);
            return Ok(Some((None, None)));
        }
        let Some((name, width, count)) = rs_definition(source) else {
            return Ok(None);
        };
        let count = if count.is_empty() {
            1
        } else {
            fold_const(&value(count, line)?, &self.consts, line)?
        };
        if count < 0 {
            return Err(AsmError::new(
                line,
                "an RS allocation count must not be negative",
            ));
        }
        let bound = self.rs;
        let advance = count
            .checked_mul(width)
            .and_then(|n| bound.checked_add(n))
            .ok_or_else(|| AsmError::new(line, "RS counter overflow"))?;
        self.consts.insert(name.to_string(), bound);
        self.defined.insert(name.to_string());
        self.set_rs(advance);
        Ok(Some((
            Some(name.to_string()),
            Some(Operation::Equ(Expr::Num(bound))),
        )))
    }

    fn set_rs(&mut self, value: i64) {
        self.rs = value;
        self.consts.insert("_RS".to_string(), value);
    }
}

/// `_RS` is a predefined value read at the point an expression is parsed, not
/// an ordinary final symbol. Substitute that exact token on live source lines;
/// quoted text and longer identifiers remain untouched.
fn expand_rs_symbol(source: &str, rs: i64) -> String {
    let bytes = source.as_bytes();
    let ident = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.');
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let mut quoted = false;
    let mut escaped = false;
    while i < bytes.len() {
        if bytes[i] == b'"' && !escaped {
            quoted = !quoted;
            out.push('"');
            i += 1;
            continue;
        }
        let before_symbol = i == 0 || !ident(bytes[i - 1]);
        let after = i + 3;
        let after_symbol = after >= bytes.len() || !ident(bytes[after]);
        if !quoted
            && before_symbol
            && after <= bytes.len()
            && &bytes[i..after] == b"_RS"
            && after_symbol
        {
            out.push_str(&rs.to_string());
            i = after;
        } else {
            out.push(bytes[i] as char);
            escaped = bytes[i] == b'\\' && !escaped;
            if bytes[i] != b'\\' {
                escaped = false;
            }
            i += 1;
        }
    }
    out
}
