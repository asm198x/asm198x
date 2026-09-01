//! Section placement (#486's artifact/layout split): where each `SECTION`
//! lands in the Game Boy memory map — the bank geometry, the first and last
//! address of every RGBDS region, and RGBLINK's largest-first, first-fit
//! layout for floating ROM0 and non-ROM sections. Moved verbatim from the
//! parent so the layout can be read on its own; the seam is the boundary,
//! not a rewrite.

use std::collections::BTreeMap;

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

#[derive(Clone)]
pub(super) struct NonromMeta {
    kind: String,
    pinned: bool,
    declaration: usize,
}

pub(super) fn nonrom_section_meta(nodes: &[Node]) -> BTreeMap<String, NonromMeta> {
    nodes
        .iter()
        .enumerate()
        .filter_map(|(declaration, node)| {
            let code = node.source.trim();
            code.to_ascii_uppercase()
                .starts_with("SECTION")
                .then(|| {
                    let kind = section_kind(code);
                    (!matches!(kind.as_str(), "ROM0" | "ROMX")).then(|| {
                        (
                            section_name(code),
                            NonromMeta {
                                kind,
                                pinned: section_origin(code, node.span.line as usize)
                                    .ok()
                                    .flatten()
                                    .is_some(),
                                declaration,
                            },
                        )
                    })
                })
                .flatten()
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
    meta: &BTreeMap<String, NonromMeta>,
) -> Result<(), AsmError> {
    #[derive(Clone, Copy)]
    struct Section {
        statement: usize,
        end: usize,
        base: i64,
        declaration: usize,
        pinned: bool,
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
            });
        }
        let mut occupied = Vec::new();
        for section in sections.iter().copied().filter(|s| s.pinned) {
            occupied.push((section.base, extent(ops, section, section.base)?));
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
                if let Some((_, used_end)) = occupied
                    .iter()
                    .find(|(used_start, used_end)| base < *used_end && end > *used_start)
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
                occupied.push((base, end));
                occupied.sort_unstable();
                break;
            }
        }
    }
    Ok(())
}

/// Give floating ROM0 sections RGBLINK's largest-first, first-fit layout.
/// Pinned sections reserve their ranges first; floating sections are ordered
/// by decreasing size (declaration order breaks ties), then each takes the
/// lowest gap that can contain it.
pub(super) fn place_floating_rom0_sections(ops: &mut [Statement]) -> Result<(), AsmError> {
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
    floating.sort_by_key(|(section, end)| (std::cmp::Reverse(*end), section.statement));

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
            if end > BANK_SIZE {
                return Err(ops[section.statement].err("ROM0 sections exceed the 16 KiB bank"));
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
