//! The dialect-agnostic assembler engine.
//!
//! Everything here is independent of any one CPU or source dialect: the
//! two-pass driver, the symbol table, expression evaluation, the directive
//! semantics (origin, bytes, words), and byte emission. A [`Dialect`]
//! front-end parses source into the generic [`Statement`] stream this engine
//! consumes — resolving each instruction's addressing mode against its target
//! [`isa::InstructionSet`] at parse time, so instruction *size* never depends
//! on a (possibly forward) symbol value. The engine then lays bytes down using
//! only the shared spec. CPU/dialect knowledge stays in the front-end — see
//! [`crate::dialect`] and [`crate::dialects`].

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::dialect::{Dialect, Oversize};
use crate::span::Span;

/// The result of a successful assembly: where it loads and the bytes to load.
#[derive(Debug, Clone)]
pub struct Assembly {
    /// Load address (first origin directive, or 0 if none given).
    pub origin: u16,
    /// Assembled machine code, contiguous from `origin`.
    pub bytes: Vec<u8>,
    /// Resolved labels, for listings and debugging. Values are `i64` to hold
    /// the 65816's 24-bit addresses and bank constants; 8-/16-bit CPUs use the
    /// low bits only.
    pub symbols: BTreeMap<String, i64>,
    /// The program's entry point, if an `end <addr>` directive gave one. Used by
    /// containers that carry a start address (a Spectrum `.sna`); `None` for a
    /// plain flat binary.
    pub start: Option<u16>,
    /// Non-fatal advisories raised during assembly (e.g. a byte truncated to fit
    /// its operand, sjasmplus-style). Empty for dialects that don't warn.
    pub warnings: Vec<Warning>,
    /// Debug-info captured during pass 2 — the line→address map and typed
    /// symbols the CLI renders into a `.dbg198x` sidecar / `--sym` / `--listing`.
    /// Header-less (the CPU/dialect/source-file identity is the CLI's to add) and
    /// section-less (the flat engine is a single implicit section 0, based at
    /// `origin`). Capturing it never changes an emitted byte.
    pub debug: DebugData,
}

/// The engine's slice of a `.dbg198x` record: typed symbols and line→address
/// spans, in the CPU's **address units** (a decle for the word-addressed CP1610,
/// a byte elsewhere) so a consumer's address lookups line up with the CPU's own
/// addressing. Header-less; the CLI wraps it with identity and the source
/// filename to form a full [`dbg198x::DebugInfo`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugData {
    /// Every label (address), `equ`/`=` constant (value), and entry point.
    pub symbols: Vec<dbg198x::Symbol>,
    /// One span per source-bearing statement that emitted bytes. Fill from `org`
    /// gaps and `align` carries no span (the padding rule).
    pub lines: Vec<LineRec>,
}

/// A line→address span before the source filename is attached: `length` address
/// units at section-relative `offset` were produced by `line`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRec {
    pub line: u32,
    pub offset: u64,
    pub length: u64,
}

/// An assembly error, with the 1-based source line it occurred on (0 = no
/// specific line).
#[derive(Debug, Clone)]
pub struct AsmError {
    pub line: usize,
    pub message: String,
    /// The source span, when the raising site knows a column-level position (the
    /// AST-routed dialects, once U3 wires them). `None` for the line-only sites,
    /// where the diagnostic is line-granular. Per contract KTD1 the span rides
    /// this engine error path — not the AST — so every CPU inherits diagnostics,
    /// and column accuracy improves as CPUs adopt the AST.
    pub span: Option<Span>,
}

impl AsmError {
    pub(crate) fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
            span: None,
        }
    }

    /// An error carrying a source span. `line` mirrors the span's line so the
    /// `Display` impl and existing `.line` readers keep working unchanged.
    // The AST-routed dialects call this from U3 (populate real columns); U2
    // builds the seam and covers it by test. Reserved until then.
    #[allow(dead_code)]
    pub(crate) fn at(span: Span, message: impl Into<String>) -> Self {
        Self {
            line: span.line as usize,
            message: message.into(),
            span: Some(span),
        }
    }
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for AsmError {}

/// A non-fatal assembly advisory, with the 1-based source line it applies to
/// (0 = no specific line). Reference assemblers assemble *and* flag questionable
/// source (e.g. an immediate too wide for its operand); a `Warning` carries that
/// signal without failing the assembly. The bytes are still produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub line: usize,
    pub message: String,
}

impl Warning {
    pub(crate) fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "warning: {}", self.message)
        } else {
            write!(f, "line {}: warning: {}", self.line, self.message)
        }
    }
}

// ---------------------------------------------------------------------------
// Expressions — the shared engine IR
// ---------------------------------------------------------------------------

/// A binary arithmetic operator. The dialect parser is responsible for
/// precedence (it builds the tree); the engine only evaluates.
#[derive(Debug, Clone, Copy)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    /// Bitwise AND/OR/XOR and left/right shift (vasm `&` `|` `^` `<<` `>>`).
    And,
    Or,
    Xor,
    Shl,
    Shr,
    /// Exponentiation (ACME's `^`): `a` raised to the power `b`.
    Pow,
}

/// An expression in the shared engine IR. Each dialect parses its own operator
/// syntax into this tree; the engine evaluates it. The tree stays dialect-
/// agnostic: a `<`/`>` operator and a `low()`/`high()` function both lower to
/// [`Expr::Lo`]/[`Expr::Hi`], and any dialect's `+`/`-`/`*`/`/` lower to
/// [`Expr::Bin`].
#[derive(Debug, Clone)]
pub(crate) enum Expr {
    Num(i64),
    Sym(String),
    /// The current location counter (`$` in pasmo/sjasmplus) — the address of
    /// the statement being assembled.
    Pc,
    /// Low byte of the inner value.
    Lo(Box<Expr>),
    /// High byte of the inner value.
    Hi(Box<Expr>),
    /// Bank byte (bits 16–23) of the inner value — the 65816 `^` operator.
    Bank(Box<Expr>),
    /// Negation of the inner value.
    Neg(Box<Expr>),
    /// A binary operation on two sub-expressions.
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Evaluate against the engine's `u16` symbol table, with `pc` the address
    /// of the current statement. A thin wrapper over [`Expr::eval_with`], the
    /// single evaluator shared by every dialect.
    pub(crate) fn eval(
        &self,
        symbols: &BTreeMap<String, i64>,
        pc: i64,
        line: usize,
    ) -> Result<i64, AsmError> {
        self.eval_with(&|s| symbols.get(s).copied(), Some(pc), line)
    }

    /// The one expression evaluator, shared by the engine and every dialect.
    /// `resolve` returns a symbol's value or `None` if it's unknown (or not a
    /// constant). `pc` is `Some` where the location counter (`*`/`$`) is
    /// meaningful, `None` in parse-time-constant contexts (where `*` is an
    /// error). Keeping this the only evaluator means a new operator or rule is
    /// added in exactly one place.
    pub(crate) fn eval_with(
        &self,
        resolve: &impl Fn(&str) -> Option<i64>,
        pc: Option<i64>,
        line: usize,
    ) -> Result<i64, AsmError> {
        Ok(match self {
            Expr::Num(n) => *n,
            Expr::Pc => pc.ok_or_else(|| AsmError::new(line, "`*` cannot be used here"))?,
            Expr::Sym(s) => {
                resolve(s).ok_or_else(|| AsmError::new(line, format!("undefined symbol `{s}`")))?
            }
            Expr::Lo(e) => e.eval_with(resolve, pc, line)? & 0xFF,
            Expr::Hi(e) => (e.eval_with(resolve, pc, line)? >> 8) & 0xFF,
            Expr::Bank(e) => (e.eval_with(resolve, pc, line)? >> 16) & 0xFF,
            Expr::Neg(e) => e
                .eval_with(resolve, pc, line)?
                .checked_neg()
                .ok_or_else(|| AsmError::new(line, "arithmetic overflow in expression"))?,
            Expr::Bin(op, l, r) => {
                let a = l.eval_with(resolve, pc, line)?;
                let b = r.eval_with(resolve, pc, line)?;
                eval_binop(*op, a, b, line)?
            }
        })
    }
}

/// Evaluate one binary operator — the single place each operator's semantics
/// live (shifts wrap; `/` checks for zero; `+ - *` check for overflow).
pub(crate) fn eval_binop(op: BinOp, a: i64, b: i64, line: usize) -> Result<i64, AsmError> {
    let overflow = || AsmError::new(line, "arithmetic overflow in expression");
    Ok(match op {
        BinOp::Add => a.checked_add(b).ok_or_else(overflow)?,
        BinOp::Sub => a.checked_sub(b).ok_or_else(overflow)?,
        BinOp::Mul => a.checked_mul(b).ok_or_else(overflow)?,
        BinOp::Div if b == 0 => return Err(AsmError::new(line, "division by zero in expression")),
        BinOp::Div => a.checked_div(b).ok_or_else(overflow)?,
        BinOp::And => a & b,
        BinOp::Or => a | b,
        BinOp::Xor => a ^ b,
        BinOp::Shl => a.wrapping_shl(b as u32),
        BinOp::Shr => a.wrapping_shr(b as u32),
        BinOp::Pow => {
            let exp = u32::try_from(b)
                .map_err(|_| AsmError::new(line, "negative exponent in expression"))?;
            a.checked_pow(exp).ok_or_else(overflow)?
        }
    })
}

// ---------------------------------------------------------------------------
// The generic statement stream a dialect produces
// ---------------------------------------------------------------------------

/// One operation, with its addressing mode already resolved by the dialect.
pub(crate) enum Operation {
    /// Set the program counter (the `.org`/`org` directive).
    Org(Expr),
    /// Define the statement's label as a constant value rather than the PC
    /// (the `equ`/`=` directive). The statement must carry a label.
    Equ(Expr),
    /// Emit one byte per expression.
    Bytes(Vec<Expr>),
    /// Emit one word per expression, in the instruction set's endianness.
    Words(Vec<Expr>),
    /// An instruction whose form the dialect has already chosen by `mode`.
    /// `operands` carries one value per operand byte-slot the form declares, in
    /// order (empty for operand-less forms; two for e.g. Z80 `LD (IX+d),n`).
    Instruction {
        mnemonic: String,
        mode: &'static str,
        operands: Vec<Expr>,
    },
    /// An instruction the dialect has encoded itself into a sequence of
    /// [`Piece`]s — literal bytes it computed (opcode, a 6809 postbyte, later an
    /// 8086 modrm) interleaved with sized values resolved in pass 2. The general
    /// seam for CPUs whose operands are computed, not fixed-width slots; the
    /// dialect still reuses this engine's two-pass driver, symbols, and `org`.
    Encoded(Vec<Piece>),
    /// Record the program's entry point (the `end <addr>` directive). Emits no
    /// bytes; surfaced on [`Assembly::start`] for containers that carry a start
    /// address (e.g. a Spectrum `.sna` snapshot). A flat binary ignores it.
    Entry(Expr),
    /// Advance the program counter to the next address where `pc & andmask ==
    /// value`, filling the gap with `fill` (ACME's `!align andmask, value
    /// [, fill]`). The pad count is PC-dependent, so it is resolved in the
    /// engine passes; `andmask`/`value`/`fill` are folded to constants by the
    /// dialect. The pad is `(value - pc) & andmask`.
    Align { andmask: i64, value: i64, fill: u8 },
}

/// One piece of a dialect-computed instruction encoding.
pub(crate) enum Piece {
    /// A byte the dialect already determined (opcode, postbyte, modrm…).
    Lit(u8),
    /// A value laid down at `bytes` width (big-/little-endian per the CPU),
    /// resolved in pass 2. `rel` makes it a branch offset from the following
    /// address; `signed` range-checks it as signed (an index displacement).
    Val {
        expr: Expr,
        bytes: u8,
        rel: bool,
        signed: bool,
    },
    /// A value packed into `bytes` bytes (in the CPU's endianness), resolved in
    /// pass 2. `expr` carries the raw (possibly `Pc`-relative) value. It is first
    /// divided by `scale` — which must divide it exactly, else a range error
    /// (the PDP-11's word-scaled branch, whose byte distance must be even —
    /// `asl`'s "jump distance is odd"); `scale` of 1 is the plain case. The
    /// scaled value is range-checked against `min..=max`, then masked to `mask`
    /// and OR-ed with `or_bits`. So the check sees the real number before the low
    /// bits are masked out and the high mode flags are set. This is the 2650's
    /// relative / page-zero / absolute operand (low bits a displacement or
    /// address, high bits indirect and index-control flags) and the PDP-11's
    /// word-scaled branch / `SOB` offset. `what` names the field in the error.
    Packed {
        expr: Expr,
        bytes: u8,
        scale: i64,
        min: i64,
        max: i64,
        mask: u32,
        or_bits: u32,
        what: &'static str,
    },
    /// A two-word relative branch whose opcode word carries a **direction bit**
    /// selected by the *sign* of the displacement, with the magnitude in the
    /// following word — the CP1610 (Intellivision) branch shape, which the linear
    /// [`Piece::Packed`] can't express. `target` is the destination address (in
    /// the CPU's address units); `base` is the opcode word (direction bit clear).
    /// The signed displacement `d` is `target` minus the address two words past
    /// the opcode (the branch is two words long). Forward (`d >= 0`): opcode
    /// `base`, magnitude `d`. Backward: opcode `base | dir_bit`, magnitude
    /// `-d - 1`. Both words are laid down in the CPU's endianness; `what` names the
    /// field in a range error.
    Branch {
        target: Expr,
        base: u16,
        dir_bit: u16,
        what: &'static str,
    },
}

impl Piece {
    fn len(&self) -> i64 {
        match self {
            Piece::Lit(_) => 1,
            Piece::Val { bytes, .. } => i64::from(*bytes),
            Piece::Packed { bytes, .. } => i64::from(*bytes),
            // Two 16-bit words: the opcode word plus the magnitude.
            Piece::Branch { .. } => 4,
        }
    }
}

/// One source line, reduced to an optional label and an optional operation.
pub(crate) struct Statement {
    pub(crate) line: usize,
    pub(crate) label: Option<String>,
    pub(crate) op: Option<Operation>,
}

// ---------------------------------------------------------------------------
// The two-pass driver
// ---------------------------------------------------------------------------

/// Assemble `source` with `dialect` into a flat binary.
///
/// Two passes: pass one assigns addresses to labels; pass two emits bytes with
/// labels resolved. The dialect has already resolved each instruction's mode,
/// so form sizes are stable between the passes.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub(crate) fn assemble(source: &str, dialect: &dyn Dialect) -> Result<Assembly, AsmError> {
    let set = dialect.instruction_set();
    let ext = dialect.extension_set();
    let statements = dialect.parse(source)?;

    // Pass 1 — assign addresses to labels.
    let require_origin = dialect.requires_explicit_origin();
    // Emitted bytes per address unit — 1 for the byte-addressed CPUs, 2 for the
    // word-addressed CP1610 (a decle is two bytes). The location counter advances
    // in address units, so a byte length is divided by this.
    let addr_unit = dialect.addr_unit();
    let mut symbols: BTreeMap<String, i64> = BTreeMap::new();
    let mut pc: i64 = 0;
    let mut origin: Option<i64> = None;
    for s in &statements {
        // `equ` binds the label to a value, not the current address, and emits
        // nothing — so it is handled before the address-label assignment below.
        if let Some(Operation::Equ(e)) = &s.op {
            let label = s
                .label
                .as_ref()
                .ok_or_else(|| AsmError::new(s.line, "`equ` needs a label"))?;
            let v = e.eval(&symbols, pc, s.line)?;
            // Constants may be 24-bit (65816 bank/long addresses).
            if !(0..=0xFF_FFFF).contains(&v) {
                return Err(AsmError::new(
                    s.line,
                    format!("equ value {v} out of range 0..=16777215"),
                ));
            }
            if symbols.insert(label.clone(), v).is_some() {
                return Err(AsmError::new(s.line, format!("duplicate label `{label}`")));
            }
            continue;
        }
        if let Some(label) = &s.label {
            if !(0..=0xFF_FFFF).contains(&pc) {
                return Err(AsmError::new(s.line, "address out of range"));
            }
            if symbols.insert(label.clone(), pc).is_some() {
                return Err(AsmError::new(s.line, format!("duplicate label `{label}`")));
            }
        }
        match &s.op {
            None => {}
            Some(Operation::Org(e)) => {
                let v = e.eval(&symbols, pc, s.line)?;
                if !(0..=0xFFFF).contains(&v) {
                    return Err(AsmError::new(s.line, "origin address out of range"));
                }
                pc = v;
                origin.get_or_insert(v);
            }
            Some(
                Operation::Bytes(_)
                | Operation::Words(_)
                | Operation::Instruction { .. }
                | Operation::Encoded(_)
                | Operation::Align { .. },
            ) if require_origin && origin.is_none() => {
                return Err(AsmError::new(
                    s.line,
                    "program counter undefined — set an origin (`*=`) before any code or data",
                ));
            }
            Some(Operation::Bytes(items)) => pc += items.len() as i64 / addr_unit,
            Some(Operation::Words(items)) => pc += 2 * items.len() as i64 / addr_unit,
            Some(Operation::Instruction { mnemonic, mode, .. }) => {
                pc += form(set, ext, mnemonic, mode, s.line)?.len() as i64 / addr_unit;
            }
            Some(Operation::Encoded(pieces)) => {
                pc += pieces.iter().map(Piece::len).sum::<i64>() / addr_unit;
            }
            Some(Operation::Equ(_)) => {}   // handled above
            Some(Operation::Entry(_)) => {} // records a start address; emits nothing
            Some(Operation::Align { andmask, value, .. }) => pc += (value - pc) & andmask,
        }
    }
    let origin = origin.unwrap_or(0);

    // Pass 2 — emit.
    let byte_policy = dialect.oversized_byte_policy();
    let mut warnings: Vec<Warning> = Vec::new();
    let mut start: Option<u16> = None;
    let mut bytes: Vec<u8> = Vec::new();
    let mut debug = DebugData::default();
    for s in &statements {
        // The location counter (`$`) is the address of this statement's start,
        // in address units (bytes divided by `addr_unit`).
        let pc = origin + bytes.len() as i64 / addr_unit;
        let len_before = bytes.len();
        match &s.op {
            None => {}
            Some(Operation::Org(e)) => {
                let target = e.eval(&symbols, pc, s.line)?;
                let cur = origin + bytes.len() as i64 / addr_unit;
                if target < cur {
                    return Err(AsmError::new(s.line, "cannot move origin backwards"));
                }
                bytes.resize(bytes.len() + ((target - cur) * addr_unit) as usize, 0);
            }
            Some(Operation::Equ(_)) => {} // defines a symbol; emits nothing
            Some(Operation::Entry(e)) => {
                let v = e.eval(&symbols, pc, s.line)?;
                if !(0..=0xFFFF).contains(&v) {
                    return Err(AsmError::new(s.line, "entry address out of range"));
                }
                start = Some(v as u16);
            }
            Some(Operation::Align {
                andmask,
                value,
                fill,
            }) => {
                let pad = (value - pc) & andmask;
                bytes.extend(std::iter::repeat_n(*fill, pad as usize));
            }
            Some(Operation::Bytes(items)) => {
                for e in items {
                    let v = e.eval(&symbols, pc, s.line)?;
                    emit_byte(&mut bytes, v, byte_policy, &mut warnings, s.line)?;
                }
            }
            Some(Operation::Words(items)) => {
                for e in items {
                    let v = e.eval(&symbols, pc, s.line)?;
                    push_word(&mut bytes, v, s.line, set.endianness)?;
                }
            }
            Some(Operation::Instruction {
                mnemonic,
                mode,
                operands,
            }) => {
                let f = form(set, ext, mnemonic, mode, s.line)?;
                if operands.len() != f.operands.len() {
                    return Err(AsmError::new(
                        s.line,
                        format!(
                            "internal: `{mnemonic}` {mode} takes {} operand value(s), got {}",
                            f.operands.len(),
                            operands.len()
                        ),
                    ));
                }
                let next_addr = origin + bytes.len() as i64 + f.len() as i64;
                bytes.extend_from_slice(f.opcode);
                for (slot, e) in f.operands.iter().zip(operands.iter()) {
                    let v = e.eval(&symbols, pc, s.line)?;
                    match slot.kind {
                        // Immediates and addresses lay down a value of the
                        // slot's width; only the width matters on the wire, so
                        // they share a path. (A 6502 immediate is always one
                        // byte; a Z80 `LD BC,nn` immediate is two.)
                        isa::OperandKind::Immediate | isa::OperandKind::Address => {
                            match slot.bytes {
                                1 => emit_byte(&mut bytes, v, byte_policy, &mut warnings, s.line)?,
                                2 => push_word(&mut bytes, v, s.line, set.endianness)?,
                                // 24-bit address (65816 long addressing).
                                3 => push_addr24(&mut bytes, v, s.line, set.endianness)?,
                                other => {
                                    return Err(AsmError::new(
                                        s.line,
                                        format!("unsupported operand width {other}"),
                                    ));
                                }
                            }
                        }
                        // A big-endian immediate (Z80N `push nn`): high byte
                        // first, regardless of the set's little-endian default.
                        isa::OperandKind::ImmediateBe => {
                            push_word(&mut bytes, v, s.line, isa::Endianness::Big)?;
                        }
                        // A signed index displacement, e.g. the `d` in (IX+d).
                        isa::OperandKind::Displacement => {
                            if !(-128..=127).contains(&v) {
                                return Err(AsmError::new(
                                    s.line,
                                    format!("displacement {v} out of range (-128..=127)"),
                                ));
                            }
                            bytes.push(v as i8 as u8);
                        }
                        isa::OperandKind::RelativePc => {
                            let offset = v - next_addr;
                            match slot.bytes {
                                1 => {
                                    if !(-128..=127).contains(&offset) {
                                        return Err(AsmError::new(
                                            s.line,
                                            format!(
                                                "branch target out of range ({offset} bytes; must be -128..=127)"
                                            ),
                                        ));
                                    }
                                    bytes.push(offset as i8 as u8);
                                }
                                // 16-bit relative (65816 brl/per).
                                2 => {
                                    if !(-32768..=32767).contains(&offset) {
                                        return Err(AsmError::new(
                                            s.line,
                                            format!(
                                                "long branch target out of range ({offset} bytes; must be -32768..=32767)"
                                            ),
                                        ));
                                    }
                                    push_word(&mut bytes, offset & 0xFFFF, s.line, set.endianness)?;
                                }
                                other => {
                                    return Err(AsmError::new(
                                        s.line,
                                        format!("unsupported relative width {other}"),
                                    ));
                                }
                            }
                        }
                    }
                }
                // Trailing opcode bytes after the operands (Z80 DD CB / FD CB).
                bytes.extend_from_slice(f.suffix);
            }
            Some(Operation::Encoded(pieces)) => {
                for piece in pieces {
                    match piece {
                        Piece::Lit(b) => bytes.push(*b),
                        Piece::Val {
                            expr,
                            bytes: width,
                            rel,
                            signed,
                        } => {
                            let raw = expr.eval(&symbols, pc, s.line)?;
                            // A branch offset is relative to the address that
                            // follows this value (the next instruction).
                            let next = origin + bytes.len() as i64 + i64::from(*width);
                            let v = if *rel { raw - next } else { raw };
                            emit_value(
                                &mut bytes,
                                v,
                                *width,
                                *rel || *signed,
                                set.endianness,
                                s.line,
                            )?;
                        }
                        Piece::Packed {
                            expr,
                            bytes: width,
                            scale,
                            min,
                            max,
                            mask,
                            or_bits,
                            what,
                        } => {
                            let raw = expr.eval(&symbols, pc, s.line)?;
                            if *scale != 1 && raw % *scale != 0 {
                                return Err(AsmError::new(
                                    s.line,
                                    format!("{what} ({raw}) is not a multiple of {scale}"),
                                ));
                            }
                            let v = raw / *scale;
                            if !(*min..=*max).contains(&v) {
                                return Err(AsmError::new(
                                    s.line,
                                    format!("{what} out of range ({v}; must be {min}..={max})"),
                                ));
                            }
                            let packed = i64::from((v as u32 & *mask) | *or_bits);
                            emit_value(&mut bytes, packed, *width, false, set.endianness, s.line)?;
                        }
                        Piece::Branch {
                            target,
                            base,
                            dir_bit,
                            what,
                        } => {
                            let tgt = target.eval(&symbols, pc, s.line)?;
                            // The CP1610 measures from the address after both
                            // words (opcode + magnitude) — two address units past
                            // this instruction's start (`pc`).
                            let d = tgt - (pc + 2);
                            let (word1, mag) = if d >= 0 {
                                (i64::from(*base), d)
                            } else {
                                (i64::from(*base | *dir_bit), -d - 1)
                            };
                            if !(0..=0xFFFF).contains(&mag) {
                                return Err(AsmError::new(
                                    s.line,
                                    format!("{what} out of range ({d} words)"),
                                ));
                            }
                            emit_value(&mut bytes, word1, 2, false, set.endianness, s.line)?;
                            emit_value(&mut bytes, mag, 2, false, set.endianness, s.line)?;
                        }
                    }
                }
            }
        }

        // --- Debug capture (U2). Reads only `pc`/`bytes.len()`/`symbols`; it
        // never influences an emitted byte (AE2). Addresses are section-relative
        // offsets in address units, section 0 based at `origin`. ---
        if let Some(label) = &s.label {
            let kind = if matches!(&s.op, Some(Operation::Equ(_))) {
                // An `equ`/`=` constant: its value, not an address, and no space.
                let value = symbols.get(label).copied().unwrap_or_default();
                dbg198x::SymbolKind::Const {
                    value: value as u64,
                }
            } else {
                // A label lives at this statement's address (`pc`).
                dbg198x::SymbolKind::Label {
                    section: 0,
                    offset: (pc - origin) as u64,
                    space: None,
                }
            };
            debug.symbols.push(dbg198x::Symbol {
                name: label.clone(),
                kind,
            });
        }
        // A source-bearing statement that emitted bytes gets a line span; `org`
        // gaps and `align` fill do not (the padding rule).
        let source_bearing = matches!(
            &s.op,
            Some(
                Operation::Bytes(_)
                    | Operation::Words(_)
                    | Operation::Instruction { .. }
                    | Operation::Encoded(_)
            )
        );
        if source_bearing && bytes.len() > len_before {
            debug.lines.push(LineRec {
                line: s.line as u32,
                offset: (pc - origin) as u64,
                length: ((bytes.len() - len_before) as i64 / addr_unit) as u64,
            });
        }
        // The entry point (`end <addr>`) is an Entry symbol. When it targets a
        // bare label, upgrade that label's kind in place (the entry *is* that
        // location) rather than emitting a second same-named symbol; otherwise
        // record a fresh `@entry`.
        if let (Some(Operation::Entry(e)), Some(v)) = (&s.op, start) {
            let entry = dbg198x::SymbolKind::Entry {
                section: 0,
                offset: (i64::from(v) - origin) as u64,
                space: None,
            };
            let existing = match e {
                Expr::Sym(n) => debug
                    .symbols
                    .iter_mut()
                    .find(|s| s.name == *n && matches!(s.kind, dbg198x::SymbolKind::Label { .. })),
                _ => None,
            };
            if let Some(sym) = existing {
                sym.kind = entry;
            } else {
                let name = match e {
                    Expr::Sym(n) => n.clone(),
                    _ => "@entry".to_string(),
                };
                debug.symbols.push(dbg198x::Symbol { name, kind: entry });
            }
        }
    }

    if origin + bytes.len() as i64 > 0x1_0000 {
        return Err(AsmError::new(0, "program exceeds the 64K address space"));
    }

    Ok(Assembly {
        origin: origin as u16,
        bytes,
        symbols,
        start,
        warnings,
        debug,
    })
}

/// Look up a resolved instruction form in the spec, erroring with the source
/// line if the mnemonic is unknown or lacks the chosen addressing mode.
fn form<'a>(
    set: &'a isa::InstructionSet,
    ext: Option<&'a isa::InstructionSet>,
    mnemonic: &str,
    mode: &str,
    line: usize,
) -> Result<&'a isa::Form, AsmError> {
    let found = set
        .find_form(mnemonic, mode)
        .or_else(|| ext.and_then(|e| e.find_form(mnemonic, mode)));
    if let Some(f) = found {
        Ok(f)
    } else if set.has_mnemonic(mnemonic) || ext.is_some_and(|e| e.has_mnemonic(mnemonic)) {
        Err(AsmError::new(
            line,
            format!("`{mnemonic}` has no {mode} addressing mode"),
        ))
    } else {
        Err(AsmError::new(
            line,
            format!("unknown instruction `{mnemonic}`"),
        ))
    }
}

/// Emit a [`Piece::Val`]: `width` bytes of `v` in the CPU's endianness. `signed`
/// range-checks as two's-complement (branch offsets, index displacements);
/// otherwise as an unsigned address/immediate (a byte also accepts `-128..=-1`).
fn emit_value(
    bytes: &mut Vec<u8>,
    v: i64,
    width: u8,
    signed: bool,
    endianness: isa::Endianness,
    line: usize,
) -> Result<(), AsmError> {
    // `signed` (branch offsets, signed index displacements) range-checks as
    // two's-complement. Otherwise the value is an address/immediate/large index
    // offset, accepted as either-signed across the full width: a 16-bit indexed
    // offset is often a base address ≥ `$8000` yet a small one may be negative.
    let (lo, hi) = match width {
        1 if signed => (-128, 127),
        1 => (-128, 0xFF),
        2 if signed => (-32768, 32767),
        2 => (-32768, 0xFFFF),
        4 if signed => (i64::from(i32::MIN), i64::from(i32::MAX)),
        4 => (i64::from(i32::MIN), i64::from(u32::MAX)),
        other => {
            return Err(AsmError::new(
                line,
                format!("unsupported value width {other}"),
            ));
        }
    };
    if !(lo..=hi).contains(&v) {
        return Err(AsmError::new(
            line,
            format!("value {v} out of range for a {width}-byte operand"),
        ));
    }
    let b = v.to_le_bytes();
    match (width, endianness) {
        (1, _) => bytes.push(b[0]),
        (2, isa::Endianness::Little) => bytes.extend_from_slice(&b[..2]),
        (2, isa::Endianness::Big) => bytes.extend_from_slice(&[b[1], b[0]]),
        (4, isa::Endianness::Little) => bytes.extend_from_slice(&b[..4]),
        (4, isa::Endianness::Big) => bytes.extend_from_slice(&[b[3], b[2], b[1], b[0]]),
        _ => unreachable!("width validated above"),
    }
    Ok(())
}

/// Emit a byte value, applying the dialect's over-range `policy`. A value in
/// `-128..=255` fits and is emitted as-is; beyond that, the policy decides —
/// error, silently keep the low 8 bits (pasmo), or keep them with a warning
/// (sjasmplus).
fn emit_byte(
    bytes: &mut Vec<u8>,
    v: i64,
    policy: Oversize,
    warnings: &mut Vec<Warning>,
    line: usize,
) -> Result<(), AsmError> {
    if !(-128..=0xFF).contains(&v) {
        match policy {
            Oversize::Error => {
                return Err(AsmError::new(
                    line,
                    format!("value {v} does not fit in a byte"),
                ));
            }
            Oversize::Truncate => {}
            Oversize::TruncateWarn => {
                warnings.push(Warning::new(line, format!("value {v} truncated to a byte")));
            }
        }
    }
    bytes.push((v & 0xFF) as u8);
    Ok(())
}

fn push_word(
    bytes: &mut Vec<u8>,
    v: i64,
    line: usize,
    endianness: isa::Endianness,
) -> Result<(), AsmError> {
    if !(0..=0xFFFF).contains(&v) {
        return Err(AsmError::new(
            line,
            format!("value {v} does not fit in a word"),
        ));
    }
    let lo = (v & 0xFF) as u8;
    let hi = ((v >> 8) & 0xFF) as u8;
    match endianness {
        isa::Endianness::Little => {
            bytes.push(lo);
            bytes.push(hi);
        }
        isa::Endianness::Big => {
            bytes.push(hi);
            bytes.push(lo);
        }
    }
    Ok(())
}

/// Emit a 24-bit address (the 65816 long-addressing operand).
fn push_addr24(
    bytes: &mut Vec<u8>,
    v: i64,
    line: usize,
    endianness: isa::Endianness,
) -> Result<(), AsmError> {
    if !(0..=0xFF_FFFF).contains(&v) {
        return Err(AsmError::new(
            line,
            format!("value {v} does not fit in a 24-bit address"),
        ));
    }
    let b = [
        (v & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        ((v >> 16) & 0xFF) as u8,
    ];
    match endianness {
        isa::Endianness::Little => bytes.extend_from_slice(&b),
        isa::Endianness::Big => bytes.extend_from_slice(&[b[2], b[1], b[0]]),
    }
    Ok(())
}
