//! The projection sweep (#486's first legibility split): the phase layer of
//! `decisions/parse-affecting-directives.md` and
//! `decisions/conditionals-in-multipass-dialects.md`. The structural parse in
//! the parent module read the tree eagerly; this sweep reads *meaning* —
//! folding each conditional and repetition once, in source order, and reading
//! every deferred line through the eager reader at the moment it is reached,
//! so the text environment, constants, scopes, and anonymous-label stream are
//! all positional in the order ca65's own single sequential reader would see
//! them. Moved verbatim from the parent; the seam is the boundary, not a
//! rewrite.

use super::super::mos6502;
use super::{Expr, Kind};

// Structural ca65 parsing keeps every physical body line, even blank ones.
// Elseif is a nested conditional sharing its parent's closer (empty close).

/// Substitute a `.repeat`'s loop variables into an expression.
///
/// The value has to be **baked in**, not left as a symbol, for the same reason
/// acme's `!for` variable is: ca65 resolves `Expr::Sym` once, in a later pass,
/// against one table — and a loop variable holds a different value on every
/// iteration, so there is no single entry that table could hold.
pub(super) fn map_expr_syms(e: &Expr, f: &dyn Fn(&str) -> Option<Expr>) -> Expr {
    match e {
        Expr::Sym(name) => f(name).unwrap_or_else(|| e.clone()),
        Expr::Lo(i) => Expr::Lo(Box::new(map_expr_syms(i, f))),
        Expr::Hi(i) => Expr::Hi(Box::new(map_expr_syms(i, f))),
        Expr::Bank(i) => Expr::Bank(Box::new(map_expr_syms(i, f))),
        Expr::Neg(i) => Expr::Neg(Box::new(map_expr_syms(i, f))),
        Expr::BitNot(i) => Expr::BitNot(Box::new(map_expr_syms(i, f))),
        Expr::LogNot(i) => Expr::LogNot(Box::new(map_expr_syms(i, f))),
        Expr::FixRound(i) => Expr::FixRound(Box::new(map_expr_syms(i, f))),
        Expr::TrailingZeros(i) => Expr::TrailingZeros(Box::new(map_expr_syms(i, f))),
        Expr::Bin(op, l, r) => Expr::Bin(
            *op,
            Box::new(map_expr_syms(l, f)),
            Box::new(map_expr_syms(r, f)),
        ),
        Expr::Num(_) | Expr::Pc => e.clone(),
    }
}

/// The same, over an operand.
fn map_operand_syms(
    o: &mos6502::OperandSyntax,
    f: &dyn Fn(&str) -> Option<Expr>,
) -> mos6502::OperandSyntax {
    use mos6502::OperandSyntax as O;
    let b = |e: &Expr| map_expr_syms(e, f);
    match o {
        O::None => O::None,
        O::Accumulator => O::Accumulator,
        O::Immediate(e) => O::Immediate(b(e)),
        O::Indirect(e) => O::Indirect(b(e)),
        O::IndexedIndirect(e) => O::IndexedIndirect(b(e)),
        O::IndirectIndexed(e) => O::IndirectIndexed(b(e)),
        O::IndirectIndexedZ(e) => O::IndirectIndexedZ(b(e)),
        O::StackIndirectIndexedY(e) => O::StackIndirectIndexedY(b(e)),
        O::Indexed(e, i) => O::Indexed(b(e), *i),
        O::Direct(e) => O::Direct(b(e)),
    }
}

/// The same, over a statement kind.
pub(super) fn map_kind_syms(k: &Kind, f: &dyn Fn(&str) -> Option<Expr>) -> Kind {
    let list = |es: &Vec<Expr>| es.iter().map(|e| map_expr_syms(e, f)).collect();
    match k {
        Kind::Unread => Kind::Unread,
        Kind::Bytes(es) => Kind::Bytes(list(es)),
        Kind::Words(es) => Kind::Words(list(es)),
        Kind::FarAddrs(es) => Kind::FarAddrs(list(es)),
        Kind::DBytes(es) => Kind::DBytes(list(es)),
        Kind::DWords(es) => Kind::DWords(list(es)),
        Kind::BitBranch {
            form,
            zero_page,
            target,
        } => Kind::BitBranch {
            form,
            zero_page: map_expr_syms(zero_page, f),
            target: map_expr_syms(target, f),
        },
        Kind::Insn {
            operand,
            address_size,
            mnemonic,
            extension,
        } => Kind::Insn {
            operand: map_operand_syms(operand, f),
            address_size: *address_size,
            mnemonic: mnemonic.clone(),
            extension: *extension,
        },
        Kind::Empty => Kind::Empty,
        Kind::Res(n, f) => Kind::Res(*n, *f),
        Kind::Constant(name, value) => Kind::Constant(name.clone(), *value),
        Kind::EnumValue(name, value) => Kind::EnumValue(name.clone(), map_expr_syms(value, f)),
        Kind::Org(e) => Kind::Org(map_expr_syms(e, f)),
        Kind::Reloc => Kind::Reloc,
        Kind::Align(m, f) => Kind::Align(*m, *f),
        Kind::Message(sev, t) => Kind::Message(*sev, t.clone()),
        Kind::Visible {
            rule,
            zero_page,
            names,
            define,
        } => Kind::Visible {
            rule: *rule,
            zero_page: *zero_page,
            names: names.clone(),
            define: define.as_ref().map(|e| map_expr_syms(e, f)),
        },
        Kind::Assert(c, fatal, m) => Kind::Assert(map_expr_syms(c, f), *fatal, m.clone()),
    }
}
