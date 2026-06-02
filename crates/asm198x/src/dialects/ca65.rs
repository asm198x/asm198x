//! The ca65 (NES) dialect, with a bounded ld65-style linker for the one fixed
//! NES configuration the curriculum uses.
//!
//! ca65 is an assembler whose output is linked by ld65 into the final ROM, so
//! producing a byte-identical `.nes` means doing both jobs. The 6502 operand and
//! expression machinery is shared in [`super::mos6502`]; this module adds ca65's
//! surface (`.segment`, `.byte`/`.word`/`.res`, `=` constants, `name:` and
//! `@cheap` labels, `<`/`>` binding tight) and a small linker that places the
//! segments into the standard NROM layout.
//!
//! Every NES unit in the curriculum links with the same `nes.cfg`, so that
//! layout is encoded directly here rather than parsed from a config file —
//! `iNES header (16) + PRG ($8000, 32K, fill $00) + CHR (8K, fill $00)`, with
//! `CODE` at `$8000` and `VECTORS` at `$FFFA`. See `decisions/syntax-stance.md`.

use std::collections::BTreeMap;

use super::mos6502::{self, fold_const, is_ident, split_first_word, split_top_level, BytePrec};
use crate::engine::{AsmError, Expr};

// ---------------------------------------------------------------------------
// The fixed NES (NROM) layout
// ---------------------------------------------------------------------------

/// PRG ROM occupies the upper 32K of the CPU address space.
const PRG_BASE: u32 = 0x8000;
const PRG_SIZE: usize = 0x8000;
const CHR_SIZE: usize = 0x2000;
const HEADER_SIZE: usize = 0x10;
const FILL: u8 = 0x00;

/// The base address of a segment, and whether it contributes bytes to the ROM.
struct SegInfo {
    base: u32,
    in_file: bool,
}

fn seg_info(seg: &str) -> Option<SegInfo> {
    let (base, in_file) = match seg {
        "ZEROPAGE" => (0x0000, false),
        "OAM" => (0x0200, false),
        "BSS" => (0x0300, false),
        "HEADER" => (0x0000, true),
        "CODE" => (0x8000, true),
        "VECTORS" => (0xFFFA, true),
        "CHARS" => (0x0000, true),
        _ => return None,
    };
    Some(SegInfo { base, in_file })
}

// ---------------------------------------------------------------------------
// Parsed statements
// ---------------------------------------------------------------------------

enum Kind {
    Empty,
    Bytes(Vec<Expr>),
    Words(Vec<Expr>),
    /// `.res count [, fill]` — `count` bytes of `fill`.
    Res(usize, u8),
    Insn {
        operand: mos6502::OperandSyntax,
        mnemonic: String,
    },
}

struct Stmt {
    line: usize,
    seg: String,
    label: Option<String>,
    kind: Kind,
}

struct Parsed {
    stmts: Vec<Stmt>,
    /// Each label's segment, for the zero-page-vs-absolute decision.
    label_seg: BTreeMap<String, String>,
    /// `=` constants, folded in source order.
    consts: BTreeMap<String, i64>,
}

// ---------------------------------------------------------------------------
// Entry point: assemble + link
// ---------------------------------------------------------------------------

/// Assemble ca65 source and link it into a `.nes` ROM image.
///
/// # Errors
/// Returns an [`AsmError`] on any parse, range, or symbol-resolution failure.
pub(crate) fn assemble(source: &str) -> Result<Vec<u8>, AsmError> {
    let set = &isa::mos6502::SET;
    let parsed = parse(set, source)?;

    // The address-size environment: constants by value, plus zero-page labels
    // pinned below $100 so the shared mode picker selects the short form.
    let mut size_env = parsed.consts.clone();
    for (name, seg) in &parsed.label_seg {
        if seg == "ZEROPAGE" {
            size_env.insert(name.clone(), 0);
        }
    }

    // Layout pass: resolve each instruction's mode and size, lay statements out
    // within their segment, and record every label's absolute address.
    let mut offsets: BTreeMap<String, u32> = BTreeMap::new();
    let mut addr_env: BTreeMap<String, u16> = BTreeMap::new();
    for (name, value) in &parsed.consts {
        addr_env.insert(name.clone(), *value as u16);
    }
    let mut placed: Vec<(String, u32, usize, Resolved)> = Vec::new(); // (segment, addr, line, item)
    for stmt in parsed.stmts {
        let info = seg_info(&stmt.seg)
            .ok_or_else(|| AsmError::new(stmt.line, format!("unknown segment `{}`", stmt.seg)))?;
        let off = *offsets.entry(stmt.seg.clone()).or_insert(0);
        let addr = info.base + off;
        if let Some(label) = &stmt.label {
            addr_env.insert(label.clone(), addr as u16);
        }
        let (resolved, size) = resolve(set, stmt.kind, &size_env, stmt.line)?;
        *offsets.get_mut(&stmt.seg).expect("segment offset") += size as u32;
        if !matches!(resolved, Resolved::Nothing) {
            placed.push((stmt.seg, addr, stmt.line, resolved));
        }
    }

    // Emit pass: turn each placed item into bytes, per segment.
    let mut seg_bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (seg, addr, line, item) in placed {
        if !seg_info(&seg).expect("seg").in_file {
            continue; // bss/zp segments occupy address space but emit no file bytes
        }
        let buf = seg_bytes.entry(seg).or_default();
        emit(item, addr, &addr_env, buf, line)?;
    }

    link(&seg_bytes)
}

/// Lay the file segments into the NROM ROM image.
fn link(seg_bytes: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, AsmError> {
    let empty = Vec::new();
    let get = |s: &str| seg_bytes.get(s).unwrap_or(&empty);

    // iNES header (16 bytes, zero-padded).
    let header = get("HEADER");
    let mut rom = vec![FILL; HEADER_SIZE];
    rom[..header.len().min(HEADER_SIZE)].copy_from_slice(&header[..header.len().min(HEADER_SIZE)]);

    // PRG: CODE at $8000, VECTORS at $FFFA, gap filled.
    let mut prg = vec![FILL; PRG_SIZE];
    let code = get("CODE");
    let vectors = get("VECTORS");
    place(&mut prg, 0, code, "CODE")?;
    place(&mut prg, (0xFFFA - PRG_BASE) as usize, vectors, "VECTORS")?;
    rom.extend_from_slice(&prg);

    // CHR: CHARS from the start, filled.
    let mut chr = vec![FILL; CHR_SIZE];
    place(&mut chr, 0, get("CHARS"), "CHARS")?;
    rom.extend_from_slice(&chr);

    Ok(rom)
}

fn place(region: &mut [u8], at: usize, bytes: &[u8], name: &str) -> Result<(), AsmError> {
    let end = at + bytes.len();
    if end > region.len() {
        return Err(AsmError::new(0, format!("segment `{name}` overflows its region")));
    }
    region[at..end].copy_from_slice(bytes);
    Ok(())
}

// ---------------------------------------------------------------------------
// Resolution and emission
// ---------------------------------------------------------------------------

enum Resolved {
    Nothing,
    Bytes(Vec<Expr>),
    Words(Vec<Expr>),
    Fill(usize, u8),
    Insn {
        form: &'static isa::Form,
        operands: Vec<Expr>,
    },
}

/// Resolve a parsed statement to an emittable item plus its byte size.
fn resolve(
    set: &'static isa::InstructionSet,
    kind: Kind,
    size_env: &BTreeMap<String, i64>,
    line: usize,
) -> Result<(Resolved, usize), AsmError> {
    Ok(match kind {
        Kind::Empty => (Resolved::Nothing, 0),
        Kind::Bytes(v) => {
            let n = v.len();
            (Resolved::Bytes(v), n)
        }
        Kind::Words(v) => {
            let n = v.len() * 2;
            (Resolved::Words(v), n)
        }
        Kind::Res(count, fill) => (Resolved::Fill(count, fill), count),
        Kind::Insn { operand, mnemonic } => {
            let insn = set
                .instruction(&mnemonic)
                .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))?;
            let (mode, operand) = mos6502::resolve_mode(insn, operand, size_env, line)?;
            let form = insn
                .form(mode)
                .ok_or_else(|| AsmError::new(line, format!("`{mnemonic}` has no {mode} form")))?;
            let operands: Vec<Expr> = operand.into_iter().collect();
            let size = form.len();
            (Resolved::Insn { form, operands }, size)
        }
    })
}

/// Emit one resolved item's bytes at address `addr`.
fn emit(
    item: Resolved,
    addr: u32,
    env: &BTreeMap<String, u16>,
    out: &mut Vec<u8>,
    line_for_errors: usize,
) -> Result<(), AsmError> {
    let pc = i64::from(addr);
    match item {
        Resolved::Nothing => {}
        Resolved::Fill(count, fill) => out.extend(std::iter::repeat_n(fill, count)),
        Resolved::Bytes(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                out.push(to_byte(v, line_for_errors)?);
            }
        }
        Resolved::Words(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u16::try_from(v & 0xFFFF).expect("masked");
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
        Resolved::Insn { form, operands } => {
            let next = pc + form.len() as i64;
            out.extend_from_slice(form.opcode);
            for (slot, e) in form.operands.iter().zip(operands.iter()) {
                let v = e.eval(env, pc, line_for_errors)?;
                match slot.kind {
                    isa::OperandKind::Immediate | isa::OperandKind::Address => match slot.bytes {
                        1 => out.push(to_byte(v, line_for_errors)?),
                        2 => out.extend_from_slice(&u16::try_from(v & 0xFFFF).expect("masked").to_le_bytes()),
                        other => {
                            return Err(AsmError::new(line_for_errors, format!("unsupported operand width {other}")));
                        }
                    },
                    isa::OperandKind::RelativePc => {
                        let offset = v - next;
                        if !(-128..=127).contains(&offset) {
                            return Err(AsmError::new(
                                line_for_errors,
                                format!("branch target out of range ({offset} bytes)"),
                            ));
                        }
                        out.push(offset as i8 as u8);
                    }
                    isa::OperandKind::Displacement => {
                        return Err(AsmError::new(line_for_errors, "displacement operand not valid on 6502"));
                    }
                }
            }
            out.extend_from_slice(form.suffix);
        }
    }
    Ok(())
}

fn to_byte(v: i64, line: usize) -> Result<u8, AsmError> {
    if (-128..=0xFF).contains(&v) {
        Ok((v & 0xFF) as u8)
    } else {
        Err(AsmError::new(line, format!("value {v} does not fit in a byte")))
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse(set: &'static isa::InstructionSet, source: &str) -> Result<Parsed, AsmError> {
    let mut stmts = Vec::new();
    let mut label_seg: BTreeMap<String, String> = BTreeMap::new();
    let mut consts: BTreeMap<String, i64> = BTreeMap::new();
    let mut seg = "CODE".to_string(); // ca65's default segment
    let mut current_global = String::new();

    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let code = strip_comment(raw);
        let trimmed = code.trim();
        if trimmed.is_empty() {
            continue;
        }

        // `.segment "NAME"` switches the active segment.
        if let Some(rest) = trimmed.strip_prefix(".segment") {
            seg = rest.trim().trim_matches('"').to_string();
            continue;
        }

        // `NAME = expr` defines a constant.
        if let Some(eq) = assignment_split(trimmed) {
            let name = trimmed[..eq].trim();
            if !is_ident(name) {
                return Err(AsmError::new(line, format!("invalid constant name `{name}`")));
            }
            let expr = parse_value(&current_global, &trimmed[eq + 1..], line)?;
            if let Ok(v) = fold_const(&expr, &consts, line) {
                consts.insert(name.to_string(), v);
            }
            continue;
        }

        // An optional `name:` / `@cheap:` label, then an optional operation.
        let (label, rest) = split_label(line, &mut current_global, trimmed)?;
        if let Some(name) = &label {
            label_seg.insert(name.clone(), seg.clone());
        }
        let kind = parse_op(set, &current_global, &consts, rest, line)?;
        if label.is_none() && matches!(kind, Kind::Empty) {
            continue;
        }
        stmts.push(Stmt { line, seg: seg.clone(), label, kind });
    }
    Ok(Parsed { stmts, label_seg, consts })
}

/// Strip a `;` comment, ignoring `;` inside `'c'` or `"..."`.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let (mut in_char, mut in_str) = (false, false);
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_str => in_char = !in_char,
            b'"' if !in_char => in_str = !in_str,
            b';' if !in_char && !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

/// A lone `=` (not `==`/`!=`/`<=`/`>=`), used for constant definitions.
fn assignment_split(trimmed: &str) -> Option<usize> {
    let bytes = trimmed.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'=' {
            let prev = i.checked_sub(1).map(|p| bytes[p]);
            let next = bytes.get(i + 1).copied();
            if !matches!(prev, Some(b'!' | b'<' | b'>' | b'=')) && next != Some(b'=') {
                return Some(i);
            }
        }
    }
    None
}

/// Split a leading `name:` or `@cheap:` label. Updates `current_global` when a
/// non-cheap label is defined (cheap locals scope to the preceding global).
fn split_label<'a>(
    line: usize,
    current_global: &mut String,
    trimmed: &'a str,
) -> Result<(Option<String>, &'a str), AsmError> {
    let (word, remainder) = split_first_word(trimmed);
    let Some(name) = word.strip_suffix(':') else {
        return Ok((None, trimmed));
    };
    if let Some(cheap) = name.strip_prefix('@') {
        if !is_ident(cheap) {
            return Err(AsmError::new(line, format!("invalid cheap-local label `{name}`")));
        }
        return Ok((Some(cheap_key(current_global, cheap)), remainder));
    }
    if !is_ident(name) {
        return Err(AsmError::new(line, format!("invalid label `{name}`")));
    }
    *current_global = name.to_string();
    Ok((Some(name.to_string()), remainder))
}

fn parse_op(
    set: &'static isa::InstructionSet,
    current_global: &str,
    consts: &BTreeMap<String, i64>,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(Kind::Empty);
    }
    if let Some(directive) = rest.strip_prefix('.') {
        return parse_directive(current_global, consts, directive, line);
    }
    let (mnemonic, operand_text) = split_first_word(rest);
    let mnemonic = mnemonic.to_ascii_uppercase();
    let operand = mos6502::parse_operand(operand_text, line, &|s, l| parse_value(current_global, s, l))?;
    if set.instruction(&mnemonic).is_none() {
        return Err(AsmError::new(line, format!("unknown instruction `{mnemonic}`")));
    }
    Ok(Kind::Insn { operand, mnemonic })
}

fn parse_directive(
    current_global: &str,
    consts: &BTreeMap<String, i64>,
    directive: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let (name, rest) = split_first_word(directive);
    match name.to_ascii_lowercase().as_str() {
        "byte" | "byt" => Ok(Kind::Bytes(parse_data_list(current_global, rest, line)?)),
        "word" | "addr" => Ok(Kind::Words(parse_value_list(current_global, rest, line)?)),
        "res" => parse_res(current_global, consts, rest, line),
        other => Err(AsmError::new(line, format!("unsupported directive `.{other}`"))),
    }
}

/// `.res count [, fill]`. `count` must fold to a constant (a literal expression
/// or a `=` constant such as `NUM_ENEMIES`); `fill` defaults to 0.
fn parse_res(
    current_global: &str,
    consts: &BTreeMap<String, i64>,
    rest: &str,
    line: usize,
) -> Result<Kind, AsmError> {
    let mut parts = rest.splitn(2, ',');
    let count_src = parts.next().unwrap_or("").trim();
    let count = fold_const(&parse_value(current_global, count_src, line)?, consts, line)
        .map_err(|_| AsmError::new(line, "`.res` count must be a constant"))?;
    let count = usize::try_from(count)
        .map_err(|_| AsmError::new(line, "`.res` count must be non-negative"))?;
    let fill = match parts.next() {
        None => 0,
        Some(v) => {
            let n = fold_const(&parse_value(current_global, v, line)?, consts, line)?;
            u8::try_from(n).map_err(|_| AsmError::new(line, "`.res` fill must be a byte"))?
        }
    };
    Ok(Kind::Res(count, fill))
}

/// `.byte` list: `"..."` strings expand to raw ASCII bytes; values are bytes.
fn parse_data_list(current_global: &str, rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "`.byte` needs a value"));
    }
    let mut out = Vec::new();
    for piece in split_strings(rest) {
        if let Some(text) = string_literal(piece) {
            out.extend(text.bytes().map(|b| Expr::Num(i64::from(b))));
        } else {
            out.push(parse_value(current_global, piece, line)?);
        }
    }
    Ok(out)
}

fn parse_value_list(current_global: &str, rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "directive needs a value"));
    }
    split_top_level(rest, ',')
        .iter()
        .map(|p| parse_value(current_global, p, line))
        .collect()
}

// ---------------------------------------------------------------------------
// Value parsing over the shared expression core
// ---------------------------------------------------------------------------

/// Parse a ca65 value. A bare `@cheap` operand is a cheap-local reference scoped
/// to the current global; otherwise it is an expression with `<`/`>` binding
/// tight ([`BytePrec::Tight`]).
fn parse_value(current_global: &str, raw: &str, line: usize) -> Result<Expr, AsmError> {
    let t = raw.trim();
    if let Some(cheap) = t.strip_prefix('@')
        && is_ident(cheap)
    {
        return Ok(Expr::Sym(cheap_key(current_global, cheap)));
    }
    mos6502::parse_expr(t, line, parse_number, BytePrec::Tight)
}

/// A collision-proof symbol key for a cheap local, scoped to its global.
fn cheap_key(global: &str, name: &str) -> String {
    format!("{global}\u{1}{name}")
}

/// ca65 numbers: `$hex`, `%binary`, `'c'` char, decimal.
fn parse_number(tok: &str, line: usize) -> Result<i64, AsmError> {
    let t = tok.trim();
    let bad = || AsmError::new(line, format!("invalid number `{tok}`"));
    if let Some(hex) = t.strip_prefix('$') {
        i64::from_str_radix(hex, 16).map_err(|_| bad())
    } else if let Some(bin) = t.strip_prefix('%') {
        i64::from_str_radix(bin, 2).map_err(|_| bad())
    } else if t.starts_with('\'') && t.ends_with('\'') && t.chars().count() == 3 {
        t.chars().nth(1).map(|c| c as i64).ok_or_else(bad)
    } else {
        t.parse::<i64>().map_err(|_| bad())
    }
}

fn split_strings(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_string = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_string = !in_string,
            ',' if !in_string => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

fn string_literal(piece: &str) -> Option<&str> {
    let p = piece.trim();
    (p.len() >= 2 && p.starts_with('"') && p.ends_with('"')).then(|| &p[1..p.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::assemble;

    fn rom(src: &str) -> Vec<u8> {
        assemble(src).expect("assembles")
    }

    #[test]
    fn rom_has_nrom_shape() {
        let r = rom(".segment \"CODE\"\nrts\n");
        assert_eq!(r.len(), 16 + 0x8000 + 0x2000);
    }

    #[test]
    fn header_and_code_and_vectors_place_correctly() {
        let src = "\
.segment \"HEADER\"\n\
    .byte \"NES\", $1A, 2, 1\n\
.segment \"CODE\"\n\
reset:\n\
    sei\n\
nmi:\n\
    rti\n\
irq:\n\
    rti\n\
.segment \"VECTORS\"\n\
    .word nmi, reset, irq\n";
        let r = rom(src);
        // iNES magic.
        assert_eq!(&r[..5], &[0x4E, 0x45, 0x53, 0x1A, 0x02]);
        // CODE at $8000 (file offset 16): sei, rti, rti.
        assert_eq!(&r[16..19], &[0x78, 0x40, 0x40]);
        // reset=$8000, nmi=$8001, irq=$8002. VECTORS at $FFFA (file off 16+0x7FFA).
        let v = 16 + 0x7FFA;
        assert_eq!(&r[v..v + 6], &[0x01, 0x80, 0x00, 0x80, 0x02, 0x80]);
    }

    #[test]
    fn zeropage_label_uses_zp_addressing() {
        let src = "\
.segment \"ZEROPAGE\"\n\
counter: .res 1\n\
.segment \"CODE\"\n\
    sta counter\n";
        let r = rom(src);
        // sta zp = $85 $00 (counter at $00), not abs $8D.
        assert_eq!(&r[16..18], &[0x85, 0x00]);
    }

    #[test]
    fn cheap_locals_scope_to_global() {
        let src = "\
.segment \"CODE\"\n\
one:\n\
@loop:\n\
    jmp @loop\n\
two:\n\
@loop:\n\
    jmp @loop\n";
        let r = rom(src);
        // one@loop at $8000: jmp $8000. two@loop at $8003: jmp $8003.
        assert_eq!(&r[16..22], &[0x4C, 0x00, 0x80, 0x4C, 0x03, 0x80]);
    }
}
