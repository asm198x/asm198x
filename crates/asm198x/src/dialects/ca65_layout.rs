//! The ca65 placement layout as a value (#483,
//! `decisions/layouts-are-data.md`): named memory areas, the segments
//! assigned into them, and the resolution that turns both into the two
//! numbers placement runs on — a CPU base address and a file offset.
//!
//! The curriculum's fixed NROM table was compiled into the dialect; this
//! module is that table becoming data, so a project may eventually supply its
//! own (`-C`, the bounded ld65 `.cfg` reader). Resolution is ld65's: areas
//! contribute file bytes in declaration order, segments place into their area
//! in declaration order from the area's start, an `align` rounds the cursor
//! up, and a `start` pins it — never backwards.

use crate::engine::AsmError;

/// One named memory area: a span of address space, optionally contributing
/// its full extent to the output file (ld65 `file = %O, fill = yes`).
pub(crate) struct Area {
    pub(crate) name: String,
    pub(crate) start: u32,
    pub(crate) size: u32,
    /// Whether the area's bytes land in the output file. A `file = ""` area
    /// (RAM, zero page) occupies address space only.
    pub(crate) in_file: bool,
}

/// One segment definition: which area holds it, and the placement attributes
/// ld65 recognises on the shapes this reader is bounded to.
pub(crate) struct SegmentDef {
    pub(crate) name: String,
    /// Index into [`Layout::areas`].
    pub(crate) area: usize,
    /// ld65 `start =` — pins the segment's base address inside its area.
    pub(crate) start: Option<u32>,
    /// ld65 `align =` — rounds the running cursor up before placing.
    pub(crate) align: Option<u32>,
}

/// A layout: the two declaration-ordered lists a `.cfg` states.
pub(crate) struct Layout {
    pub(crate) areas: Vec<Area>,
    pub(crate) segments: Vec<SegmentDef>,
    /// The byte every filled gap carries (ld65 `fillval`). Bounded to one
    /// value across the file areas; the shapes in scope all use `$00`.
    pub(crate) fill: u8,
}

/// A segment after resolution: the numbers placement runs on.
#[derive(Debug)]
pub(crate) struct ResolvedSeg {
    pub(crate) name: String,
    /// CPU base address.
    pub(crate) base: u32,
    /// Offset of the segment's first byte in the output file; `None` for a
    /// segment in a RAM-only area.
    pub(crate) file_at: Option<usize>,
    /// Whether `base` means anything to the CPU. A file area whose address
    /// range overlaps a RAM area is paged somewhere the CPU is not looking —
    /// the iNES header, CHR in PPU space — so its segments carry file
    /// positions but no CPU-meaningful base. Derived from the ranges, not
    /// from anybody's segment names.
    pub(crate) cpu_addressable: bool,
}

/// The resolved layout: segments in declaration order (their index is the
/// debug section id, as the config order always has been), plus the image
/// shape the linker fills.
#[derive(Debug)]
pub(crate) struct ResolvedLayout {
    pub(crate) segs: Vec<ResolvedSeg>,
    pub(crate) image_size: usize,
    pub(crate) fill: u8,
}

impl ResolvedLayout {
    pub(crate) fn seg(&self, name: &str) -> Option<&ResolvedSeg> {
        self.segs.iter().find(|s| s.name == name)
    }

    pub(crate) fn seg_id(&self, name: &str) -> debug198x::SectionId {
        self.segs
            .iter()
            .position(|s| s.name == name)
            .expect("seg validated against the layout") as debug198x::SectionId
    }

    /// The valid segment names, for a rejection message.
    pub(crate) fn known(&self) -> String {
        self.segs
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl Layout {
    /// The curriculum's fixed NROM layout (the `dash` track's `nes.cfg`),
    /// expressed as the value it always encoded: iNES header (16) + PRG
    /// ($8000, 32K) + CHR (8K), `$00` fill, `VECTORS` pinned at $FFFA.
    pub(crate) fn nes_default() -> Layout {
        let area = |name: &str, start: u32, size: u32, in_file: bool| Area {
            name: name.to_string(),
            start,
            size,
            in_file,
        };
        let seg = |name: &str, area: usize, start: Option<u32>| SegmentDef {
            name: name.to_string(),
            area,
            start,
            align: None,
        };
        Layout {
            areas: vec![
                area("ZP", 0x0000, 0x0100, false),
                area("OAM", 0x0200, 0x0100, false),
                area("RAM", 0x0300, 0x0500, false),
                area("HDR", 0x0000, 0x0010, true),
                area("PRG", 0x8000, 0x8000, true),
                area("CHR", 0x0000, 0x2000, true),
            ],
            segments: vec![
                seg("ZEROPAGE", 0, None),
                seg("OAM", 1, None),
                seg("BSS", 2, None),
                seg("HEADER", 3, None),
                seg("CODE", 4, None),
                seg("VECTORS", 4, Some(0xFFFA)),
                seg("CHARS", 5, None),
            ],
            fill: 0x00,
        }
    }

    /// Resolve every segment to its base and file position. `lengths` feeds
    /// the running cursor, so a segment with no `start` lands after whatever
    /// its area already holds; a layout whose floating segments are untouched
    /// (the default layout under today's sources) resolves identically with
    /// an empty map.
    ///
    /// # Errors
    /// A `start` behind the cursor — the area is already filled past it.
    pub(crate) fn resolve(
        &self,
        lengths: &std::collections::BTreeMap<String, u32>,
    ) -> Result<ResolvedLayout, AsmError> {
        // File areas stack in declaration order, each contributing its full
        // extent (the bounded `fill = yes` shape).
        let mut file_base = vec![0usize; self.areas.len()];
        let mut image_size = 0usize;
        for (i, a) in self.areas.iter().enumerate() {
            if a.in_file {
                file_base[i] = image_size;
                image_size += a.size as usize;
            }
        }
        let addressable: Vec<bool> = self
            .areas
            .iter()
            .map(|a| {
                !a.in_file
                    || !self.areas.iter().any(|o| {
                        !o.in_file
                            && a.start < o.start.saturating_add(o.size)
                            && o.start < a.start.saturating_add(a.size)
                    })
            })
            .collect();

        let mut cursor: Vec<u32> = self.areas.iter().map(|a| a.start).collect();
        let mut segs = Vec::with_capacity(self.segments.len());
        for s in &self.segments {
            let a = &self.areas[s.area];
            let mut at = cursor[s.area];
            if let Some(align) = s.align.filter(|&al| al > 1) {
                at = at.div_ceil(align) * align;
            }
            if let Some(start) = s.start {
                if start < at {
                    return Err(AsmError::new(
                        0,
                        format!(
                            "segment `{}` starts at ${start:04X}, but memory area `{}` \
                             is already filled to ${at:04X}",
                            s.name, a.name
                        ),
                    ));
                }
                at = start;
            }
            segs.push(ResolvedSeg {
                name: s.name.clone(),
                base: at,
                file_at: a
                    .in_file
                    .then(|| file_base[s.area] + (at - a.start) as usize),
                cpu_addressable: addressable[s.area],
            });
            cursor[s.area] = at + lengths.get(&s.name).copied().unwrap_or(0);
        }
        Ok(ResolvedLayout {
            segs,
            image_size,
            fill: self.fill,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default layout resolves to the exact table the dialect used to
    /// compile in — same rows, same order, same file offsets, same
    /// addressability split — with no lengths at all.
    #[test]
    fn the_default_layout_is_the_historical_table() {
        let r = Layout::nes_default()
            .resolve(&Default::default())
            .expect("resolves");
        let rows: Vec<(&str, u32, Option<usize>, bool)> = r
            .segs
            .iter()
            .map(|s| (s.name.as_str(), s.base, s.file_at, s.cpu_addressable))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("ZEROPAGE", 0x0000, None, true),
                ("OAM", 0x0200, None, true),
                ("BSS", 0x0300, None, true),
                ("HEADER", 0x0000, Some(0), false),
                ("CODE", 0x8000, Some(0x10), true),
                ("VECTORS", 0xFFFA, Some(0x10 + 0x7FFA), true),
                ("CHARS", 0x0000, Some(0x10 + 0x8000), false),
            ]
        );
        assert_eq!(r.image_size, 0x10 + 0x8000 + 0x2000);
        assert_eq!(r.fill, 0x00);
    }

    /// A floating segment lands after its area-mate's recorded length — the
    /// sequential rule real ld65 configs rely on (RODATA after CODE).
    #[test]
    fn a_floating_segment_follows_its_predecessor() {
        let mut layout = Layout::nes_default();
        layout.segments.insert(
            5,
            SegmentDef {
                name: "RODATA".to_string(),
                area: 4,
                start: None,
                align: None,
            },
        );
        let mut lengths = std::collections::BTreeMap::new();
        lengths.insert("CODE".to_string(), 0x123);
        let r = layout.resolve(&lengths).expect("resolves");
        let rodata = r.seg("RODATA").expect("placed");
        assert_eq!(rodata.base, 0x8123);
        assert_eq!(rodata.file_at, Some(0x10 + 0x123));
    }

    /// A pinned start the cursor has already passed is the area-full error,
    /// named like ld65 names it.
    #[test]
    fn a_start_behind_the_cursor_is_refused() {
        let mut lengths = std::collections::BTreeMap::new();
        lengths.insert("CODE".to_string(), 0x8000 - 5);
        let e = Layout::nes_default()
            .resolve(&lengths)
            .expect_err("VECTORS start is behind CODE's end");
        assert!(e.message.contains("VECTORS"), "{}", e.message);
        assert!(e.message.contains("PRG"), "{}", e.message);
    }
}
