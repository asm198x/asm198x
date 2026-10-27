//! The ld65 job (#486's artifact/layout split): the memory account, the
//! link that lays the segments into the layout's image, and the resolve and
//! emit steps that turn a placed statement into bytes. It is its own file so
//! that a change to how bytes land in the ROM is read apart from the parse
//! and the projection sweep. Moved verbatim from the parent; the seam is the
//! boundary, not a rewrite.

use std::collections::BTreeMap;

use super::super::mos6502;
use super::{
    AddressSize, AsmError, DiagSeverity, Expr, FileId, Kind, Warning, WarningKind, display_label,
};

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

    // The image is the layout's shape: its file areas stacked and filled.
    let size = layout.image_size;
    let (_, mut rom) =
        crate::engine::lay_out(runs, layout.fill, 1, Some(0), false, |_| Some(size))?;
    // The shared engine has no image when every run is empty. A configured
    // ld65 file can still consist entirely of filled memory areas.
    if layout.from_config {
        rom.resize(size, layout.fill);
    }
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
    FarAddrs(Vec<Expr>),
    DBytes(Vec<Expr>),
    DWords(Vec<Expr>),
    Fill(usize, u8),
    Insn {
        form: &'static isa::Form,
        operands: Vec<Expr>,
        /// Explicit absolute operands and 4510 word pushes need unsigned words.
        unsigned_word: bool,
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

pub(super) fn alignment_padding(boundary: i64, off: u32) -> usize {
    let pad = (boundary - i64::from(off).rem_euclid(boundary)) % boundary;
    usize::try_from(pad).expect("non-negative")
}

/// Resolve a parsed statement to an emittable item plus its byte size.
pub(super) fn resolve(
    set: &'static isa::InstructionSet,
    kind: Kind,
    size_env: &BTreeMap<String, i64>,
    off: u32,
    line: usize,
    default_fill: u8,
    bound: Option<super::BoundInsn>,
) -> Result<(Resolved, usize), AsmError> {
    if let Some(bound) = bound {
        let size = bound.form.len();
        return Ok((
            Resolved::Insn {
                form: bound.form,
                operands: bound.operands,
                unsigned_word: bound.unsigned_word,
            },
            size,
        ));
    }
    Ok(match kind {
        Kind::Empty => (Resolved::Nothing, 0),
        // The sweep reads every unread line before projection ends; one
        // reaching layout is a driver bug, not a source error.
        Kind::Unread => unreachable!("an unread line survived the projection sweep"),
        Kind::EnumValue(..) => unreachable!("an enum expression survived constant projection"),
        // The only statement whose size depends on where it stands: pad to the
        // next multiple of the boundary, measured from the segment's start.
        Kind::Align(boundary, fill) => {
            let pad = alignment_padding(boundary, off);
            (Resolved::Fill(pad, fill.unwrap_or(default_fill)), pad)
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
        Kind::FarAddrs(v) => {
            let n = v.len() * 3;
            (Resolved::FarAddrs(v), n)
        }
        Kind::DBytes(v) => {
            let n = v.len() * 2;
            (Resolved::DBytes(v), n)
        }
        Kind::DWords(v) => {
            let n = v.len() * 4;
            (Resolved::DWords(v), n)
        }
        Kind::Res(count, fill) => (Resolved::Fill(count, fill.unwrap_or(default_fill)), count),
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
        Kind::BitBranch {
            form,
            zero_page,
            target,
        } => (
            Resolved::Insn {
                form,
                operands: vec![zero_page, target],
                unsigned_word: false,
            },
            form.len(),
        ),
        Kind::Insn {
            operand,
            address_size,
            mnemonic,
            extension,
        } => {
            let (mode, mut operand) = if let Some(size) = address_size {
                use mos6502::{Index, OperandSyntax};
                let (index, value) = match operand {
                    OperandSyntax::Direct(e) => (None, e),
                    OperandSyntax::Indexed(e, index) => (Some(index), e),
                    _ => return Err(AsmError::new(line, "invalid sized memory operand")),
                };
                let mode = match (size, index) {
                    (AddressSize::ZeroPage, None) => "zeropage",
                    (AddressSize::ZeroPage, Some(Index::X)) => "zeropage,x",
                    (AddressSize::ZeroPage, Some(Index::Y)) => "zeropage,y",
                    (AddressSize::Absolute, None) => "absolute",
                    (AddressSize::Absolute, Some(Index::X)) => "absolute,x",
                    (AddressSize::Absolute, Some(Index::Y)) => "absolute,y",
                };
                (mode, Some(value))
            } else if let Some(extension) = extension {
                mos6502::resolve_mode_in_sets(
                    set,
                    Some(extension),
                    &mnemonic,
                    operand,
                    size_env,
                    false,
                    line,
                )?
            } else {
                let insn = set.instruction(&mnemonic).ok_or_else(|| {
                    AsmError::new(line, format!("unknown instruction `{mnemonic}`"))
                })?;
                mos6502::resolve_mode(insn, operand, size_env, false, line)?
            };
            let form = extension
                .and_then(|set| set.find_form(&mnemonic, mode))
                .or_else(|| set.find_form(&mnemonic, mode))
                .ok_or_else(|| AsmError::new(line, format!("`{mnemonic}` has no {mode} form")))?;
            // 65CE02/4510 long branches measure from opcode+2, rather
            // than from the three-byte instruction's end (as in ACME).
            if mode == "relative16" && extension.is_some() {
                operand = operand.map(|value| {
                    Expr::Bin(
                        crate::engine::BinOp::Add,
                        Box::new(value),
                        Box::new(Expr::Num(1)),
                    )
                });
            }
            let operands: Vec<Expr> = operand.into_iter().collect();
            let size = form.len();
            (
                Resolved::Insn {
                    form,
                    operands,
                    unsigned_word: address_size == Some(AddressSize::Absolute)
                        || matches!(
                            mode,
                            "absolute"
                                | "absolute,x"
                                | "absolute,y"
                                | "indirect"
                                | "(absolute)"
                                | "(absolute,x)"
                                | "(indirect)"
                        )
                        || (extension.is_some() && mnemonic == "PHW"),
                },
                size,
            )
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
    let mut pc = i64::from(addr);
    match item {
        Resolved::Nothing => {}
        Resolved::Message(severity, text) => match severity {
            DiagSeverity::Error => return Err(AsmError::new(line_for_errors, text)),
            DiagSeverity::Warning | DiagSeverity::Note => {
                warnings.push(Warning::new(line_for_errors, text).in_file(file).with_kind(
                    if severity == DiagSeverity::Note {
                        WarningKind::Note
                    } else {
                        WarningKind::Advisory
                    },
                ))
            }
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
                    warnings.push(
                        Warning::new(
                            line_for_errors,
                            format!(
                                "symbol `{}` is absolute but exported zeropage",
                                display_label(name)
                            ),
                        )
                        .in_file(file),
                    );
                }
            }
        }
        Resolved::Assert(cond, fatal, message) => {
            if cond.eval(env, pc, line_for_errors)? == 0 {
                if fatal {
                    return Err(AsmError::new(line_for_errors, message));
                }
                warnings.push(Warning::new(line_for_errors, message).in_file(file));
            }
        }
        Resolved::Raw(bytes) => out.extend_from_slice(&bytes),
        Resolved::Fill(count, fill) => out.extend(std::iter::repeat_n(fill, count)),
        Resolved::Bytes(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                out.push(to_byte(v, line_for_errors)?);
                pc += 1;
            }
        }
        Resolved::Words(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u16::try_from(to_width(v, 0xFFFF, line_for_errors)?).expect("checked");
                out.extend_from_slice(&w.to_le_bytes());
                pc += 2;
            }
        }
        Resolved::FarAddrs(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let address = to_width(v, 0xFF_FFFF, line_for_errors)?;
                out.extend_from_slice(&address.to_le_bytes()[..3]);
                pc += 3;
            }
        }
        Resolved::DBytes(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u16::try_from(to_width(v, 0xFFFF, line_for_errors)?).expect("checked");
                out.extend_from_slice(&w.to_be_bytes());
                pc += 2;
            }
        }
        Resolved::DWords(exprs) => {
            for e in &exprs {
                let v = e.eval(env, pc, line_for_errors)?;
                let w = u32::try_from(to_width(v, 0xFFFF_FFFF, line_for_errors)?).expect("checked");
                out.extend_from_slice(&w.to_le_bytes());
                pc += 4;
            }
        }
        Resolved::Insn {
            form,
            operands,
            unsigned_word,
        } => {
            let next = pc + form.len() as i64;
            out.extend_from_slice(form.opcode);
            let mut operand_pc = pc + form.opcode.len() as i64;
            for (slot, e) in form.operands.iter().zip(operands.iter()) {
                // ca65 reads each bit-branch operand after emitting the bytes
                // before it: `*` is opcode+1 for zp and opcode+2 for target.
                // Labels still use the ordinary end-of-instruction baseline.
                let eval_pc = if form.mode == "zeropage,relative" {
                    operand_pc
                } else {
                    pc
                };
                let v = e.eval(env, eval_pc, line_for_errors)?;
                operand_pc += i64::from(slot.bytes);
                match slot.kind {
                    // `ImmediateBe` is Z80N-only; ca65 is 6502/NES, so it never
                    // reaches here, but the match must stay exhaustive.
                    isa::OperandKind::Immediate
                    | isa::OperandKind::ImmediateBe
                    | isa::OperandKind::Address => match slot.bytes {
                        1 => out.push(to_byte(v, line_for_errors)?),
                        2 => {
                            // Check after all symbols and pseudo-PCs resolve:
                            // masking a word push silently changes its value.
                            let word = if unsigned_word {
                                to_width(v, 0xFFFF, line_for_errors)?
                            } else {
                                v & 0xFFFF
                            };
                            out.extend_from_slice(
                                &u16::try_from(word)
                                    .expect("checked or masked")
                                    .to_le_bytes(),
                            );
                        }
                        other => {
                            return Err(AsmError::new(
                                line_for_errors,
                                format!("unsupported operand width {other}"),
                            ));
                        }
                    },
                    isa::OperandKind::RelativePc => {
                        let offset = v - next;
                        match slot.bytes {
                            1 if (-128..=127).contains(&offset) => out.push(offset as i8 as u8),
                            2 if (-32768..=32767).contains(&offset) => {
                                out.extend_from_slice(&(offset as i16).to_le_bytes())
                            }
                            _ => {
                                return Err(AsmError::new(
                                    line_for_errors,
                                    format!("branch target out of range ({offset} bytes)"),
                                )
                                .with_code(crate::contract::Code::BranchOutOfRange));
                            }
                        }
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

/// One width's range check, so `.byte`, `.word`, `.faraddr` and `.dword` cannot drift
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
