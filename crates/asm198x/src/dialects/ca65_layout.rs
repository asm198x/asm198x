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
#[derive(Debug)]
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
#[derive(Debug)]
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
#[derive(Debug)]
pub(crate) struct Layout {
    pub(crate) areas: Vec<Area>,
    pub(crate) segments: Vec<SegmentDef>,
    /// Whether this layout arrived as a project's `-C` config rather than as
    /// the built-in curriculum default — the rejection message names the
    /// right authority either way.
    pub(crate) from_config: bool,
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
    /// See [`Layout::from_config`].
    pub(crate) from_config: bool,
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
            from_config: false,
        }
    }

    /// Whether resolution needs segment lengths at all: true when some
    /// segment floats behind another in its area (bradsmith's `RODATA` after
    /// `CODE`). The default layout answers false, so the historical path
    /// never runs a sizing pass and cannot change behaviour.
    pub(crate) fn needs_lengths(&self) -> bool {
        self.segments
            .iter()
            .enumerate()
            .any(|(i, s)| s.start.is_none() && self.segments[..i].iter().any(|p| p.area == s.area))
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
            from_config: self.from_config,
        })
    }
}

/// Parse a bounded ld65 `.cfg`: the `MEMORY` and `SEGMENTS` blocks, with the
/// attributes the corpus shapes use — grown by evidence, never the full ld65
/// grammar up front (`decisions/layouts-are-data.md`). Anything outside the
/// bound is refused by name, so a config leaning on unimplemented semantics
/// fails loudly instead of linking wrong.
///
/// # Errors
/// An unknown block, attribute, or value shape; a file area without
/// `fill = yes` (partial-extent output is outside the bound); differing
/// `fillval`s (the image carries one fill byte); a segment loading into an
/// unknown area.
pub(crate) fn from_cfg(text: &str) -> Result<Layout, AsmError> {
    // ld65 config comments run # to end of line.
    let clean: String = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");

    let mut areas: Vec<Area> = Vec::new();
    let mut fills: Vec<u8> = Vec::new();
    let mut segments: Vec<SegmentDef> = Vec::new();

    let mut rest = clean.trim();
    while !rest.is_empty() {
        let (block, after) = rest.split_once('{').ok_or_else(|| {
            AsmError::new(0, "config: expected `NAME {` to open a block".to_string())
        })?;
        let block = block.trim();
        let (body, tail) = after
            .split_once('}')
            .ok_or_else(|| AsmError::new(0, format!("config: block `{block}` is never closed")))?;
        match block {
            "MEMORY" => {
                for entry in entries(body) {
                    let (name, attrs) = entry?;
                    let mut start = None;
                    let mut size = None;
                    let mut in_file = false;
                    let mut fill = false;
                    let mut fillval = 0x00u8;
                    for (key, value) in attrs {
                        match key.as_str() {
                            "start" => start = Some(number(&value, &name)?),
                            "size" => size = Some(number(&value, &name)?),
                            // ro/rw affect ld65's write diagnostics, not
                            // placement; accepted so real configs parse.
                            "type" if matches!(value.as_str(), "ro" | "rw") => {}
                            "file" if value == "%O" => in_file = true,
                            "file" if value == "\"\"" => in_file = false,
                            "fill" if value == "yes" => fill = true,
                            "fill" if value == "no" => fill = false,
                            "fillval" => {
                                fillval = u8::try_from(number(&value, &name)?).map_err(|_| {
                                    AsmError::new(
                                        0,
                                        format!("config: `{name}` fillval does not fit a byte"),
                                    )
                                })?;
                            }
                            _ => {
                                return Err(AsmError::new(
                                    0,
                                    format!(
                                        "config: memory attribute `{key} = {value}` on `{name}` \
                                         is outside this bounded reader (grown from real \
                                         configs; see decisions/layouts-are-data.md)"
                                    ),
                                ));
                            }
                        }
                    }
                    let (Some(start), Some(size)) = (start, size) else {
                        return Err(AsmError::new(
                            0,
                            format!("config: memory area `{name}` needs `start` and `size`"),
                        ));
                    };
                    if in_file {
                        if !fill {
                            return Err(AsmError::new(
                                0,
                                format!(
                                    "config: file area `{name}` without `fill = yes` writes a \
                                     partial extent, which is outside this bounded reader"
                                ),
                            ));
                        }
                        fills.push(fillval);
                    }
                    areas.push(Area {
                        name,
                        start,
                        size,
                        in_file,
                    });
                }
            }
            "SEGMENTS" => {
                for entry in entries(body) {
                    let (name, attrs) = entry?;
                    let mut area = None;
                    let mut start = None;
                    let mut align = None;
                    for (key, value) in attrs {
                        match key.as_str() {
                            "load" => {
                                area =
                                    Some(areas.iter().position(|a| a.name == value).ok_or_else(
                                        || {
                                            AsmError::new(
                                                0,
                                                format!(
                                                    "config: segment `{name}` loads into \
                                                     `{value}`, which MEMORY does not define"
                                                ),
                                            )
                                        },
                                    )?);
                            }
                            // zp/bss/ro/rw select ld65 diagnostics; placement
                            // here follows the area, so they parse and pass.
                            "type" if matches!(value.as_str(), "zp" | "bss" | "ro" | "rw") => {}
                            "start" => start = Some(number(&value, &name)?),
                            "align" => align = Some(number(&value, &name)?),
                            _ => {
                                return Err(AsmError::new(
                                    0,
                                    format!(
                                        "config: segment attribute `{key} = {value}` on \
                                         `{name}` is outside this bounded reader"
                                    ),
                                ));
                            }
                        }
                    }
                    let Some(area) = area else {
                        return Err(AsmError::new(
                            0,
                            format!("config: segment `{name}` names no `load` area"),
                        ));
                    };
                    segments.push(SegmentDef {
                        name,
                        area,
                        start,
                        align,
                    });
                }
            }
            other => {
                return Err(AsmError::new(
                    0,
                    format!(
                        "config: block `{other}` is outside this bounded reader (MEMORY and \
                         SEGMENTS are taken; see decisions/layouts-are-data.md)"
                    ),
                ));
            }
        }
        rest = tail.trim();
    }
    fills.dedup();
    if fills.len() > 1 {
        return Err(AsmError::new(
            0,
            "config: file areas disagree on `fillval`; one fill byte per image is the bound"
                .to_string(),
        ));
    }
    if segments.is_empty() {
        return Err(AsmError::new(0, "config: no SEGMENTS block".to_string()));
    }
    Ok(Layout {
        areas,
        segments,
        fill: fills.first().copied().unwrap_or(0x00),
        from_config: true,
    })
}

/// Split a block body into `NAME: attr = value, ...;` entries.
fn entries(
    body: &str,
) -> impl Iterator<Item = Result<(String, Vec<(String, String)>), AsmError>> + '_ {
    body.split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(|entry| {
            let (name, attrs) = entry.split_once(':').ok_or_else(|| {
                AsmError::new(0, format!("config: expected `NAME: …` in `{entry}`"))
            })?;
            let attrs = attrs
                .split(',')
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(|attr| {
                    let (k, v) = attr.split_once('=').ok_or_else(|| {
                        AsmError::new(0, format!("config: expected `key = value` in `{attr}`"))
                    })?;
                    Ok((k.trim().to_string(), v.trim().to_string()))
                })
                .collect::<Result<Vec<_>, AsmError>>()?;
            Ok((name.trim().to_string(), attrs))
        })
}

/// `$hex` or decimal, as ld65 writes them.
fn number(value: &str, owner: &str) -> Result<u32, AsmError> {
    let parsed = if let Some(hex) = value.strip_prefix('$') {
        u32::from_str_radix(hex, 16)
    } else {
        value.parse()
    };
    parsed.map_err(|_| AsmError::new(0, format!("config: `{owner}`: `{value}` is not a number")))
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

    /// The pinned `bbbradsmith/NES-ca65-example` config (#430), verbatim,
    /// parses inside the bound and resolves the way ca65 2.18 + ld65 place
    /// it: `RODATA` floats after `CODE`, `TILES` opens the CHR area, and
    /// `VECTORS` pins at $FFFA.
    #[test]
    fn the_bradsmith_config_parses_and_places() {
        let cfg = CFG_EXAMPLE;
        let layout = from_cfg(cfg).expect("parses");
        let mut lengths = std::collections::BTreeMap::new();
        lengths.insert("CODE".to_string(), 0x0234u32);
        lengths.insert("RODATA".to_string(), 0x40);
        let r = layout.resolve(&lengths).expect("resolves");
        let rows: Vec<(&str, u32, Option<usize>)> = r
            .segs
            .iter()
            .map(|s| (s.name.as_str(), s.base, s.file_at))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("ZEROPAGE", 0x0000, None),
                ("OAM", 0x0200, None),
                ("BSS", 0x0300, None),
                ("HEADER", 0x0000, Some(0)),
                ("CODE", 0x8000, Some(0x10)),
                ("RODATA", 0x8234, Some(0x10 + 0x234)),
                ("VECTORS", 0xFFFA, Some(0x10 + 0x7FFA)),
                ("TILES", 0x0000, Some(0x10 + 0x8000)),
            ]
        );
        assert_eq!(r.image_size, 0x10 + 0x8000 + 0x2000);
        assert!(r.seg("TILES").expect("TILES placed").file_at.is_some());
        assert!(
            !r.seg("TILES").expect("TILES placed").cpu_addressable,
            "CHR is PPU space"
        );
        assert!(!r.seg("HEADER").expect("HEADER placed").cpu_addressable);
        assert!(r.seg("RODATA").expect("RODATA placed").cpu_addressable);
    }

    /// The refusals hold their bound: an unknown block, an unknown attribute,
    /// and a file area without fill all fail by name.
    #[test]
    fn out_of_bound_configs_are_refused_by_name() {
        let e = from_cfg("FEATURES { }\nSEGMENTS { A: load = X; }").expect_err("no FEATURES");
        assert!(e.message.contains("FEATURES"), "{}", e.message);
        let e =
            from_cfg("MEMORY { P: start = $0, size = $10, bank = 1; }\nSEGMENTS { A: load = P; }")
                .expect_err("no bank attr");
        assert!(e.message.contains("bank"), "{}", e.message);
        let e =
            from_cfg("MEMORY { P: start = $0, size = $10, file = %O; }\nSEGMENTS { A: load = P; }")
                .expect_err("file area needs fill");
        assert!(e.message.contains("fill"), "{}", e.message);
    }

    const CFG_EXAMPLE: &str = r#"MEMORY {
    ZP:     start = $00,    size = $0100, type = rw, file = "";
    OAM:    start = $0200,  size = $0100, type = rw, file = "";
    RAM:    start = $0300,  size = $0500, type = rw, file = "";
    HDR:    start = $0000,  size = $0010, type = ro, file = %O, fill = yes, fillval = $00;
    PRG:    start = $8000,  size = $8000, type = ro, file = %O, fill = yes, fillval = $00;
    CHR:    start = $0000,  size = $2000, type = ro, file = %O, fill = yes, fillval = $00;
}

SEGMENTS {
    ZEROPAGE: load = ZP,  type = zp;
    OAM:      load = OAM, type = bss, align = $100;
    BSS:      load = RAM, type = bss;
    HEADER:   load = HDR, type = ro;
    CODE:     load = PRG, type = ro,  start = $8000;
    RODATA:   load = PRG, type = ro;
    VECTORS:  load = PRG, type = ro,  start = $FFFA;
    TILES:    load = CHR, type = ro;
}
"#;

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
