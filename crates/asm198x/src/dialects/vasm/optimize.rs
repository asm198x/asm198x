//! vasm's 68000 optimiser, applied exactly (#969).
//!
//! `vasmm68k_mot` optimises by default, and the set it applies is documented
//! in its manual (`doc/cpu_m68k.texi`, "Optimizations") and implemented in its
//! M68k backend (`cpus/m68k/cpu.c`: `optimize_oper`, `optimize_instruction`).
//! For a 68000 with no `-opt-*` option, this is that set — no more, because
//! improving on the reference is a mismatch too (`decisions/reference-parity-
//! goal.md`).
//!
//! [`optimize`] takes a source instruction as written and gives the
//! instructions to encode in its place: none (deleted), one, or two. Every
//! addressing mode in them is settled, so sizing and encoding read the same
//! answer and cannot disagree. It runs again on every pass from the source
//! instruction, as vasm's does on a fresh copy, and branch and `jmp` sizing
//! also read the size the instruction had last pass.
//!
//! With the optimiser off (`-no-opt`) it changes nothing but the spelling:
//! condition aliases, `adda`/`suba`/`cmpa`, and the immediate instructions an
//! immediate into memory selects.

use isa::m68k::{self, EaModes, Size, Slot, ea};

use super::resolve::{Base, Syms};
use super::{
    Opnd, canonical_condition_alias, immediate_form, match_form, reglist_mask, unknown_or_unmatched,
};
use crate::engine::{AsmError, BinOp, Expr, Warning};

/// One instruction ready to encode, every addressing mode settled.
///
/// An unsized absolute ([`Opnd::Abs`]) in an effective-address slot is
/// `(xxx).L`, vasm's default; the optimiser turns it into `(xxx).W` or
/// `(d16,PC)` where it may. In a branch slot it is the target.
#[derive(Clone)]
pub(super) struct Concrete {
    pub(super) mnemonic: String,
    pub(super) size: Option<Size>,
    pub(super) operands: Vec<Opnd>,
}

/// What an instruction becomes.
pub(super) struct Optimized {
    pub(super) insns: Vec<Concrete>,
    /// Keep the size the instruction had last pass, rather than this one's: a
    /// branch out of range becomes a `jmp` but goes on being sized as the
    /// word branch it replaced (vasm's `IFL_RETAINLASTSIZE`).
    pub(super) retain_last_size: bool,
}

/// What the optimiser reads besides the instruction.
pub(super) struct Env<'a> {
    pub(super) syms: &'a Syms,
    /// The section the instruction is in.
    pub(super) sec: usize,
    /// vasm's optimiser is on: not `-no-opt`.
    pub(super) optimize: bool,
    pub(super) line: usize,
}

/// A value as vasm's optimiser reads it: 32 bits, signed (`taddr`), and
/// where it stands.
#[derive(Clone, Copy)]
struct Value {
    val: i64,
    base: Base,
}

/// The value of an expression at `pc`. A name not yet defined reads 0, as an
/// undefined import does in vasm, and its base says so.
fn value(e: &Expr, env: &Env, pc: i64) -> Value {
    let base = env.syms.base(e, env.sec);
    let val = env.syms.eval(e, pc, env.line).map_or(0, wrap32);
    Value { val, base }
}

/// A value as a 32-bit signed number, the width vasm's M68k backend computes
/// in: `$ffffffff` is -1 and `$ffff8000` is -32768.
fn wrap32(v: i64) -> i64 {
    i64::from(v as i32)
}

fn fits16(v: i64) -> bool {
    (-0x8000..=0x7fff).contains(&v)
}

/// The expression an operand carries first: vasm's `extval[0]`.
fn operand_expr(op: &Opnd) -> Option<&Expr> {
    match op {
        Opnd::Imm(e) | Opnd::Abs(e) | Opnd::AbsW(e) | Opnd::AbsL(e) => Some(e),
        Opnd::Mem { disp: Some(e), .. } => Some(e),
        Opnd::Idx { disp, .. } => Some(disp),
        _ => None,
    }
}

/// The first operand's value. An operand with no expression reads as an
/// absolute dummy that matches no rule, as vasm's does.
fn first_value(ops: &[Opnd], env: &Env, pc: i64) -> Value {
    match ops.first().and_then(operand_expr) {
        Some(e) => value(e, env, pc),
        None => Value {
            val: 0x7fff_ffff,
            base: Base::Abs,
        },
    }
}

/// Whether a value with `base` may be reached PC-relative from this section:
/// any absolute value in an `org` section, else a label in this one.
fn same_section(base: Base, env: &Env) -> bool {
    if env.syms.is_absolute(env.sec) {
        base == Base::Abs
    } else {
        base == Base::Rel(env.sec)
    }
}

fn concrete(mnemonic: &str, size: Option<Size>, operands: Vec<Opnd>) -> Concrete {
    Concrete {
        mnemonic: mnemonic.to_string(),
        size,
        operands,
    }
}

fn imm(v: i64) -> Opnd {
    Opnd::Imm(Expr::Num(v))
}

fn pc_relative(e: &Expr) -> Opnd {
    Opnd::Mem {
        mode: 7,
        reg: 2,
        bit: ea::PCD,
        disp: Some(e.clone()),
    }
}

fn indirect(reg: u8) -> Opnd {
    Opnd::Mem {
        mode: 2,
        reg,
        bit: ea::AI,
        disp: None,
    }
}

/// The spelling vasm would have chosen for this instruction, optimiser or
/// not: canonical condition codes, `adda`/`suba`/`cmpa` as their base
/// mnemonic with an address-register destination, `eor` of an immediate as
/// `eori`, and an immediate into memory as the immediate instruction.
fn spelled<'a>(mnemonic: &'a str, operands: &[Opnd]) -> &'a str {
    let mnemonic = canonical_condition_alias(mnemonic);
    let mnemonic = match (mnemonic, operands.last()) {
        ("ADDA", Some(Opnd::AReg(_))) => "ADD",
        ("SUBA", Some(Opnd::AReg(_))) => "SUB",
        ("CMPA", Some(Opnd::AReg(_))) => "CMP",
        _ => mnemonic,
    };
    // eor's source is always a data register, so an immediate one is always
    // `eori`, into a register or memory alike.
    if let [Opnd::Imm(_), dest] = operands
        && mnemonic == "EOR"
        && !matches!(dest, Opnd::AReg(_) | Opnd::Imm(_))
    {
        return "EORI";
    }
    immediate_form(mnemonic, operands)
}

/// The form an instruction encodes with, or why it has none.
fn form_of(
    mnemonic: &str,
    operands: &[Opnd],
    line: usize,
) -> Result<&'static m68k::Form, AsmError> {
    let insn = m68k::SET
        .instruction(mnemonic)
        .ok_or_else(|| unknown_or_unmatched(mnemonic, line))?;
    match_form(insn, operands)
        .ok_or_else(|| AsmError::new(line, format!("`{mnemonic}` has no form for those operands")))
}

/// vasm's `optimize_oper` for one 68000 operand: `(0,An)` to `(An)`, an
/// absolute `abs.l` to `abs.w`, and otherwise a same-section address to
/// `(d16,PC)`. A written size (`.w`/`.l`) is never touched, and neither is
/// anything but an unsized absolute or a displacement.
fn optimize_operand(op: &mut Opnd, modes: EaModes, env: &Env, pc: i64) {
    let cpc = pc + 2;
    match op {
        Opnd::Mem {
            mode: 5,
            reg,
            disp: Some(e),
            ..
        } => {
            let v = value(e, env, pc);
            if v.base == Base::Abs && v.val == 0 && modes.allows(ea::AI) {
                *op = indirect(*reg);
            }
        }
        Opnd::Abs(e) => {
            let v = value(e, env, pc);
            if v.base == Base::Abs && fits16(v.val) && modes.allows(ea::AW) {
                *op = Opnd::AbsW(e.clone());
            } else if modes.allows(ea::PCD) && same_section(v.base, env) && fits16(v.val - cpc) {
                *op = pc_relative(e);
            }
        }
        _ => {}
    }
}

/// Optimise every effective-address operand of `c`, against its own form.
fn optimize_operands(c: &mut Concrete, env: &Env, pc: i64) -> Result<(), AsmError> {
    let form = form_of(&c.mnemonic, &c.operands, env.line)?;
    for (slot, op) in form.operands.iter().zip(c.operands.iter_mut()) {
        if let Slot::Ea { modes, .. } = slot {
            optimize_operand(op, *modes, env, pc);
        }
    }
    Ok(())
}

/// The condition a branch is taken on, negated: `beq` for `bne`.
fn negated_branch(mnemonic: &str) -> Option<&'static str> {
    Some(match mnemonic {
        "BEQ" => "BNE",
        "BNE" => "BEQ",
        "BCC" => "BCS",
        "BCS" => "BCC",
        "BHI" => "BLS",
        "BLS" => "BHI",
        "BVC" => "BVS",
        "BVS" => "BVC",
        "BPL" => "BMI",
        "BMI" => "BPL",
        "BGE" => "BLT",
        "BLT" => "BGE",
        "BGT" => "BLE",
        "BLE" => "BGT",
        _ => return None,
    })
}

/// Whether an operand is `(An)+` or `-(An)` (`first..=last` of vasm's modes)
/// on an address register in `list`.
fn indirect_in_list(op: &Opnd, modes: std::ops::RangeInclusive<u8>, list: u16) -> bool {
    let reg = match op {
        Opnd::Mem { mode, reg, .. } if modes.contains(mode) => *reg,
        Opnd::Idx { bit, reg, .. } if *bit == ea::IX && modes.contains(&6) => *reg,
        _ => return false,
    };
    list & (1 << (8 + u16::from(reg))) != 0
}

/// Apply vasm's default 68000 optimisations to one instruction at `pc`.
///
/// `last_size` is the size the instruction had last pass (-1 before the
/// first), which branch and `jmp` sizing read; `resolve_warn` is set for an
/// instruction that has changed size too often, as vasm sets
/// `RESOLVE_WARN`. `warnings` is given on the final pass only, to report the
/// translations vasm warns about.
///
/// # Errors
/// An unknown instruction, or operands it has no form for.
#[allow(clippy::too_many_arguments)]
pub(super) fn optimize(
    mnemonic: &str,
    size: Option<Size>,
    operands: &[Opnd],
    env: &Env,
    pc: i64,
    last_size: i32,
    resolve_warn: bool,
    warnings: Option<&mut Vec<Warning>>,
) -> Result<Optimized, AsmError> {
    let (out, note) = rules(mnemonic, size, operands, env, pc, last_size, resolve_warn)?;
    if let (Some(warnings), Some(note)) = (warnings, note) {
        warnings.push(Warning::new(env.line, note));
    }
    Ok(out)
}

/// Nothing in an instruction's place.
fn deleted() -> Optimized {
    Optimized {
        insns: Vec::new(),
        retain_last_size: false,
    }
}

fn one(c: Concrete) -> Optimized {
    Optimized {
        insns: vec![c],
        retain_last_size: false,
    }
}

fn two(first: Concrete, second: Concrete) -> Optimized {
    Optimized {
        insns: vec![first, second],
        retain_last_size: false,
    }
}

/// [`optimize`]'s rules, with the warning vasm gives for a translation it had
/// to make.
#[allow(clippy::too_many_lines)]
fn rules(
    mnemonic: &str,
    size: Option<Size>,
    operands: &[Opnd],
    env: &Env,
    pc: i64,
    last_size: i32,
    resolve_warn: bool,
) -> Result<(Optimized, Option<&'static str>), AsmError> {
    let mnemonic = spelled(mnemonic, operands);
    let source_form = form_of(mnemonic, operands, env.line)?;
    if !env.optimize {
        return Ok((one(concrete(mnemonic, size, operands.to_vec())), None));
    }

    let cpc = pc + 2;
    let ext = size.unwrap_or(Size::W);
    let mut ops = operands.to_vec();
    // jmp/jsr (label,pc) is never optimised, so jump tables keep their shape.
    let written_pc =
        matches!(mnemonic, "JMP" | "JSR") && matches!(ops.first(), Some(Opnd::Mem { mode: 7, .. }));
    if !written_pc {
        for (slot, op) in source_form.operands.iter().zip(ops.iter_mut()) {
            if let Slot::Ea { modes, .. } = slot {
                optimize_operand(op, *modes, env, pc);
            }
        }
    }

    let Value { val, base } = first_value(&ops, env, pc);
    let abs = base == Base::Abs;
    let pcrelok = same_section(base, env);
    let sized = |m: &str, ops: Vec<Opnd>| concrete(m, size, ops);

    // Stage 2 of vasm's `optimize_instruction`: at most one rule applies.
    let rewritten: Option<Optimized> = match (mnemonic, ops.as_slice()) {
        // move.l #x,Dn into moveq, alone or with add.w or swap.
        ("MOVE" | "MOVEA", [Opnd::Imm(_), Opnd::DReg(n)]) if abs && ext == Size::L => {
            let n = *n;
            if (-0x80..=0x7f).contains(&val) {
                Some(one(concrete(
                    "MOVEQ",
                    Some(Size::L),
                    vec![imm(val), Opnd::DReg(n)],
                )))
            } else if ((0x80..=0xfe).contains(&val) || (-0x100..=-0x82).contains(&val))
                && val & 1 == 0
            {
                Some(two(
                    concrete("MOVEQ", Some(Size::L), vec![imm(val >> 1), Opnd::DReg(n)]),
                    concrete("ADD", Some(Size::W), vec![Opnd::DReg(n), Opnd::DReg(n)]),
                ))
            } else if (val & 0xffff == 0 && (0x1_0000..=0x7f_0000).contains(&val))
                || (val & 0xffff == 0xffff
                    && (0xff80_ffff_u32..=0xfffe_ffff).contains(&(val as u32)))
            {
                Some(two(
                    concrete("MOVEQ", Some(Size::L), vec![imm(val >> 16), Opnd::DReg(n)]),
                    concrete("SWAP", None, vec![Opnd::DReg(n)]),
                ))
            } else {
                None
            }
        }
        // movea #0,An into suba.l An,An; movea.l #label,An into lea.
        ("MOVE" | "MOVEA", [Opnd::Imm(e), Opnd::AReg(n)]) => {
            if abs && val == 0 {
                Some(one(concrete(
                    "SUB",
                    Some(Size::L),
                    vec![Opnd::AReg(*n), Opnd::AReg(*n)],
                )))
            } else if !abs && ext == Size::L {
                Some(one(concrete(
                    "LEA",
                    None,
                    vec![Opnd::Abs(e.clone()), Opnd::AReg(*n)],
                )))
            } else {
                None
            }
        }
        // movem loading address registers only: one becomes a move, two from
        // (An)+ become two moves.
        ("MOVEM", [ea_op, list_op])
            if matches!(source_form.operands.get(1), Some(Slot::RegList)) =>
        {
            let list = reglist_mask(list_op);
            let regs = list.count_ones();
            let address_only = list & 0xff == 0;
            if regs == 1 && address_only && !indirect_in_list(ea_op, 3..=4, list) {
                let a = (list.trailing_zeros() - 8) as u8;
                Some(one(sized("MOVE", vec![ea_op.clone(), Opnd::AReg(a)])))
            } else if regs == 2
                && address_only
                && matches!(ea_op, Opnd::Mem { mode: 3 | 4, .. })
                && !indirect_in_list(ea_op, 2..=6, list)
            {
                let first = list.trailing_zeros();
                let second = (list & !(1 << first)).trailing_zeros();
                Some(two(
                    sized("MOVE", vec![ea_op.clone(), Opnd::AReg((first - 8) as u8)]),
                    sized("MOVE", vec![ea_op.clone(), Opnd::AReg((second - 8) as u8)]),
                ))
            } else {
                None
            }
        }
        // clr.l Dn into moveq #0,Dn.
        ("CLR", [Opnd::DReg(n)]) if ext == Size::L => Some(one(concrete(
            "MOVEQ",
            Some(Size::L),
            vec![imm(0), Opnd::DReg(*n)],
        ))),
        // and of every bit at the operation's size is a test.
        ("AND", [Opnd::Imm(_), dest @ Opnd::DReg(_)]) | ("ANDI", [Opnd::Imm(_), dest])
            if abs && !matches!(dest, Opnd::Ccr | Opnd::Sr) =>
        {
            let every = match ext {
                Size::B => val == 0xff,
                Size::W => val == 0xffff,
                Size::L => val == -1,
            };
            every.then(|| one(sized("TST", vec![dest.clone()])))
        }
        // or, ori and eori of zero are a test; eori of every bit is not.
        ("OR", [Opnd::Imm(_), dest @ Opnd::DReg(_)]) | ("ORI" | "EORI", [Opnd::Imm(_), dest])
            if abs && !matches!(dest, Opnd::Ccr | Opnd::Sr) =>
        {
            let every = match ext {
                Size::B => val & 0xff == 0xff,
                Size::W => val & 0xffff == 0xffff,
                Size::L => val == -1,
            };
            if val == 0 {
                Some(one(sized("TST", vec![dest.clone()])))
            } else if mnemonic == "EORI" && every {
                Some(one(sized("NOT", vec![dest.clone()])))
            } else {
                None
            }
        }
        // add/sub of an immediate: the quick form, and for an address
        // register, deletion, the opposite quick form, or lea.
        ("ADD" | "ADDI" | "SUB" | "SUBI", [Opnd::Imm(_), dest]) if abs => {
            let sub = mnemonic.starts_with("SUB");
            if (1..=8).contains(&val) && !(matches!(dest, Opnd::AReg(_)) && ext == Size::B) {
                let quick = if sub { "SUBQ" } else { "ADDQ" };
                Some(one(sized(quick, vec![imm(val), dest.clone()])))
            } else if let Opnd::AReg(n) = dest {
                let v = if sub { -val } else { val };
                if v == 0 {
                    Some(deleted())
                } else if (-8..=-1).contains(&v) {
                    let quick = if sub { "ADDQ" } else { "SUBQ" };
                    Some(one(sized(quick, vec![imm(-v), Opnd::AReg(*n)])))
                } else if fits16(v) {
                    let disp = Opnd::Mem {
                        mode: 5,
                        reg: *n,
                        bit: ea::DI,
                        disp: Some(Expr::Num(v)),
                    };
                    Some(one(concrete("LEA", None, vec![disp, Opnd::AReg(*n)])))
                } else {
                    None
                }
            } else {
                None
            }
        }
        ("LEA", [src, Opnd::AReg(n)]) => {
            let n = *n;
            match src {
                Opnd::Mem {
                    mode: 5, reg, disp, ..
                } if abs => {
                    let same = *reg == n;
                    if same && val == 0 {
                        Some(deleted())
                    } else if same && val != 0 && (-8..=8).contains(&val) {
                        let quick = if val < 0 { "SUBQ" } else { "ADDQ" };
                        Some(one(concrete(
                            quick,
                            Some(Size::L),
                            vec![imm(val.abs()), Opnd::AReg(n)],
                        )))
                    } else if !fits16(val) {
                        // A 68000 `lea` holds 16 bits of displacement.
                        let d = Opnd::Imm(disp.clone().unwrap_or(Expr::Num(0)));
                        let add = concrete("ADD", Some(Size::L), vec![d, Opnd::AReg(n)]);
                        let out = if same {
                            one(add)
                        } else {
                            let movea = concrete(
                                "MOVE",
                                Some(Size::L),
                                vec![Opnd::AReg(*reg), Opnd::AReg(n)],
                            );
                            two(movea, add)
                        };
                        return Ok((
                            out,
                            Some("lea-displacement out of range, changed into move/add"),
                        ));
                    } else {
                        None
                    }
                }
                Opnd::Mem { mode: 2, reg, .. } if *reg == n => Some(deleted()),
                Opnd::Abs(_) | Opnd::AbsW(_) | Opnd::AbsL(_) if abs && val == 0 => Some(one(
                    concrete("SUB", Some(Size::L), vec![Opnd::AReg(n), Opnd::AReg(n)]),
                )),
                _ => None,
            }
        }
        // cmp of zero is a test, except into an address register, which a
        // 68000's tst cannot address.
        ("CMP" | "CMPI", [Opnd::Imm(_), dest])
            if abs && val == 0 && !matches!(dest, Opnd::AReg(_)) =>
        {
            Some(one(sized("TST", vec![dest.clone()])))
        }
        // asl #1,Dn into add Dn,Dn.
        ("ASL", [Opnd::Imm(_), Opnd::DReg(n)]) if val == 1 => {
            Some(one(sized("ADD", vec![Opnd::DReg(*n), Opnd::DReg(*n)])))
        }
        // jmp and jsr to a label in reach become bra and bsr; a jmp to the
        // next instruction goes.
        ("JMP" | "JSR", [target])
            if pcrelok
                && !written_pc
                && matches!(operands.first(), Some(Opnd::Abs(_)))
                && matches!(target, Opnd::Abs(_) | Opnd::Mem { mode: 7, .. }) =>
        {
            let e = operand_expr(target).cloned().unwrap_or(Expr::Num(0));
            let diff = val - cpc;
            if last_size == 0 || (diff == 0 && mnemonic == "JMP") {
                Some(deleted())
            } else if fits16(diff) {
                let short = (-0x80..=0x7f).contains(&diff)
                    && !((last_size == 2 && diff == 0) || (last_size == 4 && diff == 2));
                let branch = if mnemonic == "JMP" { "BRA" } else { "BSR" };
                let size = if short { Size::B } else { Size::W };
                Some(one(concrete(branch, Some(size), vec![Opnd::Abs(e)])))
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some(rewritten) = rewritten {
        return Ok((finish(rewritten, env, pc)?, None));
    }

    // Branches, sized from their last size when the source left it open.
    if source_form
        .operands
        .iter()
        .any(|s| matches!(s, Slot::BranchW))
        && let Some(target) = operand_expr(&ops[0]).cloned()
    {
        let bsr = mnemonic == "BSR";
        let diff = val - cpc;
        if size.is_none() && pcrelok {
            let decided = match last_size {
                0 if diff != -2 => Some(Size::B),
                0 => None,
                2 if diff == 0 && !bsr && !resolve_warn => None,
                2 if !(-128..128).contains(&diff) || diff == 0 || diff == -130 => Some(Size::W),
                2 => Some(Size::B),
                4 if diff == 2 && !bsr && !resolve_warn => None,
                4 if diff == 2 => Some(Size::W),
                4 if (-128..=128).contains(&diff) && diff != -130 && !resolve_warn => Some(Size::B),
                4 if !fits16(diff) => {
                    // Out of a 68000 branch's reach: bra and bsr become jmp
                    // and jsr, and bcc a reversed short branch over one.
                    let out = match mnemonic {
                        "BRA" => one(concrete("JMP", None, vec![Opnd::Abs(target)])),
                        "BSR" => one(concrete("JSR", None, vec![Opnd::Abs(target)])),
                        cc => {
                            let reversed = negated_branch(cc).unwrap_or(cc);
                            let over =
                                Expr::Bin(BinOp::Add, Box::new(Expr::Pc), Box::new(Expr::Num(8)));
                            two(
                                concrete(reversed, Some(Size::B), vec![Opnd::Abs(over)]),
                                concrete("JMP", None, vec![Opnd::AbsL(target)]),
                            )
                        }
                    };
                    let mut out = finish(out, env, pc)?;
                    out.retain_last_size = true;
                    return Ok((out, Some("branch out of range changed to jmp")));
                }
                _ => Some(Size::W),
            };
            let out = match decided {
                Some(size) => one(concrete(mnemonic, Some(size), ops)),
                None => deleted(),
            };
            return Ok((out, None));
        }
        if !bsr && pcrelok && diff == 0 && size == Some(Size::B) {
            // A written short branch to the next instruction cannot be
            // encoded; vasm puts a two-byte no-op in its place, `lea (a6),a6`.
            return Ok((
                one(concrete("LEA", None, vec![indirect(6), Opnd::AReg(6)])),
                Some("short-branch to following instruction turned into a nop"),
            ));
        }
        return Ok((one(concrete(mnemonic, size, ops)), None));
    }

    // `<op>.l #x,An` holds a 16-bit value as a word, sign-extended.
    let mut size = size;
    if abs
        && ext == Size::L
        && matches!(mnemonic, "MOVE" | "MOVEA" | "ADD" | "SUB" | "CMP")
        && matches!(ops.as_slice(), [Opnd::Imm(_), Opnd::AReg(_)])
        && fits16(val)
    {
        size = Some(Size::W);
    }
    Ok((finish(one(concrete(mnemonic, size, ops)), env, pc)?, None))
}

/// vasm optimises the operands once more after rewriting an instruction, so
/// a `movea.l #label` turned `lea label` can go on to `lea label(pc)`.
fn finish(mut out: Optimized, env: &Env, pc: i64) -> Result<Optimized, AsmError> {
    for c in &mut out.insns {
        optimize_operands(c, env, pc)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use crate::dialects::vasm::assemble_warned;

    /// Past 32K a 68000 branch cannot reach, so vasm makes `bra` and `bsr` a
    /// `jmp` and `jsr`, and `bcc` a reversed short branch over a `jmp`, and
    /// warns. Too big to record as probes, so pinned here: the bytes are vasm
    /// 2.0f's (`-Fbin`), whose `jmp` stays `abs.l` after a `bcc`.
    #[test]
    fn a_branch_out_of_reach_becomes_a_jump() {
        let (bytes, warnings) =
            assemble_warned(" beq l\n ds.b 40000\nl: nop\n").expect("forward bcc");
        assert_eq!(bytes.len(), 40010);
        assert_eq!(bytes[..8], [0x66, 0x06, 0x4E, 0xF9, 0x00, 0x00, 0x9C, 0x48]);
        assert_eq!(warnings.len(), 1);

        let (bytes, warnings) =
            assemble_warned(" bra l\n bsr l\n ds.b 40000\nl: nop\n").expect("bra and bsr");
        assert_eq!(bytes.len(), 40014);
        assert_eq!(
            bytes[..12],
            [
                0x4E, 0xF9, 0x00, 0x00, 0x9C, 0x4C, 0x4E, 0xB9, 0x00, 0x00, 0x9C, 0x4C
            ]
        );
        assert_eq!(warnings.len(), 2);

        let (bytes, _) = assemble_warned("l: nop\n ds.b 40000\n bne l\n").expect("backward bcc");
        assert_eq!(bytes.len(), 40010);
        assert_eq!(
            bytes[40002..],
            [0x67, 0x06, 0x4E, 0xF9, 0x00, 0x00, 0x00, 0x00]
        );
    }

    /// A written branch size is kept, so a written `bra.w` out of reach is
    /// still refused rather than turned into a `jmp`.
    #[test]
    fn a_written_branch_out_of_reach_is_refused() {
        assert!(assemble_warned(" bra.w l\n ds.b 40000\nl: nop\n").is_err());
    }
}
