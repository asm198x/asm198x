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
use crate::source::{SourceLoader, SourceMap};
use crate::span::{FileId, Span};

/// The result of a successful assembly: where it loads and the bytes to load.
#[derive(Debug, Clone)]
pub struct Assembly {
    /// Load address (first origin directive, or 0 if none given). On a dialect
    /// whose toolchain reserves rather than materialises, a leading gap moves
    /// this past the unwritten region — see `reserved_prefix`.
    pub origin: u16,
    /// Address units of leading `org` gap or reservation dropped from the front
    /// of the image, so `origin` is that far above where the source's own
    /// addresses start (#90). Zero on every dialect that materialises its gaps.
    ///
    /// Debug offsets are relative to the source's origin — `origin -
    /// reserved_prefix` — not to `origin`, because they are captured before the
    /// trim. A consumer splitting them into sections uses this as the boundary.
    pub reserved_prefix: u16,
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
    /// symbols the CLI renders into a `.debug198x` sidecar / `--sym` / `--listing`.
    /// Header-less (the CPU/dialect/source-file identity is the CLI's to add) and
    /// section-less (the flat engine is a single implicit section 0, based at
    /// `origin`). Capturing it never changes an emitted byte.
    pub debug: DebugData,
}

/// The engine's slice of a `.debug198x` record: typed symbols and line→address
/// spans, in the CPU's **address units** (a decle for the word-addressed CP1610,
/// a byte elsewhere) so a consumer's address lookups line up with the CPU's own
/// addressing. Header-less; the CLI wraps it with identity and the source
/// filename to form a full [`debug198x::DebugInfo`].
// No `Eq`: `debug198x::Symbol` may carry a `Space::Unknown` holding arbitrary
// JSON, which is `PartialEq` but not `Eq`. Nothing compares these as map keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DebugData {
    /// Every label (address), `equ`/`=` constant (value), and entry point.
    pub symbols: Vec<debug198x::Symbol>,
    /// One span per source-bearing statement that emitted bytes. Fill from `org`
    /// gaps and `align` carries no span (the padding rule).
    pub lines: Vec<LineRec>,
}

/// A line→address span before the source filename is attached: `length` address
/// units at section-relative `offset` were produced by `line`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LineRec {
    pub line: u32,
    pub offset: u64,
    pub length: u64,
    /// The file `line` counts within (language-surface U2): the root input for
    /// a single-file assemble, an include's `FileId` otherwise. Additive per
    /// KTD7 — absent means the root, and a root value is not serialized, so
    /// pre-multi-file payloads are byte-identical. U9 renders these; the
    /// engine just carries the data.
    #[serde(default, skip_serializing_if = "FileId::is_root")]
    pub file: FileId,
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
#[non_exhaustive]
pub struct Warning {
    pub line: usize,
    pub message: String,
    /// The file `line` counts within (language-surface U2) — a warning raised
    /// inside an include names that include. Additive per KTD7: absent means
    /// the root, and a root value is not serialized, so pre-multi-file
    /// payloads are byte-identical.
    #[serde(default, skip_serializing_if = "FileId::is_root")]
    pub file: FileId,
}

impl Warning {
    pub(crate) fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
            file: FileId(0),
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
    /// The larger of the two (ca65 `.max`).
    Max,
    /// The smaller of the two (ca65 `.min`).
    Min,
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
        // ca65's `.max`/`.min`. Binary operations rather than a dedicated node:
        // they take two values and produce one, which is what `BinOp` is for.
        BinOp::Max => a.max(b),
        BinOp::Min => a.min(b),
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
    Align { andmask: i64, value: i64, fill: u8 },
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
    AlignTo { modulus: i64, fill: u8 },
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
    /// `at` is where the section's bytes sit **in the image**, when that is not
    /// the same thing as where the CPU sees them. A banked Game Boy section is
    /// addressed at `$4000` whichever bank it is in, and lands at
    /// `bank * $4000` in the ROM; the NES segments are the same shape, with
    /// file offsets a linker config fixes. `None` means the two coincide,
    /// which is every flat dialect.
    Section {
        name: String,
        base: Option<i64>,
        at: Option<i64>,
    },
    /// A diagnostic the **source** asked for: ACME's `!error`/`!warn`, lwasm's
    /// `error`, rgbasm's `FAIL`/`WARN`. `fatal` aborts the assembly; otherwise
    /// it joins the warnings and assembly continues.
    ///
    /// An operation rather than a parse-time error on purpose: every dialect
    /// that has one also has conditional assembly, and `!error` inside an
    /// untaken `!if` must stay silent. Raising it at parse time would fire on
    /// a branch the program never takes.
    Diagnose { fatal: bool, message: String },
    /// Reserve `count` address units without emitting source-derived data (the
    /// `ds`/`rmb`/`res`/`block` directives). Deliberately **not**
    /// [`Operation::Bytes`] of zeros: what fills reserved space is a property of
    /// the dialect's toolchain, not of the source, so the engine fills it with
    /// [`Dialect::gap_fill`] — the same value an `org` gap takes. Emitting zeros
    /// here is what made the asl-family output diverge from asl + p2bin, which
    /// leaves a gap and fills it with `$FF`.
    Reserve(usize),
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
pub(crate) fn next_pc(
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
        // A binary payload occupies whole address units, the final partial unit
        // zero-padded in pass 2 — so both passes count the same.
        Operation::Binary(payload) => pc + payload.len().div_ceil(addr_unit as usize) as i64,
        Operation::Words(items) => pc + 2 * items.len() as i64 / addr_unit,
        Operation::Instruction { mnemonic, mode, .. } => {
            pc + form(set, ext, mnemonic, mode, line)?.len() as i64 / addr_unit
        }
        Operation::Encoded(pieces) => pc + pieces.iter().map(Piece::len).sum::<i64>() / addr_unit,
        Operation::Align { andmask, value, .. } => pc + ((value - pc) & andmask),
        Operation::AlignTo { modulus, .. } => pc + align_pad(pc, *modulus),
        // Emits nothing; it only speaks.
        Operation::Diagnose { .. } => pc,
        // Moves the counter rather than advancing it; the caller sets it.
        Operation::Section { .. } => pc,
        // Set the counter, bind a name, name an entry point — none of them a
        // width. A caller that can reach these handles them itself.
        Operation::Org(_) | Operation::Equ(_) | Operation::Entry(_) => pc,
    })
}

/// How far short of the next multiple of `modulus` the counter stands. Zero
/// when already there, and for a `modulus` of 1 or less (no boundary to reach).
fn align_pad(pc: i64, modulus: i64) -> i64 {
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
pub(crate) struct Statement {
    pub(crate) line: usize,
    /// The file `line` counts within (language-surface U2). `FileId(0)` for
    /// every single-file parse; the include-capable walks mint real ids so an
    /// engine error deep in an include names the right file.
    pub(crate) file: FileId,
    pub(crate) label: Option<String>,
    pub(crate) op: Option<Operation>,
    /// The operand field's source position, when the dialect parse knew it
    /// (contract U3, [`crate::ast::operand_span`]). Pass-2 range errors point
    /// here; `None` keeps them line-granular (contract KTD1).
    pub(crate) operand_span: Option<Span>,
}

impl Statement {
    /// A line-granular error at this statement, carrying its `(file, line)`
    /// as a real span — so a failure inside an included file renders with
    /// that file's name, not a bare line number.
    fn err(&self, message: impl Into<String>) -> AsmError {
        AsmError::at(Span::in_file(self.file, self.line as u32, 0), message)
    }

    /// Stamp this statement's file onto an error raised by a line-only helper
    /// (expression evaluation, form lookup): a span-less error gains a
    /// line-granular span in this statement's file; an error that already
    /// carries a span (an operand-accurate one) is left alone.
    fn stamp(&self, mut e: AsmError) -> AsmError {
        if e.span.is_none() && e.line != 0 {
            e.span = Some(Span::in_file(self.file, e.line as u32, 0));
        }
        e
    }

    /// An operand-range error: at the operand's span when the parse supplied
    /// one (a column-accurate diagnostic, contract U3), else line-granular.
    fn operand_err(&self, message: impl Into<String>) -> AsmError {
        match &self.operand_span {
            Some(span) => AsmError::at(span.clone(), message),
            None => self.err(message),
        }
    }
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
    let (statements, warnings) = dialect.parse_warned(source)?;
    assemble_statements(statements, warnings, dialect)
}

/// Assemble a multi-file program (language-surface U2): the root is
/// `FileId(0)` in `map`, and the dialect's include-capable parse resolves
/// `INCLUDE` directives through `loader`, minting further ids as it goes. The
/// two-pass driver below is the same one [`assemble`] uses — only the parse
/// differs.
///
/// # Errors
/// As [`assemble`]; errors raised inside an included file carry that file's
/// `FileId` in their span.
pub(crate) fn assemble_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    dialect: &dyn Dialect,
) -> Result<Assembly, AsmError> {
    let (statements, warnings) = dialect.parse_multi_warned(map, loader)?;
    assemble_statements(statements, warnings, dialect)
}

/// The shared two-pass driver over an already-parsed statement stream — the
/// single body behind [`assemble`] and [`assemble_multi`], so the single- and
/// multi-file paths cannot drift.
/// Lay a program's sections into one image.
///
/// The single placement implementation: the engine's own section path and the
/// dialects whose toolchain builds a container (ca65's NES ROM) both come
/// through here, so "where does a section go" is answered once
/// (`decisions/sections-in-the-shared-engine.md`).
///
/// Sections are placed **by image position, not by source order** — a section
/// may sit below one written before it, which is what an origin could never
/// express. A section's position is its own `at` where it has one, since a
/// banked or config-placed section is addressed somewhere the image does not
/// put it, and otherwise its address.
///
/// `image_base` fixes where the image starts when the container does rather
/// than the program; `image_size` pads it out. Both are `None`/no-op for a
/// dialect whose image is just what the source wrote.
pub(crate) fn lay_out(
    mut runs: Vec<Run>,
    gap_fill: u8,
    addr_unit: i64,
    image_base: Option<i64>,
    image_size: impl Fn(&[u8]) -> Option<usize>,
) -> Result<(i64, Vec<u8>), AsmError> {
    runs.retain(|r| !r.bytes.is_empty());
    if runs.is_empty() {
        return Ok((image_base.unwrap_or(0), Vec::new()));
    }
    let placed = |r: &Run| r.at.unwrap_or(r.base);
    runs.sort_by_key(placed);
    let origin = image_base.unwrap_or(runs[0].base);
    let first = image_base.unwrap_or_else(|| placed(&runs[0]));
    let mut image: Vec<u8> = Vec::new();
    for r in &runs {
        let at = ((placed(r) - first) * addr_unit) as usize;
        if at < image.len() {
            return Err(AsmError::new(
                0,
                format!(
                    "section `{}` at {:#06x} overlaps the section before it",
                    r.name,
                    placed(r)
                ),
            ));
        }
        image.resize(at, gap_fill);
        image.extend_from_slice(&r.bytes);
    }
    if let Some(size) = image_size(&image) {
        image.resize(size, gap_fill);
    }
    Ok((origin, image))
}

/// One section's placed bytes, closed off when the next section opens.
///
/// The engine's whole section model: a program is a list of these, and a
/// dialect with no section concept produces exactly one. Sections are internal
/// — they are laid into the flat [`Assembly`] before anything outside the
/// engine sees them (`decisions/sections-in-the-shared-engine.md`).
pub(crate) struct Run {
    pub(crate) name: String,
    /// The address the CPU sees these bytes at — what labels resolve to.
    pub(crate) base: i64,
    /// Where the bytes sit in the image, when that differs from `base`.
    pub(crate) at: Option<i64>,
    pub(crate) bytes: Vec<u8>,
}

// No written-range fields here: the reserve-rather-than-materialise trim
// (`Dialect::trims_trailing_gap`) belongs to the asl family, and no asl dialect
// has sections. A sectioned dialect that trimmed would need the range per
// section, and this is where it would go.

fn assemble_statements(
    statements: Vec<Statement>,
    parse_warnings: Vec<Warning>,
    dialect: &dyn Dialect,
) -> Result<Assembly, AsmError> {
    let set = dialect.instruction_set();
    let ext = dialect.extension_set();

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
                .ok_or_else(|| s.err("`equ` needs a label"))?;
            let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
            if let Some(range) = dialect.equ_range()
                && !range.contains(&v)
            {
                return Err(s.err(format!(
                    "equ value {v} out of range {}..={}",
                    range.start(),
                    range.end()
                )));
            }
            if symbols.insert(label.clone(), v).is_some() {
                return Err(s.err(format!("duplicate label `{label}`")));
            }
            continue;
        }
        if let Some(label) = &s.label {
            if !(0..=0xFF_FFFF).contains(&pc) {
                return Err(s.err("address out of range"));
            }
            if symbols.insert(label.clone(), pc).is_some() {
                return Err(s.err(format!("duplicate label `{label}`")));
            }
        }
        match &s.op {
            None => {}
            Some(Operation::Org(e)) => {
                let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                if !(0..=0xFFFF).contains(&v) {
                    return Err(s.err("origin address out of range"));
                }
                pc = v;
                origin.get_or_insert(v);
            }
            Some(Operation::Section {
                base: Some(base), ..
            }) => {
                pc = *base;
                origin.get_or_insert(*base);
            }
            // A section whose address its linker chooses: nothing to move to.
            Some(Operation::Section { base: None, .. }) => {}
            Some(
                Operation::Bytes(_)
                | Operation::Words(_)
                | Operation::Instruction { .. }
                | Operation::Encoded(_)
                | Operation::Binary(_)
                | Operation::Align { .. }
                | Operation::AlignTo { .. },
            ) if require_origin && origin.is_none() => {
                return Err(s.err(
                    "program counter undefined — set an origin (`*=`) before any code or data",
                ));
            }
            Some(op) => {
                pc = next_pc(op, pc, set, ext, addr_unit, s.line).map_err(|err| s.stamp(err))?;
            }
        }
    }
    let mut origin = origin.unwrap_or(0);

    // Pass 2 — emit.
    let byte_policy = dialect.oversized_byte_policy();
    let gap_fill = dialect.gap_fill();
    // The image length as of the last operation that actually *wrote* data.
    // Reservations and `org` gaps advance the counter without contributing, so
    // on a dialect whose toolchain reserves rather than materialises, anything
    // past this point is trimmed (see `Dialect::trims_trailing_gap`).
    let mut written_len = 0usize;
    // Where the first written byte landed. Anything before it is `org` gap or
    // reservation, which `p2bin` does not put in the image at all — it starts
    // the file at the lowest *written* address, so a leading gap shifts the
    // load address instead of padding it.
    let mut written_start: Option<usize> = None;
    // The parse's advisories come first: they describe the source, and the
    // layout's describe what the source turned into.
    let mut warnings: Vec<Warning> = parse_warnings;
    let mut start: Option<u16> = None;
    let mut bytes: Vec<u8> = Vec::new();
    // Sections closed so far. A dialect with no section concept never opens
    // one, leaves this empty, and takes the single-run path below — which is
    // the flat engine unchanged.
    let mut runs: Vec<Run> = Vec::new();
    let mut section_name = String::new();
    let mut section_at: Option<i64> = None;
    let mut debug = DebugData::default();
    for s in &statements {
        // The location counter (`$`) is the address of this statement's start,
        // in address units (bytes divided by `addr_unit`).
        let pc = origin + bytes.len() as i64 / addr_unit;
        let len_before = bytes.len();
        match &s.op {
            None => {}
            Some(Operation::Org(e)) => {
                let target = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                let cur = origin + bytes.len() as i64 / addr_unit;
                if target < cur {
                    return Err(s.err("cannot move origin backwards"));
                }
                bytes.resize(
                    bytes.len() + ((target - cur) * addr_unit) as usize,
                    gap_fill,
                );
            }
            Some(Operation::Section { name, base, at }) => {
                // Close the run so far and start one at the new base. Bytes
                // are per-section from here, so the location counter, the
                // written-range trims and the 64K check all measure within
                // this section rather than across the image.
                if let Some(base) = base {
                    runs.push(Run {
                        name: std::mem::take(&mut section_name),
                        base: origin,
                        at: section_at.take(),
                        bytes: std::mem::take(&mut bytes),
                    });
                    origin = *base;
                    written_len = 0;
                    written_start = None;
                }
                section_name = name.clone();
                section_at = *at;
            }
            Some(Operation::Equ(_)) => {} // defines a symbol; emits nothing
            Some(Operation::Entry(e)) => {
                let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                if !(0..=0xFFFF).contains(&v) {
                    return Err(s.err("entry address out of range"));
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
            Some(Operation::AlignTo { modulus, fill }) => {
                let pad = align_pad(pc, *modulus);
                bytes.extend(std::iter::repeat_n(*fill, pad as usize));
            }
            Some(Operation::Diagnose { fatal, message }) => {
                if *fatal {
                    return Err(s.err(message));
                }
                warnings.push(Warning {
                    line: s.line,
                    file: s.file,
                    message: message.clone(),
                });
            }
            Some(Operation::Bytes(items)) => {
                for e in items {
                    let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                    emit_byte(&mut bytes, v, byte_policy, &mut warnings, s)?;
                }
            }
            Some(Operation::Reserve(count)) => {
                bytes.extend(std::iter::repeat_n(gap_fill, count * addr_unit as usize));
            }
            Some(Operation::Binary(payload)) => {
                // Asset data, laid down verbatim — never through the per-byte
                // oversize policy (it is data, not source values). An empty
                // payload is legal (a zero-length incbin) but advisory, the
                // posture sjasmplus takes (`requested to include no data`).
                if payload.is_empty() {
                    warnings.push(Warning {
                        line: s.line,
                        message: "binary inclusion included no data".to_string(),
                        file: s.file,
                    });
                }
                bytes.extend_from_slice(payload);
                // Pad the final partial address unit so the location counter
                // and the byte stream stay in step (pass 1 counted whole units).
                let slack = (addr_unit - payload.len() as i64 % addr_unit) % addr_unit;
                bytes.extend(std::iter::repeat_n(0u8, slack as usize));
            }
            Some(Operation::Words(items)) => {
                for e in items {
                    let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                    push_word(&mut bytes, v, s, set.endianness)?;
                }
            }
            Some(Operation::Instruction {
                mnemonic,
                mode,
                operands,
            }) => {
                let f = form(set, ext, mnemonic, mode, s.line).map_err(|err| s.stamp(err))?;
                if operands.len() != f.operands.len() {
                    return Err(s.err(format!(
                        "internal: `{mnemonic}` {mode} takes {} operand value(s), got {}",
                        f.operands.len(),
                        operands.len()
                    )));
                }
                let next_addr = origin + bytes.len() as i64 + f.len() as i64;
                bytes.extend_from_slice(f.opcode);
                for (slot, e) in f.operands.iter().zip(operands.iter()) {
                    let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                    match slot.kind {
                        // Immediates and addresses lay down a value of the
                        // slot's width; only the width matters on the wire, so
                        // they share a path. (A 6502 immediate is always one
                        // byte; a Z80 `LD BC,nn` immediate is two.)
                        isa::OperandKind::Immediate | isa::OperandKind::Address => {
                            match slot.bytes {
                                1 => emit_byte(&mut bytes, v, byte_policy, &mut warnings, s)?,
                                2 => push_word(&mut bytes, v, s, set.endianness)?,
                                // 24-bit address (65816 long addressing).
                                3 => push_addr24(&mut bytes, v, s, set.endianness)?,
                                other => {
                                    return Err(s.err(format!("unsupported operand width {other}")));
                                }
                            }
                        }
                        // A big-endian immediate (Z80N `push nn`): high byte
                        // first, regardless of the set's little-endian default.
                        isa::OperandKind::ImmediateBe => {
                            push_word(&mut bytes, v, s, isa::Endianness::Big)?;
                        }
                        // A signed index displacement, e.g. the `d` in (IX+d).
                        isa::OperandKind::Displacement => {
                            if !(-128..=127).contains(&v) {
                                return Err(s.operand_err(format!(
                                    "displacement {v} out of range (-128..=127)"
                                )));
                            }
                            bytes.push(v as i8 as u8);
                        }
                        isa::OperandKind::RelativePc => {
                            let offset = v - next_addr;
                            match slot.bytes {
                                1 => {
                                    if !(-128..=127).contains(&offset) {
                                        return Err(s.operand_err(format!(
                                            "branch target out of range ({offset} bytes; must be -128..=127)"
                                        )));
                                    }
                                    bytes.push(offset as i8 as u8);
                                }
                                // 16-bit relative (65816 brl/per).
                                2 => {
                                    if !(-32768..=32767).contains(&offset) {
                                        return Err(s.operand_err(format!(
                                            "long branch target out of range ({offset} bytes; must be -32768..=32767)"
                                        )));
                                    }
                                    push_word(&mut bytes, offset & 0xFFFF, s, set.endianness)?;
                                }
                                other => {
                                    return Err(
                                        s.err(format!("unsupported relative width {other}"))
                                    );
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
                            let raw = expr
                                .eval(&symbols, pc, s.line)
                                .map_err(|err| s.stamp(err))?;
                            // A branch offset is relative to the address that
                            // follows this value (the next instruction).
                            let next = origin + bytes.len() as i64 + i64::from(*width);
                            let v = if *rel { raw - next } else { raw };
                            emit_value(&mut bytes, v, *width, *rel || *signed, set.endianness, s)?;
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
                            let raw = expr
                                .eval(&symbols, pc, s.line)
                                .map_err(|err| s.stamp(err))?;
                            if *scale != 1 && raw % *scale != 0 {
                                return Err(s.operand_err(format!(
                                    "{what} ({raw}) is not a multiple of {scale}"
                                )));
                            }
                            let v = raw / *scale;
                            if !(*min..=*max).contains(&v) {
                                return Err(s.operand_err(format!(
                                    "{what} out of range ({v}; must be {min}..={max})"
                                )));
                            }
                            let packed = i64::from((v as u32 & *mask) | *or_bits);
                            emit_value(&mut bytes, packed, *width, false, set.endianness, s)?;
                        }
                        Piece::Branch {
                            target,
                            base,
                            dir_bit,
                            what,
                        } => {
                            let tgt = target
                                .eval(&symbols, pc, s.line)
                                .map_err(|err| s.stamp(err))?;
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
                                return Err(
                                    s.operand_err(format!("{what} out of range ({d} words)"))
                                );
                            }
                            emit_value(&mut bytes, word1, 2, false, set.endianness, s)?;
                            emit_value(&mut bytes, mag, 2, false, set.endianness, s)?;
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
                debug198x::SymbolKind::Const {
                    value: value as u64,
                }
            } else {
                // A label lives at this statement's address (`pc`).
                debug198x::SymbolKind::Label {
                    section: 0,
                    offset: (pc - origin) as u64,
                    space: None,
                }
            };
            debug.symbols.push(debug198x::Symbol {
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
                    | Operation::Binary(_)
                    | Operation::Reserve(_)
            )
        );
        if !matches!(s.op, Some(Operation::Org(_)) | Some(Operation::Reserve(_))) {
            written_len = bytes.len();
            if bytes.len() > len_before {
                written_start.get_or_insert(len_before);
            }
        }
        if source_bearing && bytes.len() > len_before {
            debug.lines.push(LineRec {
                line: s.line as u32,
                offset: (pc - origin) as u64,
                length: ((bytes.len() - len_before) as i64 / addr_unit) as u64,
                file: s.file,
            });
        }
        // The entry point (`end <addr>`) is an Entry symbol. When it targets a
        // bare label, upgrade that label's kind in place (the entry *is* that
        // location) rather than emitting a second same-named symbol; otherwise
        // record a fresh `@entry`.
        if let (Some(Operation::Entry(e)), Some(v)) = (&s.op, start) {
            let entry = debug198x::SymbolKind::Entry {
                section: 0,
                offset: (i64::from(v) - origin) as u64,
                space: None,
            };
            let existing = match e {
                Expr::Sym(n) => debug.symbols.iter_mut().find(|s| {
                    s.name == *n && matches!(s.kind, debug198x::SymbolKind::Label { .. })
                }),
                _ => None,
            };
            if let Some(sym) = existing {
                sym.kind = entry;
            } else {
                let name = match e {
                    Expr::Sym(n) => n.clone(),
                    _ => "@entry".to_string(),
                };
                debug.symbols.push(debug198x::Symbol { name, kind: entry });
            }
        }

        // The 64K cap, checked as bytes land so the error carries the span of
        // the statement that crossed it (`bytes` only grows, so the first
        // offender fires) — a failure in an included file names that file.
        // The cap is on the **address space**, so the byte count converts to
        // address units first (a CP1610 decle is 2 bytes but 1 address); a
        // trailing partial unit still occupies its address.
        if origin + bytes.len().div_ceil(addr_unit as usize) as i64 > 0x1_0000 {
            return Err(s.err("program exceeds the 64K address space"));
        }
    }

    // Lay the sections into one image. Only a dialect that opened a section
    // reaches this; the flat path below is unchanged for the other twenty.
    if !runs.is_empty() {
        runs.push(Run {
            name: section_name,
            base: origin,
            at: section_at,
            bytes,
        });
        let (origin_of_image, image) =
            lay_out(runs, gap_fill, addr_unit, dialect.image_base(), |img| {
                dialect.image_size(img)
            })?;
        return Ok(Assembly {
            origin: origin_of_image as u16,
            reserved_prefix: 0,
            bytes: image,
            symbols,
            start,
            warnings,
            debug,
        });
    }

    // asl reserves rather than materialises: `p2bin` fills only the gaps *inside*
    // the written range. Outside it, both ends fall away — a trailing
    // reservation is absent from the image, and a leading one moves where the
    // image starts rather than padding it. The two trims are the same rule read
    // from each end, so they live together (#66, #90).
    let mut reserved_prefix = 0u16;
    if dialect.trims_trailing_gap() {
        bytes.truncate(written_len);
        match written_start {
            Some(first) if first > 0 => {
                bytes.drain(..first);
                let units = first as i64 / addr_unit;
                origin += units;
                reserved_prefix = units as u16;
            }
            // Nothing was ever written, so `truncate` already emptied the image
            // and there is no load address to move.
            _ => {}
        }
    }

    Ok(Assembly {
        origin: origin as u16,
        reserved_prefix,
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
    s: &Statement,
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
            return Err(s.err(format!("unsupported value width {other}")));
        }
    };
    if !(lo..=hi).contains(&v) {
        return Err(s.operand_err(format!("value {v} out of range for a {width}-byte operand")));
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
    s: &Statement,
) -> Result<(), AsmError> {
    if !(-128..=0xFF).contains(&v) {
        match policy {
            Oversize::Error => {
                return Err(s.operand_err(format!("value {v} does not fit in a byte")));
            }
            Oversize::Truncate => {}
            Oversize::TruncateWarn => {
                warnings.push(Warning {
                    line: s.line,
                    message: format!("value {v} truncated to a byte"),
                    file: s.file,
                });
            }
        }
    }
    bytes.push((v & 0xFF) as u8);
    Ok(())
}

fn push_word(
    bytes: &mut Vec<u8>,
    v: i64,
    s: &Statement,
    endianness: isa::Endianness,
) -> Result<(), AsmError> {
    if !(0..=0xFFFF).contains(&v) {
        return Err(s.operand_err(format!("value {v} does not fit in a word")));
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
    s: &Statement,
    endianness: isa::Endianness,
) -> Result<(), AsmError> {
    if !(0..=0xFF_FFFF).contains(&v) {
        return Err(s.operand_err(format!("value {v} does not fit in a 24-bit address")));
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
