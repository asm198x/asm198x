//! Resolve final instruction bytes against shared ISA timing metadata.
//! The assembler supplies operand bytes, never its own hardware timing table.
use crate::engine::CycleBounds;
use isa::mos6809::{self, Kind};

fn fixed(cycles: u8) -> CycleBounds {
    CycleBounds {
        min: cycles,
        max: Some(cycles),
    }
}

pub(super) fn resolve(bytes: &[u8]) -> Option<CycleBounds> {
    for insn in mos6809::SET.iter().filter(|i| !i.undocumented) {
        match &insn.kind {
            Kind::Inherent(op) if bytes == *op => {
                return insn
                    .fixed_inherent_effects()
                    .map(|e| fixed(e.cycles))
                    .or_else(|| {
                        insn.interrupt_effects(None).map(|e| {
                            let (min, max) = e.timing.bounds();
                            CycleBounds { min, max }
                        })
                    });
            }
            Kind::Branch { short, long } => {
                for (op, mode, width) in [(*short, "relative", 1), (*long, "relative long", 2)] {
                    if !op.is_empty() && bytes.starts_with(op) && bytes.len() == op.len() + width {
                        return insn.branch_effects(mode).map(|e| CycleBounds {
                            min: e.cycles.base,
                            max: Some(e.cycles.base + e.cycles.branch_taken),
                        });
                    }
                }
            }
            Kind::Mem {
                imm,
                direct,
                indexed,
                extended,
                width,
            } => {
                for (op, mode, size) in [
                    (*imm, "immediate", usize::from(*width)),
                    (*direct, "direct", 1),
                    (*indexed, "indexed", 1),
                    (*extended, "extended", 2),
                ] {
                    if op.is_empty() || !bytes.starts_with(op) {
                        continue;
                    }
                    let operand = bytes.get(op.len()).copied()?;
                    let postbyte = (mode == "indexed").then_some(operand);
                    let tail = if let Some(postbyte) = postbyte {
                        1 + usize::from(mos6809::timing::indexed_cost(postbyte)?.extension_bytes)
                    } else {
                        size
                    };
                    if bytes.len() != op.len() + tail {
                        return None;
                    }
                    return insn
                        .memory_effects(mode, postbyte)
                        .map(|e| fixed(e.cycles))
                        .or_else(|| {
                            if mode == "immediate" {
                                insn.immediate_cc_effects(operand)
                                    .map(|e| fixed(e.cycles))
                                    .or_else(|| {
                                        insn.interrupt_effects(Some(operand)).map(|e| {
                                            let (min, max) = e.timing.bounds();
                                            CycleBounds { min, max }
                                        })
                                    })
                            } else {
                                None
                            }
                        });
                }
            }
            Kind::Transfer(op) if bytes.first() == Some(op) && bytes.len() == 2 => {
                return insn.transfer_effects(bytes[1]).map(|e| fixed(e.cycles));
            }
            Kind::Stack { opcode, .. } if bytes.first() == Some(opcode) && bytes.len() == 2 => {
                return insn.stack_effects(bytes[1]).map(|e| fixed(e.cycles));
            }
            _ => {}
        }
    }
    None
}
