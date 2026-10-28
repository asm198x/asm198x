//! The generic statement stream a front end produces and the back end
//! assembles: [`Statement`], its [`Operation`]s and the [`Expr`] values inside
//! them, plus the output requests a source can make.

use std::collections::BTreeMap;

use crate::diagnostic::AsmError;
use crate::span::{FileId, Span};

/// How a source-named output file is framed. ACME's three, and the only ones
/// `!to` accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// The bytes alone.
    Plain,
    /// A two-byte little-endian load address, then the bytes — a C64 `.prg`.
    /// ACME's default when `!to` names no format, with a warning.
    Cbm,
    /// A two-byte load address and a two-byte length, then the bytes.
    Apple,
}

/// An output the **source** asked for, rather than the command line.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestedOutput {
    pub path: String,
    pub format: OutputFormat,
}

/// What a tape's header says the block after it holds — the two kinds an
/// assembler has any use for, of the four the format has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapeKind {
    /// A BASIC program.
    Program,
    /// Machine code.
    Code,
}

/// What an `Artifact` holds. One variant for now; a container format joins it
/// with its serialiser behind this seam, kept free of this crate's types so it
/// can graduate to Format198x by moving rather than by being rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactFormat {
    /// A span of the address space, as bytes, with nothing wrapped round it.
    #[default]
    Raw,
    /// A ZX Spectrum tape image: a ROM header block naming the span, then the
    /// span itself. Written by `format198x-sinclair-zx-spectrum-tap`, which holds
    /// the layout and the citations for it.
    Tap,
    /// An Amstrad Plus cartridge: RIFF `AMS!` with one `cbNN` chunk per bank.
    Cpr,
    /// A ZX Spectrum Next NEX V1.2 executable.
    Nex,
    /// SjASMPlus's CSpect logical/physical symbol map.
    CspectMap,
    /// SjASMPlus's UnrealSpeccy label list.
    LabelsList,
}

// ---------------------------------------------------------------------------
// Expressions — the shared engine IR
// ---------------------------------------------------------------------------

/// A binary arithmetic operator. The dialect parser is responsible for
/// precedence (it builds the tree); the engine only evaluates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
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
    /// Remainder — ca65's `.mod`. C's sign rule, which is the reference's:
    /// `7 .mod (0-4)` is `3`, and a negative left operand is refused there
    /// rather than defined.
    Mod,
    /// The **logical** operators — ca65's `&&`/`.and`, `||`/`.or` and `.xor`.
    /// Each takes its operands as true-or-false and answers `1` or `0`, so
    /// `2 .and 3` is `1` where `2 & 3` is `2`.
    LogAnd,
    LogOr,
    LogXor,
    /// Exponentiation (ACME's `^`): `a` raised to the power `b`.
    Pow,
    /// Fixed-point multiply and divide in rgbasm's Q16.16: `MUL(1.5, 1.5)` is
    /// `2.25`. Both follow RGBDS exactly: the operands become `f64`s, the
    /// result rounds to the nearest Q16.16 value (halves away from zero) and
    /// saturates to the 32-bit range. `DIV` by zero is not an error there: it
    /// answers `INT32_MAX`, `INT32_MIN` or `0` by the dividend's sign. See
    /// [`fix_mul`] and [`fix_div`].
    FixMul,
    FixDiv,
    /// The larger of the two (ca65 `.max`).
    Max,
    /// The smaller of the two (ca65 `.min`).
    Min,
    /// The comparisons. Evaluation answers 1 or 0; a dialect whose reference
    /// answers `$FF` for true wraps the comparison in a negation, so the tree
    /// carries that and evaluation stays dialect-agnostic
    /// (`docs/comparison-operators.md`).
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

/// The fraction's width in rgbasm's fixed-point numbers: `1.0` is `$10000`.
pub const FIX_BITS: i64 = 16;

/// An expression in the shared engine IR. Each dialect parses its own operator
/// syntax into this tree; the engine evaluates it. The tree stays dialect-
/// agnostic: a `<`/`>` operator and a `low()`/`high()` function both lower to
/// [`Expr::Lo`]/[`Expr::Hi`], and any dialect's `+`/`-`/`*`/`/` lower to
/// [`Expr::Bin`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
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
    /// Bitwise complement — ca65's `~` and `.bitnot`. Every bit, so the value
    /// is negative unless something masks it: `~0` is `-1` and `~0 & $FF` is
    /// `$FF`.
    BitNot(Box<Expr>),
    /// rgbasm's `ROUND` over a Q16.16 value: to the nearest whole number, with
    /// a half going *away from zero*. `ROUND(-3.2)` is `-3.0` and `ROUND(-3.5)`
    /// is `-4.0`, which is why this is a node rather than an add-and-mask. See
    /// [`fix_round`].
    FixRound(Box<Expr>),
    /// rgbasm's `TZCOUNT`: how many low bits of the value are zero. RGBDS counts
    /// over 32 bits, so `TZCOUNT(0)` is `32`. See [`trailing_zeros`].
    TrailingZeros(Box<Expr>),
    /// Logical negation — ca65's `!` and `.not`. Answers `1` or `0`, and binds
    /// *looser* than everything else in that dialect: `.not 1 .or 1` is `0`,
    /// because the `.or` happens first.
    LogNot(Box<Expr>),
    /// A binary operation on two sub-expressions.
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Evaluate against the engine's `u16` symbol table, with `pc` the address
    /// of the current statement. A thin wrapper over [`Expr::eval_with`], the
    /// single evaluator shared by every dialect.
    pub fn eval(
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
    pub fn eval_with(
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
            Expr::BitNot(e) => !e.eval_with(resolve, pc, line)?,
            Expr::LogNot(e) => i64::from(e.eval_with(resolve, pc, line)? == 0),
            // Half goes away from zero, which is what rgbasm does: `ROUND(3.5)`
            // is `4.0` and `ROUND(-3.5)` is `-4.0`.
            Expr::FixRound(e) => fix_round(e.eval_with(resolve, pc, line)?),
            Expr::TrailingZeros(e) => trailing_zeros(e.eval_with(resolve, pc, line)?),
            Expr::Bin(op, l, r) => {
                let a = l.eval_with(resolve, pc, line)?;
                // Logical operators short-circuit in the source languages
                // that expose them. Besides matching their observable error
                // behavior, this lets a definedness guard protect a symbol.
                if matches!(op, BinOp::LogAnd) && a == 0 {
                    return Ok(0);
                }
                if matches!(op, BinOp::LogOr) && a != 0 {
                    return Ok(1);
                }
                let b = r.eval_with(resolve, pc, line)?;
                eval_binop(*op, a, b, line)?
            }
        })
    }
}

/// Evaluate one binary operator — the single place each operator's semantics
/// live (shifts wrap; `/` checks for zero; `+ - *` check for overflow).
pub fn eval_binop(op: BinOp, a: i64, b: i64, line: usize) -> Result<i64, AsmError> {
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
        // ca65 refuses a negative left operand rather than defining one, so it
        // never reaches here; the sign rule for the rest is C's, which is
        // Rust's — `7 .mod (0-4)` is `3`.
        BinOp::Mod if b == 0 => return Err(AsmError::new(line, "modulo by zero in expression")),
        BinOp::Mod => a.checked_rem(b).ok_or_else(overflow)?,
        // Logical, not bitwise: the operands are read as true-or-false and the
        // answer is 1 or 0. `2 .and 3` is 1 where `2 & 3` is 2.
        BinOp::LogAnd => i64::from(a != 0 && b != 0),
        BinOp::LogOr => i64::from(a != 0 || b != 0),
        BinOp::LogXor => i64::from((a != 0) ^ (b != 0)),
        BinOp::Shl => a.wrapping_shl(b as u32),
        BinOp::Shr => a.wrapping_shr(b as u32),
        // ca65's `.max`/`.min`. Binary operations rather than a dedicated node:
        // they take two values and produce one, which is what `BinOp` is for.
        BinOp::Eq => i64::from(a == b),
        BinOp::Ne => i64::from(a != b),
        BinOp::Lt => i64::from(a < b),
        BinOp::Gt => i64::from(a > b),
        BinOp::Le => i64::from(a <= b),
        BinOp::Ge => i64::from(a >= b),
        BinOp::Max => a.max(b),
        BinOp::Min => a.min(b),
        BinOp::Pow => {
            let exp = u32::try_from(b)
                .map_err(|_| AsmError::new(line, "negative exponent in expression"))?;
            a.checked_pow(exp).ok_or_else(overflow)?
        }
        BinOp::FixMul => fix_mul(a, b),
        BinOp::FixDiv => fix_div(a, b),
    })
}

// ---------------------------------------------------------------------------
// rgbasm's fixed-point arithmetic
// ---------------------------------------------------------------------------
//
// Transcribed from RGBDS v1.0.3 `src/asm/fixpoint.cpp` (unchanged at v1.0.4),
// the reference Asm198x's rgbasm dialect is measured against. RGBDS works in
// `double`, so these do too: `f64` arithmetic and `f64::round` are the same
// IEEE 754 operations as C's `*`, `/` and `round()`, which keeps every result
// byte-identical rather than merely close. RGBDS's values are `int32_t`, so
// an operand is read as its low 32 bits, signed — `$80000000` is -32768.0, as
// it is to RGBDS, not a wide positive number — and results saturate to `i32`.

/// One in Q16.16, as a `double`: `pow(2.0, q)` with RGBDS's default `q`.
const FIX_ONE: f64 = (1u32 << FIX_BITS) as f64;

/// A Q16.16 value as the number it represents (RGBDS `fix2double`), read as
/// the `int32_t` RGBDS holds: the low 32 bits, signed.
fn fix_to_f64(value: i64) -> f64 {
    // Truncation is the point: it is RGBDS's argument conversion.
    #[allow(clippy::cast_possible_truncation)]
    let value = value as i32;
    f64::from(value) / FIX_ONE
}

/// The nearest Q16.16 value to `d`, halves away from zero, saturated to the
/// 32-bit range; NaN is `0` (RGBDS `double2fix`).
fn f64_to_fix(d: f64) -> i64 {
    if d.is_nan() {
        return 0;
    }
    let scaled = (d * FIX_ONE).round();
    // `as` from `f64` saturates, and the clamp bounds are exact in `f64`.
    scaled.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i64
}

/// rgbasm's `MUL(a, b)` on Q16.16 values (RGBDS `fix_Mul`).
///
/// ```
/// use asm198x_ir::stream::fix_mul;
/// assert_eq!(fix_mul(10 << 16, 1 << 15), 5 << 16); // MUL(10.0, 0.5) == 5.0
/// // `$FFFF0000` is -1.0 to RGBDS, so doubling it is -2.0, not saturation.
/// assert_eq!(fix_mul(0xFFFF_0000, 2 << 16), -2 << 16);
/// ```
#[must_use]
pub fn fix_mul(a: i64, b: i64) -> i64 {
    f64_to_fix(fix_to_f64(a) * fix_to_f64(b))
}

/// rgbasm's `DIV(a, b)` on Q16.16 values (RGBDS `fix_Div`). A zero divisor
/// saturates by the dividend's sign rather than failing.
///
/// ```
/// use asm198x_ir::stream::fix_div;
/// assert_eq!(fix_div(5 << 16, 2 << 16), 5 << 15); // DIV(5.0, 2.0) == 2.5
/// assert_eq!(fix_div(5 << 16, 0), i64::from(i32::MAX)); // +inf
/// assert_eq!(fix_div(-5 << 16, 0), i64::from(i32::MIN)); // -inf
/// assert_eq!(fix_div(0, 0), 0); // NaN
/// ```
#[must_use]
pub fn fix_div(a: i64, b: i64) -> i64 {
    let (dividend, divisor) = (fix_to_f64(a), fix_to_f64(b));
    if divisor == 0.0 {
        return match dividend.partial_cmp(&0.0) {
            Some(std::cmp::Ordering::Less) => i64::from(i32::MIN),
            Some(std::cmp::Ordering::Greater) => i64::from(i32::MAX),
            _ => 0,
        };
    }
    f64_to_fix(dividend / divisor)
}

/// rgbasm's `ROUND(v)` on a Q16.16 value (RGBDS `fix_Round`): the nearest whole
/// number, halves away from zero.
///
/// ```
/// use asm198x_ir::stream::fix_round;
/// let fix = |v: f64| (v * 65536.0) as i64;
/// assert_eq!(fix_round(fix(1.5)), fix(2.0));
/// assert_eq!(fix_round(fix(-1.5)), fix(-2.0));
/// assert_eq!(fix_round(fix(-3.2)), fix(-3.0));
/// assert_eq!(fix_round(0x8000_0000), i64::from(i32::MIN)); // -32768.0
/// ```
#[must_use]
pub fn fix_round(value: i64) -> i64 {
    f64_to_fix(fix_to_f64(value).round())
}

/// rgbasm's `TZCOUNT(v)` (RGBDS `op_tzcount`): trailing zero bits of the low 32
/// bits, so zero has all 32.
///
/// ```
/// use asm198x_ir::stream::trailing_zeros;
/// assert_eq!(trailing_zeros(0), 32);
/// assert_eq!(trailing_zeros(42), 1);
/// assert_eq!(trailing_zeros(-1), 0);
/// assert_eq!(trailing_zeros(0x8000_0000), 31);
/// ```
#[must_use]
pub fn trailing_zeros(value: i64) -> i64 {
    // RGBDS's value is an `int32_t`, so only the low 32 bits exist to count.
    let low = value as u32;
    if low == 0 {
        32
    } else {
        i64::from(low.trailing_zeros())
    }
}

// ---------------------------------------------------------------------------
// The generic statement stream a dialect produces
// ---------------------------------------------------------------------------

/// Accepted values for a sized data directive, independent of instruction operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataRange {
    SignedOrUnsigned,
    Unsigned,
}

/// The bounded native SAVENEX command stream, evaluated in the emit pass.
#[derive(Clone)]
pub enum NexCommand {
    Open { name: String, values: Vec<Expr> },
    Core([Expr; 3]),
    Auto(Vec<Expr>),
    Close,
}

impl NexCommand {
    pub fn map(self, f: impl FnMut(Expr) -> Expr) -> Self {
        match self {
            Self::Open { name, values } => Self::Open {
                name,
                values: values.into_iter().map(f).collect(),
            },
            Self::Core(values) => Self::Core(values.map(f)),
            Self::Auto(values) => Self::Auto(values.into_iter().map(f).collect()),
            Self::Close => Self::Close,
        }
    }
}

/// One operation, with its addressing mode already resolved by the dialect.
#[derive(Clone)]
pub enum Operation {
    /// Set the program counter (the `.org`/`org` directive).
    Org(Expr),
    /// Define the statement's label as a constant value rather than the PC
    /// (the `equ`/`=` directive). The statement must carry a label.
    /// Write a span of the address space to a named file — sjasmplus's
    /// `SAVEBIN`. The span is resolved against the finished image, so this is
    /// collected in pass two and answered once the image exists: an address
    /// the source never wrote reads as zero, which is what a device's memory
    /// does and what the reference was measured doing.
    ///
    /// `length` of `None` — or zero, which the reference treats the same way —
    /// means "to the end of the address space".
    SaveRaw {
        name: String,
        start: Expr,
        length: Option<Expr>,
    },
    /// Write a span of the flat image to a named file — PasmoNext's
    /// `SAVEBIN`. Each address holds the last byte written there at or before
    /// this statement, or else the last byte written there anywhere, or else
    /// zero: PasmoNext's memory as it stands when its last pass reaches the
    /// line, over what its first pass left. `length` bytes are written, none
    /// for zero.
    SaveImage {
        name: String,
        start: Expr,
        length: Expr,
    },
    /// Write a span of the address space as a tape image — sjasmplus's
    /// `SAVETAP`. Resolved with [`Operation::SaveRaw`] and under the same
    /// rule: the span has to be one this source assembled.
    SaveTape {
        file: String,
        kind: TapeKind,
        name: String,
        start: Expr,
        length: Expr,
    },
    /// Select or clear sjasmplus's emulated device memory.
    Device(Option<DeviceSpec>),
    /// Select the slot a following [`Operation::DevicePage`] remaps.
    DeviceSlot(Expr),
    /// Map a page into the current slot and start a new raw-output run.
    DevicePage(Expr),
    /// Save the first `pages` of an Amstrad Plus device as a CPR cartridge.
    SaveNex(NexCommand),
    SaveCpr {
        name: String,
        pages: Expr,
    },
    /// Request a symbol view of the finished assembly.
    SymbolMap {
        name: String,
        format: ArtifactFormat,
        virtual_labels: bool,
    },
    /// Preserve the distinction between a structure definition and an EQU.
    StructSymbol(String),
    /// A **redefinable** symbol binding — lwasm's `set`, and the same idea
    /// under ca65's `.set`, sjasmplus's `defl` and rgbasm's `=`. Unlike
    /// [`Operation::Equ`] it may bind the same name again, and it rebinds in
    /// pass two as well as pass one: a use *before* the statement reads the
    /// value the name was left with at the end of pass one (the last binding
    /// in the file), and a use after it reads the value bound here. That is
    /// what a two-pass assembler does with a name that moves, and it is what
    /// lwtools 4.25 was measured doing.
    Set(Expr),
    Equ(Expr),
    /// Define several immutable constants on one source line — ASL's `ENUM`.
    /// Values are folded by the source walk because each implicit member
    /// depends on the preceding member's concrete value.
    DefineSymbols(Vec<(String, i64)>),
    /// Emit one byte per expression.
    Bytes(Vec<Expr>),
    /// Emit one word per expression, in the instruction set's endianness.
    Words(Vec<Expr>),
    /// Begin an OS-9 module. The four mandatory fields are module length,
    /// name offset, type/language, and attributes/revision; executable and
    /// data-size offsets are present together for the six-field header.
    /// `mod` also establishes module-relative address zero.
    Os9Module {
        fields: Vec<Expr>,
    },
    /// Finish an OS-9 module with its three-byte CRC-24.
    Os9EndModule,
    /// Emit each expression as `width` bytes, in the byte order the
    /// **directive** names rather than the one the CPU uses — ACME's
    /// `!be16`/`!le32` family, six spellings of one rule.
    ///
    /// Distinct from [`Operation::Words`] for both halves of that: the width
    /// is 2, 3 or 4 rather than always 2, and the order is a property of the
    /// word the source wrote. A 6502 is little-endian, and `!be24` on a 6502
    /// still emits big-endian — that is the whole point of the directive.
    Sized {
        width: u8,
        big_endian: bool,
        range: DataRange,
        values: Vec<Expr>,
    },
    /// The source naming its own output file — ACME's `!to`. Emits nothing
    /// and occupies nothing; the driver reads the **first** one and warns on
    /// any later one, which is what ACME does ("Output file already chosen").
    RequestOutput {
        path: String,
        format: OutputFormat,
        /// The source named no format and got the default. ACME warns about
        /// that, so the driver does too.
        defaulted_format: bool,
    },
    /// The source naming a symbol-list file — ACME's `!symbollist`. Same rule
    /// as `RequestOutput`, and ACME warns in the same words.
    RequestSymbols {
        path: String,
    },
    /// Enter (`Some`) or leave (`None`) an ACME `!pseudopc` block: assemble
    /// here, but let labels and `*` read as if the code were somewhere else.
    ///
    /// The offset cannot be computed where the block is written — it is the
    /// target minus the **real** address the block opens at, and the walk
    /// that sees the braces has not assigned addresses yet. So the boundary
    /// is lowered as an operation and the engine, which knows the real
    /// counter, works the offset out. Emits nothing and occupies nothing.
    PseudoPc(Option<Expr>),
    /// Set the byte that fills reserved space and `org` gaps, for the **whole
    /// assembly** — ACME's `!initmem`.
    ///
    /// Emits nothing, and does not act from where it is written: a `!skip`
    /// *earlier* in the file still takes the value, because ACME is choosing
    /// what unwritten memory holds rather than running a statement. The first
    /// one wins and a second is a warning ("Memory already initialised"),
    /// which is why the driver scans for it instead of the walk applying it.
    InitMem(u8),
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
    /// An instruction encoded by its dialect; pass two resolves its timing.
    ComputedInstruction(Vec<Piece>),
    /// A 6809 bare address whose direct/extended width depends on whether its
    /// expression is already known at this point in pass one. Backward labels
    /// may select direct page; forward labels deliberately stay extended.
    DirectPage {
        direct: Vec<u8>,
        extended: Vec<u8>,
        expr: Expr,
        dp: u8,
    },
    /// Record the program's entry point (the `end <addr>` directive). Emits no
    /// bytes; surfaced on `Assembly::start` for containers that carry a start
    /// address (e.g. a Spectrum `.sna` snapshot). A flat binary ignores it.
    Entry(Expr),
    /// A raw binary payload at the current location (an `incbin` asset,
    /// language-surface U3). Deliberately **not** [`Operation::Bytes`]: a 32KB
    /// asset as one `Expr` per byte would allocate a tree node and run the
    /// per-byte oversize policy on every byte of data that was never source
    /// text. The payload is emitted verbatim; it occupies a whole number of
    /// **address units** (`Dialect::addr_unit`), the final partial unit
    /// zero-padded — a byte-addressed CPU (unit 1) is unaffected, and the
    /// word-addressed CP1610 (unit 2, U4) inherits consistent pass-1/pass-2
    /// accounting.
    Binary(Vec<u8>),
    /// Advance the program counter to the next address where `pc & andmask ==
    /// value`, filling the gap with `fill` (ACME's `!align andmask, value
    /// [, fill]`). The pad count is PC-dependent, so it is resolved in the
    /// engine passes; `andmask`/`value`/`fill` are folded to constants by the
    /// dialect. The pad is `(value - pc) & andmask`.
    Align {
        andmask: i64,
        value: i64,
        fill: u8,
    },
    /// Advance the program counter to the next multiple of `modulus`, filling
    /// the gap with `fill` — the `align`/`.align` of every dialect that states
    /// a boundary rather than ACME's mask/value pair. A `modulus` of 1 or less
    /// pads nothing.
    ///
    /// Deliberately not folded into [`Operation::Align`]: a mask can only
    /// express a power-of-two boundary, and both ca65 and lwasm pad to a
    /// non-power-of-two one (`.align 3` after a byte lands the next item at
    /// offset 3). Approximating either with the other would diverge silently
    /// on exactly the source that distinguishes them.
    AlignTo {
        modulus: i64,
        /// One address unit of fill, in output byte order. Byte-addressed
        /// dialects carry one byte; the word-addressed CP1610 carries two.
        /// Empty takes the dialect's gap fill, as a reserve does.
        fill: Vec<u8>,
    },
    /// Open a section: subsequent bytes are placed at `base` rather than
    /// continuing from the current address, and the section's own bytes are
    /// laid out independently of what came before.
    ///
    /// A dialect with no section concept never emits this and is a program of
    /// exactly one implicit section based at its origin — which is what the
    /// flat engine has always been.
    ///
    /// `base` is `None` for a section whose address its toolchain's linker
    /// chooses. Placement for those is not implemented: the section continues
    /// from the current address, which is what the dialects that have them did
    /// before this existed. `name` carries into the debug section table.
    ///
    /// `at` says where the section's bytes sit **in the image**, when that is
    /// not the same thing as where the CPU sees them.
    Section {
        name: String,
        base: Option<i64>,
        at: Place,
        bank: Option<u32>,
    },
    /// A diagnostic the **source** asked for: ACME's `!error`/`!warn`, lwasm's
    /// `error`, rgbasm's `FAIL`/`WARN`. `fatal` aborts the assembly; otherwise
    /// it joins the warnings and assembly continues.
    ///
    /// An operation rather than a parse-time error on purpose: every dialect
    /// that has one also has conditional assembly, and `!error` inside an
    /// untaken `!if` must stay silent. Raising it at parse time would fire on
    /// a branch the program never takes.
    Diagnose {
        severity: DiagSeverity,
        message: String,
    },
    /// A reference-valid construct refused only if evaluation reaches it.
    Unsupported(String),
    /// An assertion the source asked for: `ASSERT`/`assert`/`.assert`. The
    /// diagnostic fires when `cond` evaluates to zero.
    ///
    /// Evaluated in the emit pass rather than at parse, because an assertion
    /// reaches **forward** — `assert end-start = 2` above both labels is the
    /// point of having one, and both vasm and rgbasm take it.
    Assert {
        cond: Expr,
        fatal: bool,
        message: String,
    },
    /// Reserve `count` address units without emitting source-derived data (the
    /// `ds`/`rmb`/`res`/`block` directives). Deliberately **not**
    /// [`Operation::Bytes`] of zeros: what fills reserved space is a property of
    /// the dialect's toolchain, not of the source, so the engine fills it with
    /// `Dialect::gap_fill` — the same value an `org` gap takes. Emitting zeros
    /// here is what made the asl-family output diverge from asl + p2bin, which
    /// leaves a gap and fills it with `$FF`.
    Reserve(usize),
    /// Emit `value` for a count resolved at the statement's live location.
    /// RGBASM's `DS $150 - @, 0` needs this: the count is constant only once
    /// the section has been placed and `@` has a value.
    Fill {
        count: Expr,
        value: u8,
    },
}

/// Geometry of a sjasmplus emulated device. `DeviceMemory::new` applies the
/// machine-family seed established by #318.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSpec {
    pub name: String,
    pub pages: usize,
    pub slots: usize,
    pub slot_size: usize,
}

/// One piece of a dialect-computed instruction encoding.
#[derive(Clone)]
pub enum Piece {
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

pub fn encoded_pieces(opcode: &[u8], expr: Expr, width: u8) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = opcode.iter().copied().map(Piece::Lit).collect();
    pieces.push(Piece::Val {
        expr,
        bytes: width,
        rel: false,
        signed: false,
    });
    pieces
}

/// Where the location counter stands after `op`, given that it stood at `pc`
/// before it. The one place an operation's width is decided.
///
/// It is shared because a dialect can need it too: ACME sizes a zero-page
/// operand from the *value* of a backward label, which means its walk has to
/// know addresses while it is still parsing (`decisions/acme-zero-page.md`).
/// A second copy of these rules living in a front-end is how the two drift.
///
/// `Org` is not here. It sets the counter rather than advancing it, and its
/// expression resolves against whichever symbol table the caller has — the
/// engine's in pass 1, the walk's `env` in ACME. Nor are `Equ` and `Entry`,
/// which emit nothing.
pub fn next_pc(
    op: &Operation,
    pc: i64,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    addr_unit: i64,
    line: usize,
) -> Result<i64, AsmError> {
    Ok(match op {
        Operation::Bytes(items) => pc + items.len() as i64 / addr_unit,
        Operation::Reserve(count) => pc + *count as i64,
        Operation::Fill { count, .. } => {
            let count = count.eval(&BTreeMap::new(), pc, line)?;
            if count < 0 {
                return Err(AsmError::new(line, "fill count must not be negative"));
            }
            pc.checked_add(count)
                .ok_or_else(|| AsmError::new(line, "location counter overflow"))?
        }
        // A binary payload occupies whole address units, the final partial unit
        // zero-padded in pass 2 — so both passes count the same.
        Operation::Binary(payload) => pc + payload.len().div_ceil(addr_unit as usize) as i64,
        Operation::Words(items) => pc + 2 * items.len() as i64 / addr_unit,
        Operation::Os9Module { fields } => pc + if fields.len() == 6 { 13 } else { 9 },
        Operation::Os9EndModule => pc + 3,
        Operation::Sized { width, values, .. } => {
            pc + i64::from(*width) * values.len() as i64 / addr_unit
        }
        Operation::Instruction { mnemonic, mode, .. } => {
            pc + form(set, ext, mnemonic, mode, line)?.len() as i64 / addr_unit
        }
        Operation::Encoded(pieces) | Operation::ComputedInstruction(pieces) => {
            pc + pieces.iter().map(Piece::len).sum::<i64>() / addr_unit
        }
        Operation::DirectPage { .. } => {
            return Err(AsmError::new(
                line,
                "internal: unresolved direct-page operand",
            ));
        }
        Operation::Align { andmask, value, .. } => pc + ((value - pc) & andmask),
        Operation::AlignTo { modulus, .. } => pc + align_pad(pc, *modulus),
        // Emit nothing; they only speak.
        Operation::Diagnose { .. } | Operation::Assert { .. } => pc,
        // Emits nothing and occupies nothing: it chooses what unwritten
        // memory holds, and the driver has already read it.
        Operation::InitMem(_) => pc,
        // Moves no bytes: it changes what an address *reads as*, not where
        // one is.
        Operation::PseudoPc(_) => pc,
        // Name a file; emit nothing.
        Operation::RequestOutput { .. } | Operation::RequestSymbols { .. } => pc,
        // Moves the counter rather than advancing it; the caller sets it.
        Operation::Section { .. } => pc,
        // Set the counter, bind a name, name an entry point — none of them a
        // width. A caller that can reach these handles them itself.
        Operation::Org(_)
        | Operation::Equ(_)
        | Operation::Set(_)
        | Operation::DefineSymbols(_)
        | Operation::SaveRaw { .. }
        | Operation::SaveImage { .. }
        | Operation::SaveNex(_)
        | Operation::SaveTape { .. }
        | Operation::Device(_)
        | Operation::DeviceSlot(_)
        | Operation::DevicePage(_)
        | Operation::SaveCpr { .. }
        | Operation::SymbolMap { .. }
        | Operation::StructSymbol(_)
        | Operation::Unsupported(_)
        | Operation::Entry(_) => pc,
    })
}

/// How far short of the next multiple of `modulus` the counter stands. Zero
/// when already there, and for a `modulus` of 1 or less (no boundary to reach).
pub fn align_pad(pc: i64, modulus: i64) -> i64 {
    if modulus <= 1 {
        return 0;
    }
    (modulus - pc.rem_euclid(modulus)) % modulus
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
#[derive(Clone)]
pub struct Statement {
    pub line: usize,
    /// The file `line` counts within (language-surface U2). `FileId(0)` for
    /// every single-file parse; the include-capable walks mint real ids so an
    /// engine error deep in an include names the right file.
    pub file: FileId,
    pub label: Option<String>,
    pub op: Option<Operation>,
    /// The operand field's source position, when the dialect parse knew it
    /// (contract U3, [`crate::ast::operand_span`]). Pass-2 range errors point
    /// here; `None` keeps them line-granular (contract KTD1).
    pub operand_span: Option<Span>,
    /// A byte the statement's **written** output is XOR-ed with — ACME's
    /// `!xor`, and `0` (no change) for every other dialect.
    ///
    /// It has to live here rather than on an `Operation` because it reaches
    /// what the *engine* chose, not only what the dialect lowered: `!xor $ff
    /// { nop }` is `15`, so the opcode is masked too. It is set by the walk,
    /// which is where the lexical scope is known.
    ///
    /// **Written** is the operative word. The mask reaches data, opcodes and
    /// an included binary, and does *not* reach a reservation or an `org`
    /// gap: `!xor $ff { !fill 2 }` is `ff ff` while `!xor $ff { !skip 2 }` is
    /// `00 00` (probed 2026-08-25). Reserved space is not something the
    /// source wrote.
    pub xor_mask: u8,
    /// A lexical instruction-set override selected by the source (currently
    /// ACME's `!cpu`). `None` inherits the dialect's assembly-wide default.
    pub instruction_set: Option<&'static isa::InstructionSet>,
    /// The extension paired with [`Statement::instruction_set`]. Kept
    /// separately because 65816 extends, rather than replaces, the 6502 set.
    pub extension_set: Option<&'static isa::InstructionSet>,
}

impl Statement {
    fn line_span(&self, line: usize) -> Span {
        let mut span = Span::in_file(self.file, line as u32, 0);
        if line == self.line
            && let Some(operand) = &self.operand_span
        {
            span.expansion_frames.clone_from(&operand.expansion_frames);
        }
        span
    }

    /// A line-granular error at this statement, carrying its `(file, line)`
    /// as a real span — so a failure inside an included file renders with
    /// that file's name, not a bare line number.
    pub fn err(&self, message: impl Into<String>) -> AsmError {
        AsmError::at(self.line_span(self.line), message)
    }

    /// Stamp this statement's file onto an error raised by a line-only helper
    /// (expression evaluation, form lookup): a span-less error gains a
    /// line-granular span in this statement's file; an error that already
    /// carries a span (an operand-accurate one) is left alone.
    pub fn stamp(&self, mut e: AsmError) -> AsmError {
        if e.span.is_none() && e.line != 0 {
            e.span = Some(self.line_span(e.line));
        }
        e
    }

    /// An operand-range error: at the operand's span when the parse supplied
    /// one (a column-accurate diagnostic, contract U3), else line-granular.
    pub fn operand_err(&self, message: impl Into<String>) -> AsmError {
        match &self.operand_span {
            Some(span) => AsmError::at(span.clone(), message),
            None => self.err(message),
        }
    }
}

/// Where a section's bytes sit in the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Wherever its address puts it — every flat dialect, and any section whose
    /// toolchain places it by where the CPU sees it.
    ByAddress,
    /// At a fixed offset the container fixes: a Game Boy bank at
    /// `bank * $4000`, a NES segment at the offset its linker config gives it.
    /// The section is addressed somewhere the image does not put it.
    At(i64),
    /// Immediately after the section before it, whatever either is addressed
    /// at. sjasmplus's `PAGE` is this: two pages written at the same `ORG`
    /// concatenate in the output rather than colliding, because they are
    /// different memory.
    Next,
    /// The section has an address and symbols, but contributes no bytes to the
    /// output image. RGBDS RAM sections are BSS: they reserve machine memory,
    /// not cartridge space.
    Discard,
}

/// How loud a source-requested diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagSeverity {
    /// Stops the assembly: `!error`, `!serious`, `FAIL`, lwasm's `error`.
    /// A dialect that reports every error reports this one and carries on.
    Error,
    /// Stops the assembly at once, even for a dialect that reports every
    /// error: ca65's `.fatal`.
    Fatal,
    /// Notes and carries on: `!warn`, `WARN`.
    Warning,
    /// Text the source asked to print, and not a complaint about anything:
    /// `DISPLAY`, `echo`, `PRINT`, `.out`.
    Note,
}

/// Look up a resolved instruction form in the spec, erroring with the source
/// line if the mnemonic is unknown or lacks the chosen addressing mode.
pub fn form<'a>(
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
