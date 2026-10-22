//! Property-based fuzzing of the front-end/back-end boundary.
//!
//! Every front end and the back end share these functions, so a wrong answer or
//! a panic here reaches every dialect. Each property states a rule the code
//! must keep for *all* inputs in its domain, checked against an independent
//! model where one exists: wide `i128` arithmetic for the evaluator, the
//! reference assemblers' documented behaviour for dialect-specific nodes, and
//! the serialized contract's own field rules.
//!
//! The default run is `proptest`'s 256 cases per property, which keeps CI fast.
//! For a fuzzing campaign, raise the count:
//!
//! ```text
//! PROPTEST_CASES=200000 cargo test -p asm198x-ir --test properties --release
//! ```
//!
//! A failure is shrunk to a minimal input and recorded under
//! `tests/properties.proptest-regressions`, which is committed so the case
//! replays first on every later run.

use std::collections::{BTreeMap, HashSet};

use asm198x_ir::{
    AsmError, BinOp, Code, ExpansionFrame, Expr, FIX_BITS, FileId, Operation, Span, Warning,
    WarningKind, align_pad, eval_binop, form, next_pc,
    source::{MemoryLoader, SourceLoader},
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Every binary operator. `model_binop`'s exhaustive match is the guard that a
/// new operator gets a model; add it here at the same time.
const BIN_OPS: [BinOp; 24] = [
    BinOp::Add,
    BinOp::Sub,
    BinOp::Mul,
    BinOp::Div,
    BinOp::And,
    BinOp::Or,
    BinOp::Xor,
    BinOp::Shl,
    BinOp::Shr,
    BinOp::Mod,
    BinOp::LogAnd,
    BinOp::LogOr,
    BinOp::LogXor,
    BinOp::Pow,
    BinOp::FixMul,
    BinOp::FixDiv,
    BinOp::Max,
    BinOp::Min,
    BinOp::Eq,
    BinOp::Ne,
    BinOp::Lt,
    BinOp::Gt,
    BinOp::Le,
    BinOp::Ge,
];

fn bin_op() -> impl Strategy<Value = BinOp> {
    proptest::sample::select(&BIN_OPS[..])
}

/// Integers biased towards the boundaries where arithmetic goes wrong.
fn edgy_i64() -> impl Strategy<Value = i64> {
    prop_oneof![
        4 => any::<i64>(),
        2 => -70_000i64..70_000,
        1 => prop::sample::select(vec![
            0,
            1,
            -1,
            2,
            -2,
            i64::MAX,
            i64::MIN,
            i64::MAX - 1,
            i64::MIN + 1,
            1 << 15,
            1 << 16,
            1 << 31,
            1 << 32,
            1 << 47,
            -(1 << 47),
            1 << 48,
            -(1 << 48),
            1 << 62,
            -(1 << 62),
            i64::from(u32::MAX),
            i64::from(i32::MIN),
        ]),
    ]
}

/// Symbol names: a few the resolver knows, and one it never does.
fn symbol() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["a", "b", "zero", "big", "undefined"]).prop_map(str::to_owned)
}

fn resolver(name: &str) -> Option<i64> {
    match name {
        "a" => Some(5),
        "b" => Some(-3),
        "zero" => Some(0),
        "big" => Some(i64::MAX),
        _ => None,
    }
}

/// Arbitrary expression trees, up to a few hundred nodes.
fn expr() -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![
        3 => edgy_i64().prop_map(Expr::Num),
        1 => symbol().prop_map(Expr::Sym),
        1 => Just(Expr::Pc),
    ];
    leaf.prop_recursive(6, 256, 2, |inner| {
        prop_oneof![
            inner.clone().prop_map(|e| Expr::Lo(Box::new(e))),
            inner.clone().prop_map(|e| Expr::Hi(Box::new(e))),
            inner.clone().prop_map(|e| Expr::Bank(Box::new(e))),
            inner.clone().prop_map(|e| Expr::Neg(Box::new(e))),
            inner.clone().prop_map(|e| Expr::BitNot(Box::new(e))),
            inner.clone().prop_map(|e| Expr::FixRound(Box::new(e))),
            inner.clone().prop_map(|e| Expr::TrailingZeros(Box::new(e))),
            inner.clone().prop_map(|e| Expr::LogNot(Box::new(e))),
            (bin_op(), inner.clone(), inner).prop_map(|(op, l, r)| Expr::Bin(
                op,
                Box::new(l),
                Box::new(r)
            )),
        ]
    })
}

fn file_id() -> impl Strategy<Value = FileId> {
    prop_oneof![Just(FileId(0)), any::<u32>().prop_map(FileId)]
}

fn warning_kind() -> impl Strategy<Value = WarningKind> {
    prop_oneof![Just(WarningKind::Advisory), Just(WarningKind::Note)]
}

fn span() -> impl Strategy<Value = Span> {
    let plain = (
        file_id(),
        any::<u32>(),
        any::<u32>(),
        proptest::option::of(".{0,24}"),
    )
        .prop_map(|(file, line, col, path)| {
            let mut span = Span::in_file(file, line, col);
            span.path = path;
            span
        });
    plain.prop_recursive(2, 8, 3, |inner| {
        (
            inner.clone(),
            prop::collection::vec(
                ("[a-z_]{1,12}", proptest::option::of(inner.clone()), inner),
                0..3,
            ),
        )
            .prop_map(|(mut span, frames)| {
                span.expansion_frames = frames
                    .into_iter()
                    .map(|(name, defined, invoked)| ExpansionFrame::new(name, defined, invoked))
                    .collect();
                span
            })
    })
}

// ---------------------------------------------------------------------------
// An independent model of the evaluator
// ---------------------------------------------------------------------------

fn fits(v: i128) -> Option<i64> {
    i64::try_from(v).ok()
}

/// What `eval_binop(op, a, b)` must answer, computed without `i64` arithmetic
/// wherever overflow is possible. `None` means the operation is an error.
fn model_binop(op: BinOp, a: i64, b: i64) -> Option<i64> {
    let (wa, wb) = (i128::from(a), i128::from(b));
    let truth = |x: bool| i64::from(x);
    match op {
        BinOp::Add => fits(wa + wb),
        BinOp::Sub => fits(wa - wb),
        BinOp::Mul => fits(wa * wb),
        BinOp::Div => (b != 0).then(|| fits(wa / wb)).flatten(),
        // C's sign rule, and `i64::MIN % -1` is refused rather than defined.
        BinOp::Mod => (b != 0 && !(a == i64::MIN && b == -1)).then(|| a % b),
        BinOp::And => Some(a & b),
        BinOp::Or => Some(a | b),
        BinOp::Xor => Some(a ^ b),
        // Shifts wrap their count to the register width (`b mod 64`); `Shr` is
        // arithmetic.
        BinOp::Shl => Some(((a as u64) << (b as u32 % 64)) as i64),
        BinOp::Shr => Some(a >> (b as u32 % 64)),
        BinOp::LogAnd => Some(truth(a != 0 && b != 0)),
        BinOp::LogOr => Some(truth(a != 0 || b != 0)),
        BinOp::LogXor => Some(truth((a != 0) != (b != 0))),
        BinOp::Pow => model_pow(a, b),
        // Never an error. This only checks dispatch; the arithmetic has its own
        // independent models (`fixed_point_*` below).
        BinOp::FixMul => Some(asm198x_ir::fix_mul(a, b)),
        BinOp::FixDiv => Some(asm198x_ir::fix_div(a, b)),
        BinOp::Max => Some(a.max(b)),
        BinOp::Min => Some(a.min(b)),
        BinOp::Eq => Some(truth(a == b)),
        BinOp::Ne => Some(truth(a != b)),
        BinOp::Lt => Some(truth(a < b)),
        BinOp::Gt => Some(truth(a > b)),
        BinOp::Le => Some(truth(a <= b)),
        BinOp::Ge => Some(truth(a >= b)),
    }
}

/// `a` to the power `b` by repeated wide multiplication. The exponent must fit
/// a `u32`.
fn model_pow(a: i64, b: i64) -> Option<i64> {
    let exp = u32::try_from(b).ok()?;
    match a {
        0 => Some(i64::from(exp == 0)),
        1 => Some(1),
        -1 => Some(if exp % 2 == 0 { 1 } else { -1 }),
        _ => {
            // |a| >= 2, so 64 multiplications already pass any i64.
            if exp > 64 {
                return None;
            }
            let mut acc: i128 = 1;
            for _ in 0..exp {
                acc = fits(acc * i128::from(a))?.into();
            }
            fits(acc)
        }
    }
}

/// `n / d` rounded to the nearest integer, halves away from zero, for `d > 0`.
fn round_half_away(n: i128, d: i128) -> i128 {
    let magnitude = (2 * n.abs() + d) / (2 * d);
    if n < 0 { -magnitude } else { magnitude }
}

/// Saturate to RGBDS's 32-bit range.
fn saturate(v: i128) -> i64 {
    v.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i64
}

const ONE: i128 = 1 << FIX_BITS;

/// A value as RGBDS holds it: the low 32 bits, signed.
fn as_int32(v: i64) -> i128 {
    i128::from(v as i32)
}

// ---------------------------------------------------------------------------
// The evaluator
// ---------------------------------------------------------------------------

proptest! {
    /// Every binary operator agrees with the wide-arithmetic model: the same
    /// value where the result exists, an error exactly where it does not.
    #[test]
    fn binop_matches_the_wide_model(op in bin_op(), a in edgy_i64(), b in edgy_i64()) {
        let got = eval_binop(op, a, b, 7);
        let want = model_binop(op, a, b);
        match (got, want) {
            (Ok(v), Some(w)) => prop_assert_eq!(v, w, "{:?}({}, {})", op, a, b),
            (Err(e), None) => prop_assert_eq!(e.line, 7),
            (got, want) => prop_assert!(
                false,
                "{:?}({}, {}): got {:?}, model says {:?}",
                op, a, b, got.map_err(|e| e.message), want
            ),
        }
    }

    /// Evaluation is total: any tree, any location counter, answers a value or
    /// an error and never panics. (`proptest` reports a panic as a failure.)
    #[test]
    fn evaluation_never_panics(e in expr(), pc in proptest::option::of(edgy_i64())) {
        let _ = e.eval_with(&resolver, pc, 1);
    }

    /// Evaluating twice gives the same answer.
    #[test]
    fn evaluation_is_deterministic(e in expr(), pc in edgy_i64()) {
        let first = e.eval_with(&resolver, Some(pc), 1).map_err(|e| e.message);
        let second = e.eval_with(&resolver, Some(pc), 1).map_err(|e| e.message);
        prop_assert_eq!(first, second);
    }

    /// `eval` over a symbol table is `eval_with` over the same lookup.
    #[test]
    fn eval_is_eval_with_over_a_table(e in expr(), pc in edgy_i64()) {
        let table: BTreeMap<String, i64> = ["a", "b", "zero", "big"]
            .into_iter()
            .filter_map(|n| resolver(n).map(|v| (n.to_owned(), v)))
            .collect();
        let via_table = e.eval(&table, pc, 1).map_err(|e| e.message);
        let via_fn = e.eval_with(&resolver, Some(pc), 1).map_err(|e| e.message);
        prop_assert_eq!(via_table, via_fn);
    }

    /// A binary node over two literals is exactly `eval_binop` on them.
    #[test]
    fn a_literal_node_is_eval_binop(op in bin_op(), a in edgy_i64(), b in edgy_i64()) {
        let node = Expr::Bin(op, Box::new(Expr::Num(a)), Box::new(Expr::Num(b)));
        let via_tree = node.eval_with(&resolver, None, 3).map_err(|e| e.message);
        let direct = eval_binop(op, a, b, 3).map_err(|e| e.message);
        prop_assert_eq!(via_tree, direct);
    }

    /// The byte selectors always answer a byte.
    #[test]
    fn byte_selectors_answer_a_byte(e in expr(), pc in edgy_i64()) {
        for wrap in [Expr::Lo, Expr::Hi, Expr::Bank] {
            if let Ok(v) = wrap(Box::new(e.clone())).eval_with(&resolver, Some(pc), 1) {
                prop_assert!((0..=0xFF).contains(&v), "{v}");
            }
        }
    }

    /// `Lo`/`Hi`/`Bank` select bits 0–7, 8–15 and 16–23.
    #[test]
    fn byte_selectors_select_the_right_byte(v in edgy_i64()) {
        let eval = |e: Expr| e.eval_with(&resolver, None, 1).ok();
        let num = || Box::new(Expr::Num(v));
        prop_assert_eq!(eval(Expr::Lo(num())), Some(v & 0xFF));
        prop_assert_eq!(eval(Expr::Hi(num())), Some((v >> 8) & 0xFF));
        prop_assert_eq!(eval(Expr::Bank(num())), Some((v >> 16) & 0xFF));
    }

    /// Logical and comparison results are truth values, 1 or 0.
    #[test]
    fn truth_valued_nodes_answer_0_or_1(e in expr(), f in expr()) {
        let truthy = [BinOp::LogAnd, BinOp::LogOr, BinOp::LogXor, BinOp::Eq, BinOp::Ne,
                      BinOp::Lt, BinOp::Gt, BinOp::Le, BinOp::Ge];
        for op in truthy {
            let node = Expr::Bin(op, Box::new(e.clone()), Box::new(f.clone()));
            if let Ok(v) = node.eval_with(&resolver, Some(0), 1) {
                prop_assert!(v == 0 || v == 1, "{op:?} answered {v}");
            }
        }
        if let Ok(v) = Expr::LogNot(Box::new(e)).eval_with(&resolver, Some(0), 1) {
            prop_assert!(v == 0 || v == 1);
        }
    }

    /// `&&` and `||` short-circuit, so a decided left side protects a right
    /// side that could not be evaluated (a definedness guard).
    #[test]
    fn logical_operators_short_circuit(nonzero in edgy_i64().prop_filter("non-zero", |v| *v != 0)) {
        let undefined = || Box::new(Expr::Sym("undefined".into()));
        let and = Expr::Bin(BinOp::LogAnd, Box::new(Expr::Num(0)), undefined());
        let or = Expr::Bin(BinOp::LogOr, Box::new(Expr::Num(nonzero)), undefined());
        prop_assert_eq!(and.eval_with(&resolver, None, 1).ok(), Some(0));
        prop_assert_eq!(or.eval_with(&resolver, None, 1).ok(), Some(1));
    }

    /// Negation is checked: every value negates except `i64::MIN`.
    #[test]
    fn negation_is_checked(v in edgy_i64()) {
        let got = Expr::Neg(Box::new(Expr::Num(v))).eval_with(&resolver, None, 1).ok();
        prop_assert_eq!(got, v.checked_neg());
    }

    /// rgbasm's `ROUND` is the nearest whole number, halves away from zero,
    /// saturated to 32 bits (RGBDS `fix_Round`), of the operand read as RGBDS
    /// holds it: the low 32 bits, signed. Every such value is exact as a
    /// `double`, so the model is exact everywhere.
    #[test]
    fn fixed_point_round_matches_rgbds(v in edgy_i64()) {
        let got = Expr::FixRound(Box::new(Expr::Num(v)))
            .eval_with(&resolver, None, 1)
            .expect("ROUND is never an error");
        let want = saturate(round_half_away(as_int32(v), ONE) * ONE);
        prop_assert_eq!(got, want, "ROUND({})", v);
    }

    /// rgbasm's `MUL` is the exact product of the two 32-bit operands rounded
    /// to the nearest Q16.16 value, halves away from zero, saturated to 32 bits
    /// (RGBDS `fix_Mul`). Exact while both operands are below 2^26, where the
    /// `double` product is exact; beyond it, still a 32-bit value.
    #[test]
    fn fixed_point_mul_matches_rgbds(a in edgy_i64(), b in edgy_i64()) {
        let got = eval_binop(BinOp::FixMul, a, b, 1).expect("MUL is never an error");
        prop_assert!(i32::try_from(got).is_ok(), "MUL({a}, {b}) = {got}");
        let (a32, b32) = (as_int32(a), as_int32(b));
        if a32.abs() < 1 << 26 && b32.abs() < 1 << 26 {
            let want = saturate(round_half_away(a32 * b32, ONE));
            prop_assert_eq!(got, want, "MUL({}, {})", a, b);
        }
    }

    /// rgbasm's `DIV` is the exact quotient of the two 32-bit operands rounded
    /// to the nearest Q16.16 value, saturated to 32 bits; a zero divisor
    /// saturates by the dividend's sign (RGBDS `fix_Div`). Exact for divisors
    /// below 2^20, where the `double` quotient cannot round across a half.
    #[test]
    fn fixed_point_div_matches_rgbds(a in edgy_i64(), b in edgy_i64()) {
        let got = eval_binop(BinOp::FixDiv, a, b, 1).expect("DIV is never an error");
        prop_assert!(i32::try_from(got).is_ok(), "DIV({a}, {b}) = {got}");
        let (a32, b32) = (as_int32(a), as_int32(b));
        if b32 == 0 {
            let want = match a32.signum() {
                -1 => i64::from(i32::MIN),
                1 => i64::from(i32::MAX),
                _ => 0,
            };
            prop_assert_eq!(got, want, "DIV({}, {})", a, b);
        } else if b32.abs() < 1 << 20 {
            let n = a32 * ONE * b32.signum();
            let want = saturate(round_half_away(n, b32.abs()));
            prop_assert_eq!(got, want, "DIV({}, {})", a, b);
        }
    }

    /// rgbasm's `TZCOUNT` counts the trailing zero bits of a 32-bit value, so
    /// `TZCOUNT(0)` is 32 (RGBDS `op_tzcount`). Modelled bit by bit.
    #[test]
    fn trailing_zeros_matches_rgbds(v in edgy_i64()) {
        let got = Expr::TrailingZeros(Box::new(Expr::Num(v))).eval_with(&resolver, None, 1).ok();
        let want = (0..32).take_while(|bit| v >> bit & 1 == 0).count() as i64;
        prop_assert_eq!(got, Some(want));
    }

    /// The location counter is a value only where one is meaningful.
    #[test]
    fn the_location_counter_needs_a_context(pc in edgy_i64()) {
        prop_assert_eq!(Expr::Pc.eval_with(&resolver, Some(pc), 1).ok(), Some(pc));
        prop_assert!(Expr::Pc.eval_with(&resolver, None, 1).is_err());
    }

    /// An undefined symbol is an error that names it.
    #[test]
    fn an_undefined_symbol_is_named(name in "[A-Za-z_][A-Za-z0-9_]{0,15}") {
        prop_assume!(resolver(&name).is_none());
        let error = Expr::Sym(name.clone())
            .eval_with(&resolver, None, 9)
            .expect_err("an undefined symbol cannot evaluate");
        prop_assert!(error.message.contains(&format!("`{name}`")), "{}", error.message);
        prop_assert_eq!(error.line, 9);
    }
}

/// RGBDS v1.0.3's own assertions for these functions (`test/asm/math.asm` and
/// `test/asm/bit-functions.asm`), as written there. Q16.16 literals are
/// spelled as their raw values.
#[test]
fn rgbds_own_assertions_hold() {
    let q = |v: f64| (v * 65536.0) as i64;
    let (int_max, int_min) = (i64::from(i32::MAX), i64::from(i32::MIN));
    assert_eq!(asm198x_ir::fix_div(q(5.0), q(2.0)), q(2.5));
    assert_eq!(asm198x_ir::fix_div(q(-5.0), q(2.0)), q(-2.5));
    assert_eq!(asm198x_ir::fix_div(q(5.0), q(0.0)), int_max); // +inf
    assert_eq!(asm198x_ir::fix_div(q(-5.0), q(0.0)), int_min); // -inf
    assert_eq!(asm198x_ir::fix_div(q(0.0), q(0.0)), 0); // NaN
    assert_eq!(asm198x_ir::fix_mul(q(10.0), q(0.5)), q(5.0));
    assert_eq!(asm198x_ir::fix_mul(q(10.0), q(0.0)), q(0.0));
    assert_eq!(asm198x_ir::fix_round(q(1.5)), q(2.0));
    assert_eq!(asm198x_ir::fix_round(q(-1.5)), q(-2.0));
    assert_eq!(asm198x_ir::trailing_zeros(0), 32);
    assert_eq!(asm198x_ir::trailing_zeros(42), 1);
    assert_eq!(asm198x_ir::trailing_zeros(-1), 0);
    assert_eq!(asm198x_ir::trailing_zeros(0x8000_0000), 31);
    assert_eq!(asm198x_ir::trailing_zeros(q(1.0)), 16);
}

// ---------------------------------------------------------------------------
// Layout arithmetic
// ---------------------------------------------------------------------------

proptest! {
    /// The padding to the next multiple of `modulus` is in `0..modulus` and
    /// lands exactly on a multiple, for every counter value.
    #[test]
    fn align_pad_reaches_the_next_multiple(pc in edgy_i64(), modulus in 2i64..=1 << 20) {
        let pad = align_pad(pc, modulus);
        prop_assert!((0..modulus).contains(&pad), "{pad}");
        prop_assert_eq!((i128::from(pc) + i128::from(pad)).rem_euclid(i128::from(modulus)), 0);
    }

    /// A modulus of 1 or less has no boundary to reach.
    #[test]
    fn align_pad_without_a_boundary_is_zero(pc in edgy_i64(), modulus in i64::MIN..=1) {
        prop_assert_eq!(align_pad(pc, modulus), 0);
    }

    /// Data directives advance the counter by their size in address units.
    #[test]
    fn data_advances_by_its_size(
        pc in 0i64..=i64::from(u32::MAX),
        n in 0usize..512,
        unit in prop::sample::select(vec![1i64, 2, 4]),
    ) {
        let set = &isa::mos6502::SET;
        let bytes = Operation::Bytes(vec![Expr::Num(0); n]);
        let words = Operation::Words(vec![Expr::Num(0); n]);
        let binary = Operation::Binary(vec![0; n]);
        let len = n as i64;
        prop_assert_eq!(next_pc(&bytes, pc, set, None, unit, 1).ok(), Some(pc + len / unit));
        prop_assert_eq!(next_pc(&words, pc, set, None, unit, 1).ok(), Some(pc + 2 * len / unit));
        prop_assert_eq!(
            next_pc(&binary, pc, set, None, unit, 1).ok(),
            Some(pc + n.div_ceil(unit as usize) as i64)
        );
        prop_assert_eq!(next_pc(&Operation::Reserve(n), pc, set, None, 1, 1).ok(), Some(pc + len));
    }

    /// A fill advances by its count; a negative count is refused.
    #[test]
    fn fill_advances_by_its_count(pc in 0i64..=i64::from(u32::MAX), count in -70_000i64..70_000) {
        let fill = Operation::Fill { count: Expr::Num(count), value: 0 };
        let got = next_pc(&fill, pc, &isa::mos6502::SET, None, 1, 1).ok();
        prop_assert_eq!(got, (count >= 0).then_some(pc + count));
    }

    /// An instruction advances by the size of the form it was resolved to.
    #[test]
    fn an_instruction_advances_by_its_form(pc in 0i64..=i64::from(u32::MAX), pick in any::<prop::sample::Index>()) {
        let set = &isa::mos6502::SET;
        let forms: Vec<_> = set
            .instructions
            .iter()
            .flat_map(|i| i.forms.iter().map(move |f| (i.mnemonic, f)))
            .collect();
        let (mnemonic, f) = forms[pick.index(forms.len())];
        let op = Operation::Instruction { mnemonic: mnemonic.to_owned(), mode: f.mode, operands: vec![] };
        prop_assert_eq!(next_pc(&op, pc, set, None, 1, 1).ok(), Some(pc + f.len() as i64));
    }

    /// Operations that speak, name or bind emit nothing and leave the counter.
    #[test]
    fn non_emitting_operations_leave_the_counter(pc in 0i64..=i64::from(u32::MAX), v in edgy_i64()) {
        let set = &isa::mos6502::SET;
        for op in [
            Operation::Org(Expr::Num(v)),
            Operation::Equ(Expr::Num(v)),
            Operation::Set(Expr::Num(v)),
            Operation::Entry(Expr::Num(v)),
            Operation::InitMem(v as u8),
            Operation::PseudoPc(Some(Expr::Num(v))),
            Operation::Unsupported("x".into()),
        ] {
            prop_assert_eq!(next_pc(&op, pc, set, None, 1, 1).ok(), Some(pc));
        }
    }
}

// ---------------------------------------------------------------------------
// Instruction lookup
// ---------------------------------------------------------------------------

/// Instruction sets the front ends resolve against, base and extension.
fn sets() -> Vec<&'static isa::InstructionSet> {
    vec![
        &isa::mos6502::SET,
        &isa::mos65c02::SET,
        &isa::mos65816::SET,
        &isa::huc6280::SET,
        &isa::csg65ce02::SET,
        &isa::c64dtv2::SET,
        &isa::nmos6502_undocumented::SET,
        &isa::z80::SET,
        &isa::z80::NEXT,
        &isa::sm83::SET,
        &isa::i8080::SET,
        &isa::m6800::SET,
        &isa::cdp1802::SET,
        &isa::i8048::SET,
        &isa::scmp::SET,
        &isa::f8::SET,
        &isa::s2650::SET,
        &isa::tms7000::SET,
    ]
}

/// Every form in every set is found by its own mnemonic and mode. Exhaustive
/// rather than sampled: the forms are finite and this is the contract front
/// ends rely on.
#[test]
fn every_form_is_found_by_its_mnemonic_and_mode() {
    let mut checked = 0;
    for set in sets() {
        for instruction in set.instructions {
            for f in instruction.forms {
                let found = form(set, None, instruction.mnemonic, f.mode, 1).unwrap_or_else(|e| {
                    panic!("{} {} {}: {}", set.cpu, instruction.mnemonic, f.mode, e)
                });
                assert_eq!(found.mode, f.mode);
                let direct = set
                    .find_form(instruction.mnemonic, f.mode)
                    .expect("the spec finds its own form");
                assert!(std::ptr::eq(found, direct));
                checked += 1;
            }
        }
    }
    assert!(checked > 1000, "only {checked} forms checked");
}

/// An extension's instructions resolve through the extension and not without
/// it.
#[test]
fn extension_instructions_need_their_extension() {
    let base = &isa::mos6502::SET;
    let ext = &isa::mos65c02::WDC_SET;
    assert!(form(base, Some(ext), "WAI", "implied", 1).is_ok());
    let Err(error) = form(base, None, "WAI", "implied", 1) else {
        panic!("WAI must need the WDC extension");
    };
    assert!(
        error.message.contains("unknown instruction `WAI`"),
        "{}",
        error.message
    );
}

proptest! {
    /// A mnemonic no set knows is reported as unknown, naming it.
    #[test]
    fn an_unknown_mnemonic_is_named(mnemonic in "[A-Z]{1,7}", mode in "[a-z,()#]{1,12}") {
        for set in sets().into_iter().filter(|set| !set.has_mnemonic(&mnemonic)) {
            let Err(error) = form(set, None, &mnemonic, &mode, 4) else {
                panic!("{mnemonic} is not a {} mnemonic", set.cpu);
            };
            prop_assert_eq!(error.message, format!("unknown instruction `{mnemonic}`"));
            prop_assert_eq!(error.line, 4);
        }
    }

    /// A known mnemonic in a mode it lacks is reported as that, not as unknown.
    #[test]
    fn a_missing_mode_is_named(pick in any::<prop::sample::Index>(), mode in "[a-z]{1,4}[0-9]{2}") {
        let set = &isa::mos6502::SET;
        let instruction = &set.instructions[pick.index(set.instructions.len())];
        prop_assume!(set.find_form(instruction.mnemonic, &mode).is_none());
        let Err(error) = form(set, None, instruction.mnemonic, &mode, 1) else {
            panic!("{} has no {mode} form", instruction.mnemonic);
        };
        prop_assert_eq!(
            error.message,
            format!("`{}` has no {mode} addressing mode", instruction.mnemonic)
        );
    }
}

// ---------------------------------------------------------------------------
// Diagnostics and the serialized contract
// ---------------------------------------------------------------------------

/// Every code has a unique wire name that looks itself up, a unique book page,
/// and an explanation that starts with its own name as a heading.
#[test]
fn codes_are_a_consistent_registry() {
    let mut names = HashSet::new();
    let mut pages = HashSet::new();
    for &code in Code::ALL {
        assert!(
            names.insert(code.as_str()),
            "duplicate name {}",
            code.as_str()
        );
        assert!(
            pages.insert(code.page_slug()),
            "duplicate page {}",
            code.page_slug()
        );
        assert_eq!(Code::from_name(code.as_str()), Some(code));
        assert!(
            code.explanation()
                .starts_with(&format!("# {}\n", code.as_str())),
            "{} explanation must open with its name",
            code.as_str()
        );
    }
}

/// A new error is the catch-all code with no span; `with_code` changes only
/// the code.
#[test]
fn errors_default_to_the_catch_all_code() {
    let error = AsmError::new(3, "boom");
    assert_eq!(error.code, Code::AssemblyError);
    assert!(error.span.is_none());
    let classified = error.clone().with_code(Code::BranchOutOfRange);
    assert_eq!(classified.code, Code::BranchOutOfRange);
    assert_eq!(
        (classified.line, classified.message),
        (error.line, error.message)
    );
}

proptest! {
    /// Only exact registry names look up a code.
    #[test]
    fn only_registry_names_are_codes(name in "[A-Za-z]{0,24}") {
        let known = Code::ALL.iter().any(|c| c.as_str() == name);
        prop_assert_eq!(Code::from_name(&name).is_some(), known);
    }

    /// The warning builder sets exactly what it is given.
    #[test]
    fn the_warning_builder_sets_its_fields(
        line in any::<usize>(),
        message in ".{0,40}",
        file in file_id(),
        kind in warning_kind(),
    ) {
        let w = Warning::new(line, message.clone()).in_file(file).with_kind(kind);
        prop_assert_eq!(w.line, line);
        prop_assert_eq!(&w.message, &message);
        prop_assert_eq!(w.file, file);
        prop_assert_eq!(w.kind, kind);
        let plain = Warning::new(line, message);
        prop_assert_eq!((plain.file, plain.kind), (FileId(0), WarningKind::Advisory));
    }

    /// A warning survives JSON unchanged, and its additive fields are written
    /// only when they differ from the defaults older readers assume
    /// (`core-contract-freeze.md`).
    #[test]
    fn warnings_round_trip_and_omit_their_defaults(
        line in any::<usize>(),
        message in ".{0,40}",
        file in file_id(),
        kind in warning_kind(),
    ) {
        let w = Warning::new(line, message).in_file(file).with_kind(kind);
        let json = serde_json::to_value(&w).expect("a warning serializes");
        let object = json.as_object().expect("a warning is a JSON object");
        prop_assert_eq!(object.contains_key("file"), file != FileId(0));
        prop_assert_eq!(object.contains_key("kind"), kind != WarningKind::Advisory);
        let back: Warning = serde_json::from_value(json).expect("a written warning reads back");
        prop_assert_eq!(back, w);
    }

    /// A span, with any nesting of expansion frames, survives JSON unchanged,
    /// and an absent path is not written.
    #[test]
    fn spans_round_trip(span in span()) {
        let json = serde_json::to_value(&span).expect("a span serializes");
        prop_assert_eq!(json.get("path").is_some(), span.path.is_some());
        let back: Span = serde_json::from_value(json).expect("a written span reads back");
        prop_assert_eq!(back, span);
    }

    /// The in-memory loader returns exactly what was registered, under the
    /// requested name, and refuses anything else.
    #[test]
    fn the_memory_loader_returns_what_it_holds(
        path in "[a-z/]{1,16}\\.(asm|inc)",
        contents in ".{0,200}",
        bytes in prop::collection::vec(any::<u8>(), 0..64),
        other in "[a-z]{1,8}\\.bin",
    ) {
        let loader = MemoryLoader::new().text(path.clone(), contents.clone()).binary(other.clone(), bytes.clone());
        prop_assert_eq!(
            loader.load_text(&path, None).expect("a registered text loads"),
            (path.clone(), contents)
        );
        prop_assert_eq!(loader.resolve_text(&path, None), Some(path.clone()));
        prop_assert_eq!(loader.load_binary(&other, None).expect("a registered binary loads"), bytes);
        let missing = loader
            .load_text(&other, Some(&path))
            .expect_err("an unregistered name does not load");
        prop_assert_eq!(missing.request, other);
        prop_assert_eq!(missing.from, Some(path));
    }
}
