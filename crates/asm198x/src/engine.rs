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

use crate::dialect::Dialect;

/// The result of a successful assembly: where it loads and the bytes to load.
#[derive(Debug, Clone)]
pub struct Assembly {
    /// Load address (first origin directive, or 0 if none given).
    pub origin: u16,
    /// Assembled machine code, contiguous from `origin`.
    pub bytes: Vec<u8>,
    /// Resolved labels, for listings and debugging.
    pub symbols: BTreeMap<String, u16>,
}

/// An assembly error, with the 1-based source line it occurred on (0 = no
/// specific line).
#[derive(Debug, Clone)]
pub struct AsmError {
    pub line: usize,
    pub message: String,
}

impl AsmError {
    pub(crate) fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
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
    /// Low byte of the inner value.
    Lo(Box<Expr>),
    /// High byte of the inner value.
    Hi(Box<Expr>),
    /// Negation of the inner value.
    Neg(Box<Expr>),
    /// A binary operation on two sub-expressions.
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

impl Expr {
    pub(crate) fn eval(
        &self,
        symbols: &BTreeMap<String, u16>,
        line: usize,
    ) -> Result<i64, AsmError> {
        let overflow = || AsmError::new(line, "arithmetic overflow in expression");
        Ok(match self {
            Expr::Num(n) => *n,
            Expr::Sym(s) => i64::from(
                *symbols
                    .get(s)
                    .ok_or_else(|| AsmError::new(line, format!("undefined symbol `{s}`")))?,
            ),
            Expr::Lo(e) => e.eval(symbols, line)? & 0xFF,
            Expr::Hi(e) => (e.eval(symbols, line)? >> 8) & 0xFF,
            Expr::Neg(e) => e.eval(symbols, line)?.checked_neg().ok_or_else(overflow)?,
            Expr::Bin(op, l, r) => {
                let a = l.eval(symbols, line)?;
                let b = r.eval(symbols, line)?;
                match op {
                    BinOp::Add => a.checked_add(b).ok_or_else(overflow)?,
                    BinOp::Sub => a.checked_sub(b).ok_or_else(overflow)?,
                    BinOp::Mul => a.checked_mul(b).ok_or_else(overflow)?,
                    BinOp::Div => {
                        if b == 0 {
                            return Err(AsmError::new(line, "division by zero in expression"));
                        }
                        a.checked_div(b).ok_or_else(overflow)?
                    }
                }
            }
        })
    }
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
    let mut symbols: BTreeMap<String, u16> = BTreeMap::new();
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
            let v = e.eval(&symbols, s.line)?;
            let value = u16::try_from(v).map_err(|_| {
                AsmError::new(s.line, format!("equ value {v} out of range 0..=65535"))
            })?;
            if symbols.insert(label.clone(), value).is_some() {
                return Err(AsmError::new(s.line, format!("duplicate label `{label}`")));
            }
            continue;
        }
        if let Some(label) = &s.label {
            if !(0..=0xFFFF).contains(&pc) {
                return Err(AsmError::new(s.line, "address out of range"));
            }
            if symbols.insert(label.clone(), pc as u16).is_some() {
                return Err(AsmError::new(s.line, format!("duplicate label `{label}`")));
            }
        }
        match &s.op {
            None => {}
            Some(Operation::Org(e)) => {
                let v = e.eval(&symbols, s.line)?;
                if !(0..=0xFFFF).contains(&v) {
                    return Err(AsmError::new(s.line, "origin address out of range"));
                }
                pc = v;
                origin.get_or_insert(v);
            }
            Some(Operation::Bytes(items)) => pc += items.len() as i64,
            Some(Operation::Words(items)) => pc += 2 * items.len() as i64,
            Some(Operation::Instruction { mnemonic, mode, .. }) => {
                pc += form(set, ext, mnemonic, mode, s.line)?.len() as i64;
            }
            Some(Operation::Equ(_)) => {} // handled above
        }
    }
    let origin = origin.unwrap_or(0);

    // Pass 2 — emit.
    let mut bytes: Vec<u8> = Vec::new();
    for s in &statements {
        match &s.op {
            None => {}
            Some(Operation::Org(e)) => {
                let target = e.eval(&symbols, s.line)?;
                let cur = origin + bytes.len() as i64;
                if target < cur {
                    return Err(AsmError::new(s.line, "cannot move origin backwards"));
                }
                bytes.resize(bytes.len() + (target - cur) as usize, 0);
            }
            Some(Operation::Equ(_)) => {} // defines a symbol; emits nothing
            Some(Operation::Bytes(items)) => {
                for e in items {
                    let v = e.eval(&symbols, s.line)?;
                    bytes.push(to_byte(v, s.line)?);
                }
            }
            Some(Operation::Words(items)) => {
                for e in items {
                    let v = e.eval(&symbols, s.line)?;
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
                    let v = e.eval(&symbols, s.line)?;
                    match slot.kind {
                        // Immediates and addresses lay down a value of the
                        // slot's width; only the width matters on the wire, so
                        // they share a path. (A 6502 immediate is always one
                        // byte; a Z80 `LD BC,nn` immediate is two.)
                        isa::OperandKind::Immediate | isa::OperandKind::Address => {
                            match slot.bytes {
                                1 => bytes.push(to_byte(v, s.line)?),
                                2 => push_word(&mut bytes, v, s.line, set.endianness)?,
                                other => {
                                    return Err(AsmError::new(
                                        s.line,
                                        format!("unsupported operand width {other}"),
                                    ));
                                }
                            }
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
                    }
                }
                // Trailing opcode bytes after the operands (Z80 DD CB / FD CB).
                bytes.extend_from_slice(f.suffix);
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
        Err(AsmError::new(line, format!("unknown instruction `{mnemonic}`")))
    }
}

fn to_byte(v: i64, line: usize) -> Result<u8, AsmError> {
    if (0..=0xFF).contains(&v) {
        Ok(v as u8)
    } else if (-128..=-1).contains(&v) {
        Ok(v as i8 as u8)
    } else {
        Err(AsmError::new(
            line,
            format!("value {v} does not fit in a byte"),
        ))
    }
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
