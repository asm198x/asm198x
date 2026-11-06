//! What a vasm symbol is worth, and where it stands (#969).
//!
//! vasm's optimiser asks two questions of an operand: its value, and whether
//! that value moves when the sections load. Only an *absolute* value may
//! become `abs.w`, `moveq`, `addq` and the rest; a label in a relocatable
//! section may become PC-relative, and only from its own section. This module
//! answers both from the symbols as they stand at the point of a pass, the way
//! vasm's `eval_expr` and `find_base` do.
//!
//! Labels take their new value in place as a pass reaches them, so a backward
//! reference reads this pass's value and a forward one the last pass's. An
//! `equ` is evaluated when it is read, from whatever its own symbols hold then.
//! A name not yet defined (a forward reference while the source is first read)
//! has no value at all, which is how vasm sizes its first pass.

use std::collections::BTreeMap;

use crate::engine::{AsmError, BinOp, Expr};

/// How deep an `equ` may refer to other `equ`s before it is taken as circular.
const EQU_DEPTH: u32 = 64;

/// Where a value stands relative to the sections — vasm's `find_base`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Base {
    /// The same wherever the sections load: a number, an `equ` of one, a label
    /// in an `org` section, or the difference of two labels in one section.
    Abs,
    /// An offset into this section, fixed up when it loads.
    Rel(usize),
    /// Names a symbol that is not defined yet.
    Undefined,
    /// Anything else, such as the sum of two labels: not absolute, and not an
    /// offset into one section either.
    Complex,
}

/// A label: the section it is in and its offset there.
struct Label {
    sec: usize,
    pc: i64,
}

/// An `equ`: its expression, the section it was written in, and the location
/// counter there, which is what `*` in it reads.
struct Equ {
    expr: Expr,
    sec: usize,
    star: i64,
}

/// Every symbol a vasm program has defined so far.
pub(super) struct Syms {
    labels: BTreeMap<String, Label>,
    equs: BTreeMap<String, Equ>,
    /// Per section: is it an `org` section, whose labels are absolute?
    absolute: Vec<bool>,
}

impl Syms {
    /// No symbols yet, over sections whose kinds are already known.
    pub(super) fn new(absolute: Vec<bool>) -> Self {
        Self {
            labels: BTreeMap::new(),
            equs: BTreeMap::new(),
            absolute,
        }
    }

    /// Whether `sec` is an `org` section.
    pub(super) fn is_absolute(&self, sec: usize) -> bool {
        self.absolute.get(sec).copied().unwrap_or(false)
    }

    /// Place a label. Answers whether it moved, which keeps another pass
    /// coming; a label placed for the first time has moved.
    pub(super) fn place_label(&mut self, name: &str, sec: usize, pc: i64) -> bool {
        match self.labels.get_mut(name) {
            Some(label) if label.sec == sec && label.pc == pc => false,
            Some(label) => {
                label.sec = sec;
                label.pc = pc;
                true
            }
            None => {
                self.labels.insert(name.to_string(), Label { sec, pc });
                true
            }
        }
    }

    /// Define an `equ`, with `star` the location counter where it stands.
    /// Answers whether that moved, which matters when the expression reads it:
    /// vasm places a temporary label there, and a label that moves keeps
    /// another pass coming.
    pub(super) fn place_equ(&mut self, name: &str, expr: &Expr, sec: usize, star: i64) -> bool {
        match self.equs.get_mut(name) {
            Some(equ) if equ.sec == sec && equ.star == star => false,
            Some(equ) => {
                equ.sec = sec;
                equ.star = star;
                true
            }
            None => {
                self.equs.insert(
                    name.to_string(),
                    Equ {
                        expr: expr.clone(),
                        sec,
                        star,
                    },
                );
                true
            }
        }
    }

    /// The value of a symbol, if it has one yet.
    pub(super) fn value(&self, name: &str) -> Option<i64> {
        self.value_at(name, 0)
    }

    fn value_at(&self, name: &str, depth: u32) -> Option<i64> {
        if let Some(label) = self.labels.get(name) {
            return Some(label.pc);
        }
        let equ = self.equs.get(name)?;
        if depth >= EQU_DEPTH {
            return None;
        }
        equ.expr
            .eval_with(&|s| self.value_at(s, depth + 1), Some(equ.star), 0)
            .ok()
    }

    /// Evaluate `e` with `*` reading `here`.
    ///
    /// # Errors
    /// An undefined symbol, or arithmetic the expression cannot do.
    pub(super) fn eval(&self, e: &Expr, here: i64, line: usize) -> Result<i64, AsmError> {
        e.eval_with(&|s| self.value(s), Some(here), line)
    }

    /// Where `e` stands, read in section `sec` (which `*` belongs to).
    pub(super) fn base(&self, e: &Expr, sec: usize) -> Base {
        self.base_at(e, sec, 0)
    }

    fn base_at(&self, e: &Expr, sec: usize, depth: u32) -> Base {
        let located = |s: usize| {
            if self.is_absolute(s) {
                Base::Abs
            } else {
                Base::Rel(s)
            }
        };
        match e {
            Expr::Num(_) => Base::Abs,
            Expr::Pc => located(sec),
            Expr::Sym(name) => {
                if let Some(label) = self.labels.get(name) {
                    located(label.sec)
                } else if let Some(equ) = self.equs.get(name) {
                    if depth >= EQU_DEPTH {
                        Base::Undefined
                    } else {
                        self.base_at(&equ.expr, equ.sec, depth + 1)
                    }
                } else {
                    Base::Undefined
                }
            }
            Expr::Neg(x)
            | Expr::Lo(x)
            | Expr::Hi(x)
            | Expr::Bank(x)
            | Expr::BitNot(x)
            | Expr::LogNot(x)
            | Expr::FixRound(x)
            | Expr::TrailingZeros(x) => match self.base_at(x, sec, depth) {
                Base::Abs => Base::Abs,
                Base::Undefined => Base::Undefined,
                _ => Base::Complex,
            },
            Expr::Bin(op, l, r) => {
                let (l, r) = (self.base_at(l, sec, depth), self.base_at(r, sec, depth));
                match (op, l, r) {
                    (_, Base::Undefined, _) | (_, _, Base::Undefined) => Base::Undefined,
                    (_, Base::Abs, Base::Abs) => Base::Abs,
                    (BinOp::Add, Base::Rel(s), Base::Abs)
                    | (BinOp::Add, Base::Abs, Base::Rel(s)) => Base::Rel(s),
                    (BinOp::Sub, Base::Rel(s), Base::Abs) => Base::Rel(s),
                    (BinOp::Sub, Base::Rel(a), Base::Rel(b)) if a == b => Base::Abs,
                    _ => Base::Complex,
                }
            }
        }
    }

    /// Every symbol with a value, for the passes that read a finished table.
    pub(super) fn values(&self) -> BTreeMap<String, i64> {
        let mut out: BTreeMap<String, i64> = self
            .labels
            .iter()
            .map(|(name, label)| (name.clone(), label.pc))
            .collect();
        for name in self.equs.keys() {
            if let Some(v) = self.value(name) {
                out.insert(name.clone(), v);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(name: &str) -> Expr {
        Expr::Sym(name.to_string())
    }

    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr::Bin(op, Box::new(l), Box::new(r))
    }

    /// Section 0 relocatable, section 1 relocatable, section 2 an `org`.
    fn table() -> Syms {
        let mut syms = Syms::new(vec![false, false, true]);
        syms.place_label("a", 0, 4);
        syms.place_label("b", 0, 10);
        syms.place_label("d", 1, 2);
        syms.place_label("o", 2, 6);
        syms.place_equ("n", &Expr::Num(5), 0, 0);
        syms.place_equ("la", &sym("a"), 0, 0);
        syms
    }

    #[test]
    fn numbers_and_org_labels_are_absolute() {
        let syms = table();
        assert_eq!(syms.base(&Expr::Num(3), 0), Base::Abs);
        assert_eq!(syms.base(&sym("n"), 0), Base::Abs);
        assert_eq!(syms.base(&sym("o"), 0), Base::Abs);
        assert_eq!(syms.base(&Expr::Pc, 2), Base::Abs);
    }

    #[test]
    fn a_label_is_an_offset_into_its_section() {
        let syms = table();
        assert_eq!(syms.base(&sym("a"), 1), Base::Rel(0));
        assert_eq!(syms.base(&sym("la"), 1), Base::Rel(0));
        assert_eq!(
            syms.base(&bin(BinOp::Add, sym("a"), Expr::Num(2)), 0),
            Base::Rel(0)
        );
        assert_eq!(syms.base(&Expr::Pc, 1), Base::Rel(1));
        assert_eq!(syms.value("la"), Some(4));
    }

    #[test]
    fn a_difference_of_labels_in_one_section_is_absolute() {
        let syms = table();
        assert_eq!(
            syms.base(&bin(BinOp::Sub, sym("b"), sym("a")), 1),
            Base::Abs
        );
        assert_eq!(
            syms.base(&bin(BinOp::Sub, sym("b"), sym("d")), 1),
            Base::Complex
        );
        assert_eq!(
            syms.base(&bin(BinOp::Add, sym("b"), sym("a")), 1),
            Base::Complex
        );
        assert_eq!(
            syms.base(&bin(BinOp::Mul, sym("a"), Expr::Num(2)), 1),
            Base::Complex
        );
    }

    #[test]
    fn an_unknown_name_is_undefined_until_placed() {
        let mut syms = table();
        let later = bin(BinOp::Add, sym("later"), Expr::Num(1));
        assert_eq!(syms.base(&later, 0), Base::Undefined);
        assert!(syms.eval(&later, 0, 1).is_err());
        assert!(syms.place_label("later", 0, 20));
        assert!(!syms.place_label("later", 0, 20));
        assert_eq!(syms.base(&later, 0), Base::Rel(0));
        assert_eq!(syms.eval(&later, 0, 1).ok(), Some(21));
    }

    #[test]
    fn an_equ_reads_star_where_it_stands() {
        let mut syms = Syms::new(vec![false]);
        syms.place_label("start", 0, 0);
        let len = bin(BinOp::Sub, Expr::Pc, sym("start"));
        syms.place_equ("len", &len, 0, 12);
        assert_eq!(syms.value("len"), Some(12));
        assert_eq!(syms.base(&sym("len"), 0), Base::Abs);
        assert!(syms.place_equ("len", &len, 0, 14));
        assert_eq!(syms.value("len"), Some(14));
    }

    #[test]
    fn a_circular_equ_has_no_value() {
        let mut syms = Syms::new(vec![false]);
        syms.place_equ("x", &sym("y"), 0, 0);
        syms.place_equ("y", &sym("x"), 0, 0);
        assert_eq!(syms.value("x"), None);
        assert_eq!(syms.base(&sym("x"), 0), Base::Undefined);
    }
}
