//! The ld65 job (#486's artifact/layout split): the memory account, the
//! link that lays the segments into the layout's image, and the resolve and
//! emit steps that turn a placed statement into bytes. It is its own file so
//! that a change to how bytes land in the ROM is read apart from the parse
//! and the projection sweep. Moved verbatim from the parent; the seam is the
//! boundary, not a rewrite.

use std::collections::BTreeMap;

use super::super::mos6502;
use super::{AsmError, DiagSeverity, Expr, FileId, Kind, Warning, WarningKind, display_label};

/// PRG ROM occupies the upper 32K of the CPU address space — kept for the
/// default layout's CODE/VECTORS overlap message below; every other shape
/// number now lives in the layout value.
const PRG_BASE: u32 = 0x8000;

/// The layout's memory account (#499): each area's capacity against the
/// segments the program placed or reserved in it, in address order. `free` is
/// the total unoccupied span; `largest_free` merges the placed intervals and
/// takes the widest hole — a pinned segment (VECTORS at $FFFA) splits the free
/// space, and a routine needs one piece, not a total.
pub(super) fn area_usage(
    layout_def: &super::ca65_layout::Layout,
    layout: &super::ca65_layout::ResolvedLayout,
    offsets: &BTreeMap<String, u32>,
) -> Vec<crate::engine::AreaUsage> {
    layout_def
        .areas
        .iter()
        .enumerate()
        .map(|(i, area)| {
            let mut segments: Vec<crate::engine::SegmentUsage> = layout_def
                .segments
                .iter()
                .filter(|s| s.area == i)
                .filter_map(|s| {
                    let length = *offsets.get(&s.name)?;
                    let base = layout.seg(&s.name)?.base;
                    Some(crate::engine::SegmentUsage {
                        name: s.name.clone(),
                        base,
                        length,
                    })
                })
                .collect();
            segments.sort_by_key(|s| s.base);
            let used: u32 = segments.iter().map(|s| s.length).sum();
            let end = area.start + area.size;
            let mut largest = 0u32;
            let mut cursor = area.start;
            for s in &segments {
                largest = largest.max(s.base.saturating_sub(cursor));
                cursor = cursor.max(s.base + s.length);
            }
            largest = largest.max(end.saturating_sub(cursor));
            crate::engine::AreaUsage {
                name: area.name.clone(),
                start: area.start,
                size: area.size,
                used,
                free: area.size - used,
                largest_free: largest,
                segments,
            }
        })
        .collect()
}

/// Lay the file segments into the layout's image.
pub(super) fn link(
    seg_bytes: &BTreeMap<String, Vec<u8>>,
    layout: &super::ca65_layout::ResolvedLayout,
) -> Result<Vec<u8>, AsmError> {
    // Every segment the source wrote bytes into, as a run: addressed where the
    // CPU sees it, placed where the config puts it in the file. The engine's
    // `lay_out` does the rest, so the NES ROM is built by the same code that
    // places a Game Boy bank or a flat program's single section.
    let runs: Vec<crate::engine::Run> = layout
        .segs
        .iter()
        .filter_map(|s| {
            let bytes = seg_bytes.get(&s.name)?;
            Some(crate::engine::Run {
                name: s.name.clone(),
                base: i64::from(s.base),
                at: crate::engine::Place::At(s.file_at? as i64),
                bytes: bytes.clone(),
            })
        })
        .collect();

    // CODE running into the vector table would be silently overwritten, giving
    // corrupted code and a debug record describing bytes that did not survive.
    // `lay_out` refuses the overlap, but names only the second segment; ld65
    // names the area that filled, and so does this.
    if let Some(code) = seg_bytes.get("CODE")
        && code.len() > (0xFFFA - PRG_BASE) as usize
    {
        return Err(AsmError::new(
            0,
            format!(
                "segment `CODE` ({} bytes) overlaps `VECTORS` at $FFFA",
                code.len()
            ),
        ));
    }

    // The image is the layout's shape: its file areas stacked and filled.
    let size = layout.image_size;
    let (_, rom) = crate::engine::lay_out(runs, layout.fill, 1, Some(0), false, |_| Some(size))?;
    Ok(rom)
}

// ---------------------------------------------------------------------------
// Resolution and emission
// ---------------------------------------------------------------------------

pub(super) enum Resolved {
    Nothing,
    /// A resolved `.incbin` payload — raw bytes, emitted verbatim (U5).
    Raw(Vec<u8>),
    Bytes(Vec<Expr>),
    Words(Vec<Expr>),
    DBytes(Vec<Expr>),
    DWords(Vec<Expr>),
    Fill(usize, u8),
    Insn {
        form: &'static isa::Form,
        operands: Vec<Expr>,
    },
    /// Something to say, carrying no bytes.
    Message(DiagSeverity, String),
    /// A condition to fold once every symbol is known, and what a failure does.
    Assert(Expr, bool, String),
    /// A visibility claim to check once every symbol is known.
    Visible(VisRule, bool, Vec<String>),
}

/// What a visibility word asks of the name it carries, once assembly and
/// linking are one step over one translation unit.
///
/// Probed against ca65 2.18 + ld65 on 2026-08-24. The three answers do not
/// follow the export/import split the manual's headings suggest: `.global` is
/// in neither camp, and `.forceimport` is in no camp at all — defining its name
/// is `Symbol 'zz' is already an import` and not defining it is an unresolved
/// external, so it is declared `Category::RefusedByReference` instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VisRule {
    /// `.export`, `.exportzp`. ca65 itself answers `Exported symbol 'nope' was
    /// never defined`, whether or not anything references it.
    MustBeDefined,
    /// `.import`, `.importzp`. Defining the name is ca65's `Symbol 'zz' is
    /// already an import`. Leaving it undefined is fine until something reads
    /// it, and then it is an unresolved external — which is the ordinary
    /// undefined-symbol refusal here.
    MustNotBeDefined,
    /// `.global`, `.globalzp` — export if the name is defined here, import if
    /// it is not. Both are legal, so nothing is checked beyond the zero-page
    /// warning the `zp` spelling carries.
    EitherWay,
}

/// Resolve a parsed statement to an emittable item plus its byte size.
pub(super) fn resolve(
    set: &'static isa::InstructionSet,
    kind: Kind,
    size_env: &BTreeMap<String, i64>,
    off: u32,
    line: usize,
) -> Result<(Resolved, usize), AsmError> {
    Ok(match kind {
        Kind::Empty => (Resolved::Nothing, 0),
        // The sweep reads every unread line before projection ends; one
        // reaching layout is a driver bug, not a source error.
        Kind::Unread => unreachable!("an unread line survived the projection sweep"),
        // The only statement whose size depends on where it stands: pad to the
        // next multiple of the boundary, measured from the segment's start.
        Kind::Align(boundary, fill) => {
            let pad = (boundary - i64::from(off).rem_euclid(boundary)) % boundary;
            let pad = usize::try_from(pad).expect("non-negative");
            (Resolved::Fill(pad, fill), pad)
        }
        Kind::Raw(v) => {
            let n = v.len();
            (Resolved::Raw(v), n)
        }
        Kind::Bytes(v) => {
            let n = v.len();
            (Resolved::Bytes(v), n)
        }
        Kind::Words(v) => {
            let n = v.len() * 2;
            (Resolved::Words(v), n)
        }
        Kind::DBytes(v) => {
            let n = v.len() * 2;
            (Resolved::DBytes(v), n)
        }
        Kind::DWords(v) => {
            let n = v.len() * 4;
            (Resolved::DWords(v), n)
        }
        Kind::Res(count, fill) => (Resolved::Fill(count, fill), count),
        Kind::Constant(..) | Kind::Org(_) | Kind::Reloc => (Resolved::Nothing, 0),
        Kind::Message(severity, text) => (Resolved::Message(severity, text), 0),
        Kind::Visible {
            rule,
            zero_page,
            names,
            // The `:= expr` value is read where constants are collected, so by
            // here the symbol is defined like any other and only the claim is
            // left to check.
            define: _,
        } => (Resolved::Visible(rule, zero_page, names), 0),
        Kind::Assert(cond, fatal, message) => (Resolved::Assert(cond, fatal, message), 0),
        Kind::Insn { operand, mnemonic } => {
            let insn = set
                .instruction(&mnemonic)
                .ok_or_else(|| AsmError::new(line, format!("unknown instruction `{mnemonic}`")))?;
            // ca65 sizes by value (and an explicit `a:` we don't yet need); the
            // ACME-style hex-width rule does not apply.
            let (mode, operand) = mos6502::resolve_mode(insn, operand, size_env, false, line)?;
            let form = insn
                .form(mode)
                .ok_or_else(|| AsmError::new(line, format!("`{mnemonic}` has no {mode} form")))?;
            let operands: Vec<Expr> = operand.into_iter().collect();
            let size = form.len();
            (Resolved::Insn { form, operands }, size)
        }
    })
}

/// Where a statement is being emitted: the finished symbol table, every
/// label's segment (so the `zp` spellings can tell a zero-page label from an
/// absolute one — and both from a constant, which is neither), and the source
/// position a refusal is stamped with.
pub(super) struct Emit<'a> {
    pub(super) env: &'a BTreeMap<String, i64>,
    pub(super) label_seg: &'a BTreeMap<String, String>,
    pub(super) file: FileId,
    pub(super) line: usize,
}

/// Emit one resolved item's bytes at address `addr`, appending any diagnostic
/// the source asked for. `env` is the finished symbol table, so an `.assert`
/// here may name a label defined below it.
pub(super) fn emit(
    item: Resolved,
    addr: u32,
    at: &Emit<'_>,
    out: &mut Vec<u8>,
    warnings: &mut Vec<Warning>,
) -> Result<(), AsmError> {
    let (env, label_seg, file, line_for_errors) = (at.env, at.label_seg, at.file, at.line);
    let pc = i64::from(addr);
    match item {
        Resolved::Nothing => {}
        Resolved::Message(severity, text) => match severity {
            DiagSeverity::Error => return Err(AsmError::new(line_for_errors, text)),
            DiagSeverity::Warning | DiagSeverity::Note => warnings.push(Warning {
                line: line_for_errors,
                message: text,
                kind: if severity == DiagSeverity::Note {
                    WarningKind::Note
                } else {
                    WarningKind::Advisory
                },
                file,
            }),
        },
        Resolved::Visible(rule, zero_page, names) => {
            for name in &names {
                let defined = env.contains_key(name);
                match rule {
                    VisRule::MustBeDefined if !defined => {
                        return Err(AsmError::new(
                            line_for_errors,
                            format!(
                                "exported symbol `{}` was never defined",
                                display_label(name)
                            ),
                        ));
                    }
                    VisRule::MustNotBeDefined if defined => {
                        return Err(AsmError::new(
                            line_for_errors,
                            format!("symbol `{}` is already an import", display_label(name)),
                        ));
                    }
                    _ => {}
                }
                // The zero-page warning is about *labels*: a constant is never
                // "absolute" in the sense ca65 means, whatever its value.
                if zero_page
                    && let Some(segment) = label_seg.get(name)
                    && segment != "ZEROPAGE"
                {
                    warnings.push(Warning {
                        line: line_for_errors,
                        message: format!(
                            "symbol `{}` is absolute but exported zeropage",
                            display_label(name)
                        ),
                        kind: WarningKind::Advisory,
                        file,
                    });
                }
            }
        }
        Resolved::Assert(cond, fatal, message) => {
            if cond.eval(env, pc, line_for_errors)? == 0 {
                if fatal {
                    return Err(AsmError::new(line_for_errors, message));
                }
                warnings.push(Warning {
                    line: line_for_errors,
                    message,
                    kind: WarningKind::Advisory,
                    file,
                });
            }
        }
        Resolved::Raw(bytes) => out.extend_from_slice(&bytes),
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
                let w = u16::try_from(to_width(v, 0xFFFF, line_for_errors)?).expect("checked");
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
        Resolved::DBytes(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u16::try_from(to_width(v, 0xFFFF, line_for_errors)?).expect("checked");
                out.extend_from_slice(&w.to_be_bytes());
            }
        }
        Resolved::DWords(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u32::try_from(to_width(v, 0xFFFF_FFFF, line_for_errors)?).expect("checked");
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
        Resolved::Insn { form, operands } => {
            let next = pc + form.len() as i64;
            out.extend_from_slice(form.opcode);
            for (slot, e) in form.operands.iter().zip(operands.iter()) {
                let v = e.eval(env, pc, line_for_errors)?;
                match slot.kind {
                    // `ImmediateBe` is Z80N-only; ca65 is 6502/NES, so it never
                    // reaches here, but the match must stay exhaustive.
                    isa::OperandKind::Immediate
                    | isa::OperandKind::ImmediateBe
                    | isa::OperandKind::Address => match slot.bytes {
                        1 => out.push(to_byte(v, line_for_errors)?),
                        2 => out.extend_from_slice(
                            &u16::try_from(v & 0xFFFF).expect("masked").to_le_bytes(),
                        ),
                        other => {
                            return Err(AsmError::new(
                                line_for_errors,
                                format!("unsupported operand width {other}"),
                            ));
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
                        return Err(AsmError::new(
                            line_for_errors,
                            "displacement operand not valid on 6502",
                        ));
                    }
                }
            }
            out.extend_from_slice(form.suffix);
        }
    }
    Ok(())
}

/// ca65 takes **no negative literal**, at any width: `.byte -1` is `Range
/// error (-1 not in [0..255])`, and `.word -1` and `.dword -1` answer the
/// same way against their own bounds (probed 2026-08-25). It is the only
/// reference here that refuses — the other six read `-1` as its two's
/// complement — so this bound is ca65's, not a house preference.
fn to_byte(v: i64, line: usize) -> Result<u8, AsmError> {
    to_width(v, 0xFF, line).map(|v| v as u8)
}

/// One width's range check, so `.byte`, `.word` and `.dword` cannot drift
/// apart. Before this the word and dword paths simply masked, which accepted
/// `.word -1` and `.word 65536` in silence where ca65 refuses both
/// (asm198x#290).
fn to_width(v: i64, max: i64, line: usize) -> Result<i64, AsmError> {
    if (0..=max).contains(&v) {
        Ok(v)
    } else {
        Err(AsmError::new(
            line,
            format!("range error ({v} not in [0..{max}])"),
        ))
    }
}
