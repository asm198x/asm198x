//! The AmigaDOS hunk executable (#486's legibility split on the artifact
//! seam): the container vocabulary a `section` resolves to — hunk kind, memory
//! flag, the assembled section with its 32-bit relocations — and the
//! serialiser that writes them as `-Fhunkexe -kick1hunks` does. It is its own
//! file so a change to the container is found without opening the assembler
//! pass or the 68000 encoder. Moved verbatim from the parent; the seam is the
//! boundary, not a rewrite.

/// A 32-bit relocation: a byte offset within a section, and the target section
/// whose load address gets added to the longword stored there.
pub(super) type Reloc = (u32, usize);

/// One assembled section: its hunk kind and memory flag, its bytes, and the
/// 32-bit relocations within it.
pub(super) struct SecOut {
    pub(super) kind: HunkKind,
    pub(super) flag: MemFlag,
    pub(super) bytes: Vec<u8>,
    pub(super) relocs: Vec<Reloc>,
}

/// Serialize assembled sections into an AmigaDOS hunk executable, matching
/// `vasmm68k_mot -Fhunkexe -kick1hunks` for everything the loader consumes
/// (header, code/data/bss hunks, reloc32 tables). The optional HUNK_SYMBOL
/// table vasm also writes is debug-only and omitted — see the Stage 3 decision.
pub(super) fn serialize_hunkexe(sections: &[SecOut]) -> Vec<u8> {
    fn push_u32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_be_bytes());
    }
    // Each hunk's size in longwords: code/data padded to a longword, bss rounded.
    let size_longs = |s: &SecOut| -> u32 { s.bytes.len().div_ceil(4) as u32 };

    let mut out = Vec::new();
    // HUNK_HEADER: no resident libraries, then hunk count, first, last, sizes.
    push_u32(&mut out, 0x3f3);
    push_u32(&mut out, 0);
    push_u32(&mut out, sections.len() as u32);
    push_u32(&mut out, 0);
    push_u32(&mut out, sections.len() as u32 - 1);
    for s in sections {
        push_u32(&mut out, size_longs(s) | s.flag.bits());
    }

    for s in sections {
        match s.kind {
            HunkKind::Bss => {
                push_u32(&mut out, 0x3eb);
                push_u32(&mut out, size_longs(s));
            }
            HunkKind::Code | HunkKind::Data => {
                push_u32(
                    &mut out,
                    if matches!(s.kind, HunkKind::Code) {
                        0x3e9
                    } else {
                        0x3ea
                    },
                );
                push_u32(&mut out, size_longs(s));
                let mut data = s.bytes.clone();
                // Pad to a longword. A code hunk two bytes short takes a NOP
                // (0x4e71) — a whole instruction word, which is the only thing
                // a NOP can be; one or three bytes short there is no room for
                // one, and vasm pads with zeros instead. The decision is made
                // once from the length as it stands: padding a 17-byte code
                // hunk to 18 and *then* calling it two short produces
                // `00 4e 71` where vasm writes three zeros.
                let short = (4 - data.len() % 4) % 4;
                if matches!(s.kind, HunkKind::Code) && short == 2 {
                    data.extend_from_slice(&[0x4e, 0x71]);
                } else {
                    data.extend(std::iter::repeat_n(0u8, short));
                }
                out.extend_from_slice(&data);
            }
        }

        // HUNK_RELOC32: blocks of [count, target hunk, offsets…], target hunks
        // ascending, offsets in the order the assembler recorded them,
        // terminated by a zero count.
        //
        // Not sorted. vasm emits each block in discovery order, which is
        // usually ascending and is not guaranteed to be: an expression resolved
        // after the position it patches puts a lower offset later in the list.
        // Sorting matched vasm everywhere that happened not to occur and
        // diverged the moment it did — flock units 15-18 carry the same 142
        // entries as vasm in a different order. A loader applies them all
        // whatever the order, so this is invisible at runtime and fatal to
        // byte-identity, which is the claim being made.
        if !s.relocs.is_empty() {
            push_u32(&mut out, 0x3ec);
            for target in 0..sections.len() {
                let offs: Vec<u32> = s
                    .relocs
                    .iter()
                    .filter(|(_, t)| *t == target)
                    .map(|(o, _)| *o)
                    .collect();
                if offs.is_empty() {
                    continue;
                }
                push_u32(&mut out, offs.len() as u32);
                push_u32(&mut out, target as u32);
                for o in offs {
                    push_u32(&mut out, o);
                }
            }
            push_u32(&mut out, 0);
        }

        push_u32(&mut out, 0x3f2); // HUNK_END
    }
    out
}

/// A hunk's content kind, from a `section` directive's attribute.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum HunkKind {
    Code,
    Data,
    Bss,
}

/// A hunk's memory-placement preference, from the `_c`/`_f` attribute suffix.
#[derive(Clone, Copy)]
pub(super) enum MemFlag {
    Any,
    Chip,
    Fast,
}

impl MemFlag {
    /// The two-bit memory flag OR-ed into a hunk's size longword in the header.
    fn bits(self) -> u32 {
        match self {
            MemFlag::Any => 0,
            MemFlag::Chip => 0x4000_0000,
            MemFlag::Fast => 0x8000_0000,
        }
    }
}
