//! Section placement (#486's artifact/layout split): where each `SECTION`
//! lands in the Game Boy memory map — the bank geometry, the first and last
//! address of every RGBDS region, and RGBLINK's largest-first, first-fit
//! layout for floating ROM0 and non-ROM sections. Moved verbatim from the
//! parent so the layout can be read on its own; the seam is the boundary,
//! not a rewrite.

use std::collections::BTreeMap;
use std::ops::Range;

use super::{AsmError, Node, Operation, Statement, section_name, section_origin};

/// A Game Boy ROM bank, and where the CPU sees a banked one.
pub(super) const BANK_SIZE: i64 = 0x4000;
pub(super) const ROMX_BASE: i64 = 0x4000;

/// The section type following the quoted name.
pub(super) fn section_kind(code: &str) -> String {
    code.split_once('"')
        .and_then(|(_, tail)| tail.split_once('"'))
        .and_then(|(_, tail)| tail.split_once(','))
        .map(|(_, kind)| kind.trim().split(['[', ',']).next().unwrap_or(""))
        .unwrap_or("")
        .to_ascii_uppercase()
}

/// First address in each non-ROM RGBDS region. These sections are BSS in a
/// linked ROM, but their labels still need the addresses the CPU observes.
pub(super) fn section_default_base(kind: &str) -> i64 {
    match kind {
        "VRAM" => 0x8000,
        "SRAM" => 0xA000,
        "WRAM0" => 0xC000,
        "WRAMX" => 0xD000,
        "OAM" => 0xFE00,
        "HRAM" => 0xFF80,
        _ => 0,
    }
}

pub(super) fn section_region_start(kind: &str) -> Option<i64> {
    let upper = kind.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "ROM0" | "ROMX" | "VRAM" | "SRAM" | "WRAM0" | "WRAMX" | "OAM" | "HRAM"
    )
    .then(|| {
        if upper == "ROMX" {
            ROMX_BASE
        } else {
            section_default_base(&upper)
        }
    })
}

/// What the source said about a section that its lowered statement no longer
/// shows: its type, whether it pinned an address, and where it was declared.
/// A floating ROMX section is lowered at `$4000` like a pinned one.
#[derive(Clone)]
pub(super) struct SectionMeta {
    kind: String,
    pinned: bool,
    declaration: usize,
}

pub(super) fn section_meta(nodes: &[Node]) -> BTreeMap<String, SectionMeta> {
    nodes
        .iter()
        .enumerate()
        .filter_map(|(declaration, node)| {
            let code = node.source.trim();
            code.to_ascii_uppercase().starts_with("SECTION").then(|| {
                (
                    section_name(code),
                    SectionMeta {
                        kind: section_kind(code),
                        pinned: section_origin(code, node.span.line as usize)
                            .ok()
                            .flatten()
                            .is_some(),
                        declaration,
                    },
                )
            })
        })
        .collect()
}

fn section_region_end(kind: &str) -> Option<i64> {
    Some(match kind {
        "VRAM" => 0xA000,
        "SRAM" => 0xC000,
        "WRAM0" => 0xD000,
        "WRAMX" => 0xE000,
        "OAM" => 0xFEA0,
        "HRAM" => 0xFFFF,
        _ => return None,
    })
}

/// Place floating BSS sections with RGBLINK's largest-first, reverse-source
/// tie break, then first-fit around pinned ranges. They remain `Discard`
/// sections, so only their resolved addresses enter the cartridge image.
pub(super) fn place_floating_nonrom_sections(
    ops: &mut [Statement],
    meta: &BTreeMap<String, SectionMeta>,
) -> Result<(), AsmError> {
    #[derive(Clone, Copy)]
    struct Section {
        statement: usize,
        end: usize,
        base: i64,
        declaration: usize,
        pinned: bool,
        bank: Option<u32>,
    }

    fn extent(ops: &[Statement], section: Section, base: i64) -> Result<i64, AsmError> {
        let mut pc = base;
        for st in &ops[section.statement + 1..section.end] {
            if let Some(op) = &st.op {
                pc = crate::engine::next_pc(op, pc, &isa::sm83::SET, None, 1, st.line)
                    .map_err(|err| st.stamp(err))?;
            }
        }
        Ok(pc)
    }

    let starts: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter_map(|(i, st)| matches!(st.op, Some(Operation::Section { .. })).then_some(i))
        .collect();
    for kind in ["VRAM", "SRAM", "WRAM0", "WRAMX", "OAM", "HRAM"] {
        let region_start = section_default_base(kind);
        let region_end = section_region_end(kind).expect("known region");
        let mut sections = Vec::new();
        for (n, &statement) in starts.iter().enumerate() {
            let end = starts.get(n + 1).copied().unwrap_or(ops.len());
            let Some(Operation::Section {
                name,
                base: Some(base),
                at: crate::engine::Place::Discard,
                bank,
            }) = &ops[statement].op
            else {
                continue;
            };
            let Some(info) = meta.get(name).filter(|m| m.kind == kind) else {
                continue;
            };
            sections.push(Section {
                statement,
                end,
                base: *base,
                declaration: info.declaration,
                pinned: info.pinned,
                bank: *bank,
            });
        }
        let mut occupied = Vec::new();
        for section in sections.iter().copied().filter(|s| s.pinned) {
            occupied.push((
                section.bank,
                section.base,
                extent(ops, section, section.base)?,
            ));
        }
        occupied.sort_unstable();
        let mut floating = sections
            .iter()
            .copied()
            .filter(|s| !s.pinned)
            .map(|s| Ok((s, extent(ops, s, region_start)? - region_start)))
            .collect::<Result<Vec<_>, AsmError>>()?;
        floating
            .sort_by_key(|(s, size)| (std::cmp::Reverse(*size), std::cmp::Reverse(s.declaration)));
        for (section, size) in floating {
            let mut base = region_start;
            loop {
                let end = base + size;
                if let Some((_, _, used_end)) =
                    occupied.iter().find(|(bank, used_start, used_end)| {
                        *bank == section.bank && base < *used_end && end > *used_start
                    })
                {
                    base = *used_end;
                    continue;
                }
                if end > region_end {
                    return Err(
                        ops[section.statement].err(format!("{kind} sections exceed their region"))
                    );
                }
                if let Some(Operation::Section { base: slot, .. }) = &mut ops[section.statement].op
                {
                    *slot = Some(base);
                }
                occupied.push((section.bank, base, end));
                occupied.sort_unstable();
                break;
            }
        }
    }
    Ok(())
}

/// The end of ROM0: plain RGBLINK keeps it to the first bank, and
/// `rgblink -t` gives it the whole 32 KiB.
pub(super) fn rom0_end(tiny: bool) -> i64 {
    if tiny { 2 * BANK_SIZE } else { BANK_SIZE }
}

/// `rgblink -t` has no banks: each ROMX section becomes a ROM0 section in
/// bank 0, placed by address, and floating unless it pinned one. A floating
/// one then joins ROM0's own layout — RGBDS 1.0.4 links `SECTION "b", ROMX`
/// after a three-byte ROM0 section at `$0003`, and `BANK("b")` is 0. RGBLINK
/// converts only a section in bank 1, the one it gets when it names none; one
/// in another bank stays ROMX for [`check_rom_sections`] to refuse.
pub(super) fn join_romx_to_rom0(ops: &mut [Statement], meta: &BTreeMap<String, SectionMeta>) {
    for st in ops.iter_mut() {
        if let Some(Operation::Section {
            name,
            base,
            at,
            bank,
        }) = &mut st.op
            && matches!(at, crate::engine::Place::At(_))
            && *bank == Some(1)
        {
            if !meta.get(name.as_str()).is_some_and(|m| m.pinned) {
                *base = None;
            }
            *at = crate::engine::Place::ByAddress;
            *bank = Some(0);
        }
    }
}

/// Where a section's body ends when the section starts at `base`.
fn body_end(ops: &[Statement], body: Range<usize>, base: i64) -> Result<i64, AsmError> {
    let mut pc = base;
    for st in &ops[body] {
        if let Some(op) = &st.op {
            pc = crate::engine::next_pc(op, pc, &isa::sm83::SET, None, 1, st.line)
                .map_err(|err| st.stamp(err))?;
        }
    }
    Ok(pc)
}

/// RGBLINK's checks on each ROM section before it places any: a section
/// bigger than its region, a pinned address outside the region, and a pinned
/// section that runs past the region's end. ROM0 is `$0000-$3FFF` and ROMX
/// `$4000-$7FFF`. Every failure is reported, in declaration order and in
/// RGBLINK's words, as `rgblink` 1.0.4 does: `ROM0[$150]` followed by
/// `ds $8000-@` gives both
///
/// ```text
/// error: Section "a" is bigger than the max size for that type: $7eb0 > $4000
/// error: Section "a"'s end address $8000 is greater than last address $4000
/// ```
///
/// A pinned address outside the region is the only complaint about it;
/// RGBLINK does not go on to measure where it ends.
///
/// Under `tiny` (`rgblink -t`), ROM0 runs to `$7FFF`, and a ROMX section
/// still standing after [`join_romx_to_rom0`] named a bank other than 1.
pub(super) fn check_rom_sections(
    ops: &[Statement],
    meta: &BTreeMap<String, SectionMeta>,
    tiny: bool,
) -> Result<(), AsmError> {
    let starts: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter_map(|(i, st)| matches!(st.op, Some(Operation::Section { .. })).then_some(i))
        .collect();
    let mut errors = Vec::new();
    for (n, &statement) in starts.iter().enumerate() {
        let body = statement + 1..starts.get(n + 1).copied().unwrap_or(ops.len());
        let Some(Operation::Section { name, base, at, .. }) = &ops[statement].op else {
            continue;
        };
        let (first, end, pinned) = match at {
            crate::engine::Place::ByAddress => (0, rom0_end(tiny), *base),
            crate::engine::Place::At(_) if tiny => {
                errors.push(ops[statement].err(format!(
                    "Section \"{name}\" has type `ROMX`, which must be in bank 1 (if any) \
                     with option '-t'"
                )));
                continue;
            }
            crate::engine::Place::At(_) => (
                ROMX_BASE,
                ROMX_BASE + BANK_SIZE,
                base.filter(|_| meta.get(name).is_some_and(|m| m.pinned)),
            ),
            _ => continue,
        };
        let st = &ops[statement];
        let start = pinned.unwrap_or(first);
        let size = body_end(ops, body, start)? - start;
        if size > end - first {
            errors.push(st.err(format!(
                "Section \"{name}\" is bigger than the max size for that type: ${size:04x} > ${:04x}",
                end - first
            )));
        }
        if let Some(base) = pinned {
            if base < first || base >= end {
                errors.push(st.err(format!(
                    "Section \"{name}\"'s fixed address ${base:04x} is outside of range \
                     [${first:04x}; ${:04x}]",
                    end - 1
                )));
            } else if base + size > end {
                errors.push(st.err(format!(
                    "Section \"{name}\"'s end address ${:04x} is greater than last address \
                     ${end:04x}",
                    base + size
                )));
            }
        }
    }
    AsmError::gather(errors).map_or(Ok(()), Err)
}

/// Give floating ROM0 sections RGBLINK's largest-first, first-fit layout.
/// Pinned sections reserve their ranges first; floating sections are ordered
/// by decreasing size, then each takes the lowest gap that can contain it.
/// Equal sizes go in reverse declaration order, as RGBLINK inserts each
/// section ahead of the ones no bigger than it — the tie break the RAM
/// regions above already use.
pub(super) fn place_floating_rom0_sections(
    ops: &mut [Statement],
    region_end: i64,
) -> Result<(), AsmError> {
    #[derive(Clone, Copy)]
    struct Section {
        statement: usize,
        end: usize,
        base: Option<i64>,
        at: crate::engine::Place,
    }

    let starts: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter_map(|(i, st)| matches!(st.op, Some(Operation::Section { .. })).then_some(i))
        .collect();
    let sections: Vec<Section> = starts
        .iter()
        .enumerate()
        .map(|(n, &statement)| {
            let end = starts.get(n + 1).copied().unwrap_or(ops.len());
            let (base, at) = match &ops[statement].op {
                Some(Operation::Section { base, at, .. }) => (*base, *at),
                _ => unreachable!("collected only section statements"),
            };
            Section {
                statement,
                end,
                base,
                at,
            }
        })
        .collect();

    fn extent(ops: &[Statement], section: Section, base: i64) -> Result<i64, AsmError> {
        let mut pc = base;
        for st in &ops[section.statement + 1..section.end] {
            if let Some(op) = &st.op {
                pc = crate::engine::next_pc(op, pc, &isa::sm83::SET, None, 1, st.line)
                    .map_err(|err| st.stamp(err))?;
            }
        }
        Ok(pc)
    }

    let mut occupied: Vec<(i64, i64)> = Vec::new();
    for section in sections
        .iter()
        .copied()
        .filter(|s| s.at == crate::engine::Place::ByAddress && s.base.is_some())
    {
        let base = section.base.expect("filtered to pinned sections");
        occupied.push((base, extent(ops, section, base)?));
    }
    occupied.sort_unstable();

    let mut floating: Vec<(Section, i64)> = sections
        .iter()
        .copied()
        .filter(|s| s.at == crate::engine::Place::ByAddress && s.base.is_none())
        .map(|section| Ok((section, extent(ops, section, 0)?)))
        .collect::<Result<_, AsmError>>()?;
    floating.sort_by_key(|(section, end)| {
        (
            std::cmp::Reverse(*end),
            std::cmp::Reverse(section.statement),
        )
    });

    for (section, _) in floating {
        let mut base = 0;
        loop {
            let end = extent(ops, section, base)?;
            if let Some((_, used_end)) = occupied
                .iter()
                .find(|(used_start, used_end)| base < *used_end && end > *used_start)
            {
                base = *used_end;
                continue;
            }
            if end > region_end {
                // Each section fits on its own; [`check_rom_sections`] saw to
                // that. Together they do not, and RGBLINK names the one left.
                let name = match &ops[section.statement].op {
                    Some(Operation::Section { name, .. }) => name.as_str(),
                    _ => unreachable!("collected only section statements"),
                };
                return Err(ops[section.statement].err(format!(
                    "Unable to place \"{name}\" (ROM0 section) anywhere"
                )));
            }
            if let Some(Operation::Section { base: slot, .. }) = &mut ops[section.statement].op {
                *slot = Some(base);
            }
            occupied.push((base, end));
            occupied.sort_unstable();
            break;
        }
    }
    Ok(())
}
