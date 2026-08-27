//! **debug198x** — the 198x family's cross-CPU debug-info format.
//!
//! One machine-readable sidecar describes what an assembled image *means*: which
//! source line produced each byte range (the line map), every symbol with its
//! kind and address, the sections/segments the image is laid out in, and — where
//! it matters — the address space each address lives in (a flat space, or a
//! paged/banked slot). Asm198x writes it; Emu198x (and any other
//! consumer) reads it for symbolized disassembly and source-anchored breakpoints.
//!
//! ## Serialization — NDJSON
//!
//! One JSON object per line, discriminated by a `t` field (`Record`). Every
//! consumer already has a JSON parser, records grep and diff line-stably, and a
//! reader **skips record types it does not recognize** — so the format grows
//! additively without breaking older readers (the guarantee that freezes at v1).
//!
//! ## Addresses — (section, offset)
//!
//! Every address-bearing record names a **section** and a **section-relative
//! offset**, never a bare absolute address. A [`Section`] may carry an absolute
//! `base`; a flat or linked-absolute image is the degenerate case — one section
//! whose base is its load address, so its records read as absolute with no
//! ceremony. Relocatable output (Amiga hunks) keeps section-relative offsets and
//! the reader's lookups take an optional **base map** — the consumer supplies the
//! real per-section load addresses at import time. See the plan's KTD7.
//!
//! This crate owns the format alone: types, [writer](DebugInfo::write), and
//! [reader](DebugInfo::read) with the three lookups Emu198x needs
//! ([`symbol_at`](DebugInfo::symbol_at), [`addr_of`](DebugInfo::addr_of),
//! [`line_at`](DebugInfo::line_at)). It depends only on serde, never on the
//! assembler.

use std::collections::BTreeMap;
use std::io::{self, Write};

use serde::{Deserialize, Serialize};

/// The format name written in (and required of) every file's [`Header`].
pub const FORMAT: &str = "debug198x";

/// The format version this crate reads and writes. Draft (`0.x`) until the first
/// consumer ships, after which v1 freezes the additive-evolution guarantee.
pub const FORMAT_VERSION: &str = "0.1";

/// A section identifier — the flat engine emits a single section `0`; the linked
/// paths (ca65 segments, vasm hunks) number theirs in layout order.
pub type SectionId = u32;

/// An override map from [`SectionId`] to an absolute base address, supplied by a
/// consumer that knows where relocatable sections actually loaded (Emu198x hands
/// in hunk load addresses). Absent entries fall back to the section's own `base`.
///
/// # A banked machine's paging state *is* the base map
///
/// On a paged machine the map is not static setup — it is the live paging
/// state, and it carries the answer to "which page is in this slot". Map **only
/// the sections currently paged in**. A section that is neither mapped here nor
/// carrying its own `base` does not resolve, and that silence is the mechanism:
/// it keeps a paged-out bank out of every lookup.
///
/// So for a Spectrum 128 with bank 1 in slot 3, map section `bank1` to `$C000`
/// and leave `bank3` unmapped; `symbol_at($C010)` then names `draw`. Page bank 3
/// in and the map swaps with it, and the same address names `music`.
///
/// Mapping two pages of one slot to the same address at once describes a state
/// the hardware cannot be in — one slot holds one page. The lookups will answer
/// from whichever record comes first rather than reporting the contradiction, so
/// build the map from the machine's real paging state and drop entries for banks
/// that page out.
pub type BaseMap = BTreeMap<SectionId, u64>;

/// The address space an address lives in. Absent (`None` on a record) means the
/// ordinary flat space — flat CPUs emit nothing extra, and most machines need
/// nothing more. The one populated shape is [`Paged`](Space::Paged), for banked
/// machines (Spectrum 128 slots, NES mappers).
///
/// # Unknown shapes are carried, not fatal
///
/// A shape this reader does not know deserializes into
/// [`Unknown`](Space::Unknown), preserving it verbatim, so a newer producer's
/// file still loads and still round-trips. That is the skip-unknown guarantee
/// applied one level down: without it, `#[serde(untagged)]` makes an
/// unrecognized shape a hard parse error that fails the whole file — which would
/// close the set of shapes permanently at the freeze.
///
/// The cost is strictness: a misspelled qualifier lands in `Unknown` rather than
/// being rejected. That is the intended trade — a consumer that does not
/// recognize a space should ignore that record's qualifier, not refuse the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Space {
    /// A banked/paged location: a `page` (bank) of memory, and the hardware
    /// `slot` the producer expected it in.
    ///
    /// # `page` is the join key; `slot` is not
    ///
    /// The two fields are **not** a composite key, and matching on the pair is
    /// the natural-looking mistake. `page` is the durable fact: a bank of code
    /// belongs to a page, and which slot it appears in is a fact about the
    /// machine right now. `slot` records where the *producer* expected it.
    ///
    /// So a consumer turning paging state into a [`BaseMap`] matches on `page`
    /// alone and takes the address from the slot it knows the page is in:
    ///
    /// ```
    /// # use debug198x::{BaseMap, DebugInfo, Space};
    /// # fn f(info: &DebugInfo, slot: u8, page: u16, slot_addr: u64) -> BaseMap {
    /// info.sections
    ///     .iter()
    ///     .filter(|s| matches!(s.space, Some(Space::Paged { page: p, .. }) if p == page))
    ///     .map(|s| (s.id, slot_addr))
    ///     .collect()
    /// # }
    /// ```
    ///
    /// Matching `Space::Paged { slot, page }` as a pair instead makes a bank
    /// invisible wherever the machine has put it other than where the assembler
    /// expected, and makes a bank live in *two* slots impossible by
    /// construction. That case is real: a Spectrum 128 keeps bank 5 at `$4000`
    /// permanently **and** can select it into `$C000`, so one page is live at
    /// two CPU addresses at once. Resolving each address through the slot that
    /// contains it keeps `symbol_at`/`line_at` unambiguous however many windows
    /// a page appears in, because every CPU address is in exactly one slot.
    Paged { slot: u8, page: u16 },
    /// A shape this reader does not know, carried verbatim.
    ///
    /// Never written deliberately: a producer emits a shape it understands or no
    /// `space` at all (AE3's no-fabrication rule). This exists so that reading a
    /// file from a newer producer degrades to "there is a qualifier here I cannot
    /// interpret" instead of failing. Treat a record whose space is `Unknown` as
    /// one whose address space you cannot reason about — resolve it if its
    /// section resolves, and do not guess at the qualifier's meaning.
    ///
    /// # Surface it; do not let it look like nothing
    ///
    /// This variant trades a loud failure for a quiet one, and the quiet failure
    /// is the harder of the two to diagnose. A section whose shape a consumer
    /// cannot read matches no paging state, so it never maps and its symbols
    /// never resolve — which from the outside is **indistinguishable from a bank
    /// that is simply paged out**. Both look like "no symbols here". A producer
    /// typo then costs symbols instead of raising an error, and whoever is
    /// debugging it has no thread to pull.
    ///
    /// So a consumer that holds `Unknown` records owes its caller a way to find
    /// out: some means of asking "is anything here described in a way I cannot
    /// read?", so an unexpected absence of symbols can be told apart from a
    /// legitimate one. Emu198x's importer does this with an `unreadable_spaces()`
    /// query. Carrying an unknown shape is only safe if someone can discover that
    /// it happened.
    Unknown(serde_json::Value),
}

/// The kind of a [`Symbol`], with the fields that kind carries. Address kinds
/// (`Label`, `Entry`) carry a `(section, offset)` location and an optional
/// address-space qualifier; a `Const` carries a plain value and no space, so a
/// label and a constant that resolve to the same number stay distinguishable by
/// kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SymbolKind {
    /// A code/data label at a section-relative location.
    Label {
        section: SectionId,
        offset: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<Space>,
    },
    /// An entry point (from an `end <addr>` directive) at a location.
    Entry {
        section: SectionId,
        offset: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<Space>,
    },
    /// A constant (an `equ`/`=` definition) — a value, not an address.
    Const { value: u64 },
}

/// The self-identifying first record: the format and tool that produced the file,
/// and what it describes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// Always [`FORMAT`] (`"debug198x"`).
    pub format: String,
    /// The [`FORMAT_VERSION`] the file conforms to.
    pub format_version: String,
    /// The producing tool (`"asm198x"`).
    pub tool: String,
    /// The producing tool's version.
    pub tool_version: String,
    /// The target CPU (`"z80"`, `"cp1610"`, …).
    pub cpu: String,
    /// The source dialect (`"pasmo"`, `"acme"`, …).
    pub dialect: String,
    /// The source file(s) that produced the image.
    pub sources: Vec<String>,
}

/// A section/segment of the image. `base` is its absolute load address when
/// known (flat and linked-absolute paths); relocatable sections leave it `None`
/// and rely on a [`BaseMap`] at lookup time.
///
/// A section may also carry a [`Space`] — for a banked machine, the (slot, page)
/// the whole section lives in. That is what lets a consumer turn a machine's
/// paging state into a [`BaseMap`] mechanically: find the sections whose `space`
/// names the page now in a slot, map those to the slot's address, leave the rest
/// unmapped. Without it the mapping can only be inferred by scraping a symbol,
/// and a section holding only line records cannot be placed at all.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: SectionId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<u64>,
    /// The address space this section as a whole lives in, when it needs one.
    /// A record's own `space` is the finer truth where it carries one; this is
    /// the section-wide default, and the only qualifier a [`LineSpan`] has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<Space>,
}

/// A symbol: a name and its [`SymbolKind`] (which carries the location or value).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    #[serde(flatten)]
    pub kind: SymbolKind,
}

/// A line→byte-range span: `length` bytes at `(section, offset)` were produced by
/// `line` of `file`. Bytes with no source (org gaps, align fill) get no span.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineSpan {
    pub file: String,
    pub line: u32,
    pub section: SectionId,
    pub offset: u64,
    pub length: u64,
}

/// One NDJSON record, tagged by `t`. Used for serialization; on read, an unknown
/// `t` is skipped rather than deserialized here (see [`DebugInfo::read`]).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Record<'a> {
    Header(&'a Header),
    Section(&'a Section),
    Symbol(&'a Symbol),
    Line(&'a LineSpan),
}

/// The whole debug record for one assembled image — the in-memory shape the
/// writer serializes and the reader parses into, plus the consumer lookups.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DebugInfo {
    pub header: Header,
    pub sections: Vec<Section>,
    pub symbols: Vec<Symbol>,
    pub lines: Vec<LineSpan>,
}

impl Default for Header {
    fn default() -> Self {
        Header {
            format: FORMAT.to_string(),
            format_version: FORMAT_VERSION.to_string(),
            tool: String::new(),
            tool_version: String::new(),
            cpu: String::new(),
            dialect: String::new(),
            sources: Vec::new(),
        }
    }
}

/// A failure reading a `.debug198x` file: malformed JSON on some line. An unknown
/// record *type* is not an error — it is skipped.
#[derive(Debug)]
pub enum ReadError {
    /// Line `line` (1-based) was not valid JSON, or a known record was malformed.
    Json {
        line: usize,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Json { line, source } => {
                write!(f, "line {line}: {source}")
            }
        }
    }
}

impl std::error::Error for ReadError {}

/// Deserialize a JSON value into a known record type, tagging any error with its
/// source line.
fn from_value<T: for<'de> Deserialize<'de>>(
    value: serde_json::Value,
    line: usize,
) -> Result<T, ReadError> {
    serde_json::from_value(value).map_err(|source| ReadError::Json { line, source })
}

impl DebugInfo {
    /// Serialize to NDJSON — the header first, then sections, symbols, and line
    /// spans, one JSON object per line.
    ///
    /// # Errors
    /// Propagates any write error from `w`.
    pub fn write<W: Write>(&self, mut w: W) -> io::Result<()> {
        let mut emit = |rec: &Record| -> io::Result<()> {
            // A struct of plain fields cannot fail to serialize to a JSON string.
            let line = serde_json::to_string(rec).expect("record serializes");
            writeln!(w, "{line}")
        };
        emit(&Record::Header(&self.header))?;
        for s in &self.sections {
            emit(&Record::Section(s))?;
        }
        for s in &self.symbols {
            emit(&Record::Symbol(s))?;
        }
        for l in &self.lines {
            emit(&Record::Line(l))?;
        }
        Ok(())
    }

    /// Serialize to an NDJSON string (a convenience over [`write`](Self::write)).
    #[must_use]
    pub fn to_ndjson(&self) -> String {
        let mut buf = Vec::new();
        self.write(&mut buf).expect("writing to a Vec cannot fail");
        String::from_utf8(buf).expect("serde_json emits UTF-8")
    }

    /// Parse NDJSON. Blank lines are ignored; a record whose `t` is unrecognized
    /// is **skipped** (the additive-evolution guarantee), so a newer file still
    /// reads on an older reader. The last `header` record wins.
    ///
    /// # Errors
    /// Returns [`ReadError::Json`] if a line is not valid JSON or a known record
    /// type is malformed.
    pub fn read(ndjson: &str) -> Result<Self, ReadError> {
        let mut info = DebugInfo::default();
        for (i, raw) in ndjson.lines().enumerate() {
            let line = i + 1;
            if raw.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value =
                serde_json::from_str(raw).map_err(|source| ReadError::Json { line, source })?;
            match value.get("t").and_then(serde_json::Value::as_str) {
                Some("header") => info.header = from_value(value, line)?,
                Some("section") => info.sections.push(from_value(value, line)?),
                Some("symbol") => info.symbols.push(from_value(value, line)?),
                Some("line") => info.lines.push(from_value(value, line)?),
                _ => {} // unknown or missing `t` — skip
            }
        }
        Ok(info)
    }

    /// The absolute base of a section: the `bases` override if present, else the
    /// section's own `base`, else `None` (a relocatable section with no supplied
    /// address — its records can't resolve to an absolute address).
    fn base_of(&self, id: SectionId, bases: Option<&BaseMap>) -> Option<u64> {
        bases.and_then(|b| b.get(&id).copied()).or_else(|| {
            self.sections
                .iter()
                .find(|s| s.id == id)
                .and_then(|s| s.base)
        })
    }

    /// The absolute address of a `(section, offset)` location, or `None` if the
    /// section's base is unknown.
    fn absolute(&self, section: SectionId, offset: u64, bases: Option<&BaseMap>) -> Option<u64> {
        Some(self.base_of(section, bases)?.wrapping_add(offset))
    }

    /// The symbol defined at absolute address `addr` (an address-kind symbol whose
    /// resolved location equals `addr`), or `None`. `bases` optionally overrides
    /// section bases for relocatable images, and on a banked machine carries the
    /// paging state — map only the sections paged in, per [`BaseMap`].
    ///
    /// The first matching record wins, so two sections mapped to the same address
    /// make the result record order. That is unreachable for a base map built from
    /// a real paging state, where one slot holds one page.
    #[must_use]
    pub fn symbol_at(&self, addr: u64, bases: Option<&BaseMap>) -> Option<&Symbol> {
        self.symbols.iter().find(|sym| match sym.kind {
            SymbolKind::Label {
                section, offset, ..
            }
            | SymbolKind::Entry {
                section, offset, ..
            } => self.absolute(section, offset, bases) == Some(addr),
            SymbolKind::Const { .. } => false,
        })
    }

    /// The value of the named symbol: the absolute address for an address kind, or
    /// the constant's value. `None` if the name is unknown or its section base is —
    /// which is the answer for a symbol in a bank that is currently paged out, since
    /// on a banked machine `bases` is the paging state (see [`BaseMap`]).
    #[must_use]
    pub fn addr_of(&self, name: &str, bases: Option<&BaseMap>) -> Option<u64> {
        let sym = self.symbols.iter().find(|s| s.name == name)?;
        match sym.kind {
            SymbolKind::Label {
                section, offset, ..
            }
            | SymbolKind::Entry {
                section, offset, ..
            } => self.absolute(section, offset, bases),
            SymbolKind::Const { value } => Some(value),
        }
    }

    /// The line span covering absolute address `addr` — the span whose
    /// `[base+offset, base+offset+length)` range contains it — or `None`.
    ///
    /// Line records carry no space qualifier of their own; a banked span is
    /// selected by its section being mapped in `bases`, per [`BaseMap`].
    #[must_use]
    pub fn line_at(&self, addr: u64, bases: Option<&BaseMap>) -> Option<&LineSpan> {
        self.lines.iter().find(|span| {
            let Some(start) = self.absolute(span.section, span.offset, bases) else {
                return false;
            };
            // `addr - start` (once `addr >= start`) can't overflow, so a span
            // reaching the top of the address space still matches.
            addr >= start && addr - start < span.length
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small but representative record: a header, one based section, a label,
    /// an entry, a constant, and two line spans.
    fn sample() -> DebugInfo {
        DebugInfo {
            header: Header {
                cpu: "z80".into(),
                dialect: "pasmo".into(),
                tool: "asm198x".into(),
                tool_version: "0.0.7".into(),
                sources: vec!["prog.asm".into()],
                ..Header::default()
            },
            sections: vec![Section {
                id: 0,
                name: "CODE".into(),
                base: Some(0xC000),
                space: None,
            }],
            symbols: vec![
                Symbol {
                    name: "init".into(),
                    kind: SymbolKind::Label {
                        section: 0,
                        offset: 0x12,
                        space: None,
                    },
                },
                Symbol {
                    name: "main".into(),
                    kind: SymbolKind::Entry {
                        section: 0,
                        offset: 0,
                        space: None,
                    },
                },
                Symbol {
                    name: "MAX".into(),
                    kind: SymbolKind::Const { value: 255 },
                },
            ],
            lines: vec![
                LineSpan {
                    file: "prog.asm".into(),
                    line: 5,
                    section: 0,
                    offset: 0,
                    length: 3,
                },
                LineSpan {
                    file: "prog.asm".into(),
                    line: 6,
                    section: 0,
                    offset: 3,
                    length: 2,
                },
            ],
        }
    }

    #[test]
    fn round_trip_preserves_every_field() {
        let info = sample();
        let back = DebugInfo::read(&info.to_ndjson()).expect("parse");
        assert_eq!(info, back);
    }

    #[test]
    fn header_is_the_first_record_and_names_the_format() {
        let ndjson = sample().to_ndjson();
        let first = ndjson.lines().next().expect("present");
        assert!(first.contains(r#""t":"header""#));
        assert!(first.contains(r#""format":"debug198x""#));
    }

    #[test]
    fn lookups_resolve_against_section_base() {
        let info = sample();
        // init is at section 0 (base 0xC000) offset 0x12 -> 0xC012.
        assert_eq!(info.addr_of("init", None), Some(0xC012));
        assert_eq!(info.symbol_at(0xC012, None).expect("present").name, "init");
        // A line span: offset 0..3 -> 0xC000..0xC003.
        assert_eq!(info.line_at(0xC000, None).expect("present").line, 5);
        assert_eq!(info.line_at(0xC002, None).expect("present").line, 5);
        assert_eq!(info.line_at(0xC003, None).expect("present").line, 6);
        // An address in no span resolves to nothing.
        assert!(info.line_at(0xC005, None).is_none());
        assert!(info.symbol_at(0xDEAD, None).is_none());
    }

    #[test]
    fn const_is_not_an_address_but_addr_of_returns_its_value() {
        let info = sample();
        assert_eq!(info.addr_of("MAX", None), Some(255));
        // A const is never returned by an address->symbol lookup, even at 255.
        assert!(info.symbol_at(255, None).is_none_or(|s| s.name != "MAX"));
    }

    #[test]
    fn covers_ae5_unknown_record_is_skipped_and_lookups_still_resolve() {
        let mut ndjson = sample().to_ndjson();
        // A future record type the reader has never seen, mid-file.
        ndjson.push_str(r#"{"t":"macro_frame","name":"delay","from":10,"to":14}"#);
        ndjson.push('\n');
        let info = DebugInfo::read(&ndjson).expect("unknown record skipped, not an error");
        assert_eq!(info.addr_of("init", None), Some(0xC012));
        assert_eq!(info.symbol_at(0xC012, None).expect("present").name, "init");
    }

    #[test]
    fn symbol_name_with_quotes_backslash_and_non_ascii_round_trips() {
        let mut info = sample();
        info.symbols.push(Symbol {
            name: r#"lbl "quoted"\slash café_π"#.into(),
            kind: SymbolKind::Const { value: 1 },
        });
        let back = DebugInfo::read(&info.to_ndjson()).expect("parse");
        assert_eq!(info, back);
        assert_eq!(back.addr_of(r#"lbl "quoted"\slash café_π"#, None), Some(1));
    }

    #[test]
    fn label_and_const_with_equal_values_are_distinguishable() {
        // A label whose absolute address equals a constant's value: kind + base
        // keep them apart.
        let info = DebugInfo {
            sections: vec![Section {
                id: 0,
                name: "S".into(),
                base: Some(100),
                space: None,
            }],
            symbols: vec![
                Symbol {
                    name: "here".into(),
                    kind: SymbolKind::Label {
                        section: 0,
                        offset: 0,
                        space: None,
                    },
                },
                Symbol {
                    name: "HUNDRED".into(),
                    kind: SymbolKind::Const { value: 100 },
                },
            ],
            ..Default::default()
        };
        let back = DebugInfo::read(&info.to_ndjson()).expect("parse");
        assert_eq!(back, info);
        // Both resolve to 100, but only the label answers an address lookup.
        assert_eq!(back.addr_of("here", None), Some(100));
        assert_eq!(back.addr_of("HUNDRED", None), Some(100));
        assert_eq!(back.symbol_at(100, None).expect("present").name, "here");
    }

    #[test]
    fn u64_boundary_address_round_trips() {
        let info = DebugInfo {
            sections: vec![Section {
                id: 0,
                name: "S".into(),
                base: Some(u64::MAX - 4),
                space: None,
            }],
            symbols: vec![Symbol {
                name: "top".into(),
                kind: SymbolKind::Const { value: u64::MAX },
            }],
            lines: vec![LineSpan {
                file: "f".into(),
                line: 1,
                section: 0,
                offset: 4,
                length: 1,
            }],
            ..Default::default()
        };
        let back = DebugInfo::read(&info.to_ndjson()).expect("parse");
        assert_eq!(back, info);
        assert_eq!(back.addr_of("top", None), Some(u64::MAX));
        assert_eq!(back.line_at(u64::MAX, None).expect("present").line, 1);
    }

    #[test]
    fn empty_program_is_header_only_and_parses() {
        let info = DebugInfo::default();
        let ndjson = info.to_ndjson();
        assert_eq!(ndjson.lines().count(), 1); // just the header
        assert_eq!(DebugInfo::read(&ndjson).expect("parse"), info);
    }

    #[test]
    fn two_sections_resolve_relative_and_rebased() {
        // A relocatable image: section 1 has no base of its own.
        let info = DebugInfo {
            sections: vec![
                Section {
                    id: 0,
                    name: "text".into(),
                    base: Some(0),
                    space: None,
                },
                Section {
                    id: 1,
                    name: "data".into(),
                    base: None,
                    space: None,
                },
            ],
            symbols: vec![
                Symbol {
                    name: "start".into(),
                    kind: SymbolKind::Label {
                        section: 0,
                        offset: 8,
                        space: None,
                    },
                },
                Symbol {
                    name: "table".into(),
                    kind: SymbolKind::Label {
                        section: 1,
                        offset: 0x10,
                        space: None,
                    },
                },
            ],
            ..Default::default()
        };
        // Without a base map: section 0 (base 0) resolves; section 1 (no base)
        // does not.
        assert_eq!(info.addr_of("start", None), Some(8));
        assert_eq!(info.addr_of("table", None), None);
        // With a base map placing section 1 at 0x40000, `table` resolves.
        let bases: BaseMap = [(1u32, 0x40000u64)].into_iter().collect();
        assert_eq!(info.addr_of("table", Some(&bases)), Some(0x40010));
        assert_eq!(
            info.symbol_at(0x40010, Some(&bases)).expect("present").name,
            "table"
        );
    }

    #[test]
    fn address_space_qualifiers_round_trip() {
        let info = DebugInfo {
            sections: vec![Section {
                id: 0,
                name: "S".into(),
                base: Some(0),
                space: Some(Space::Paged { slot: 3, page: 7 }),
            }],
            symbols: vec![Symbol {
                name: "paged".into(),
                kind: SymbolKind::Label {
                    section: 0,
                    offset: 0xC000,
                    space: Some(Space::Paged { slot: 3, page: 7 }),
                },
            }],
            ..Default::default()
        };
        let back = DebugInfo::read(&info.to_ndjson()).expect("parse");
        assert_eq!(back, info);
        // A flat symbol carries no space field at all (AE3's no-fabrication rule).
        let flat = Symbol {
            name: "flat".into(),
            kind: SymbolKind::Label {
                section: 0,
                offset: 0,
                space: None,
            },
        };
        let json = serde_json::to_string(&Record::Symbol(&flat)).expect("present");
        assert!(
            !json.contains("space"),
            "flat symbol must not emit a space field: {json}"
        );
        // Nor does a flat section — the same rule one level up.
        let flat_section = Section {
            id: 0,
            name: "S".into(),
            base: Some(0),
            space: None,
        };
        let json = serde_json::to_string(&Record::Section(&flat_section)).expect("present");
        assert!(
            !json.contains("space"),
            "flat section must not emit a space field: {json}"
        );
    }

    /// AE5 one level down: a space shape this reader does not know must be
    /// carried, not fatal. Before `Space::Unknown` existed, `#[serde(untagged)]`
    /// made this a hard parse error that failed the entire file — which would
    /// have closed the set of shapes permanently at the v1 freeze.
    #[test]
    fn an_unknown_space_shape_is_carried_not_fatal() {
        let future = concat!(
            r#"{"t":"section","id":0,"name":"main","base":0}"#,
            "\n",
            r#"{"t":"symbol","name":"here","kind":"label","section":0,"offset":4,"#,
            r#""space":{"segment":7,"window":2}}"#,
        );
        let info = DebugInfo::read(future).expect("an unknown space shape must not fail the file");

        // The qualifier is opaque; the address is not.
        assert_eq!(info.addr_of("here", None), Some(4));
        assert_eq!(
            info.symbol_at(4, None).map(|s| &*s.name),
            Some("here"),
            "an unreadable qualifier must not cost the lookup"
        );

        // Carried verbatim, so a reader that rewrites the file does not silently
        // drop what it could not interpret.
        let SymbolKind::Label { space, .. } = &info.symbols[0].kind else {
            panic!("expected a label");
        };
        let Some(Space::Unknown(raw)) = space else {
            panic!("expected the catch-all, got {space:?}");
        };
        assert_eq!(raw["segment"], 7);
        assert_eq!(raw["window"], 2);
        assert!(
            info.to_ndjson()
                .contains(r#""space":{"segment":7,"window":2}"#)
        );
    }

    /// The `bank` shape was specified in draft and withdrawn before the freeze,
    /// having never been emitted, fixtured, or read. A file that still carries
    /// one must keep loading: it lands in the catch-all like any other shape this
    /// reader does not know.
    #[test]
    fn a_withdrawn_bank_shape_still_loads() {
        let legacy = concat!(
            r#"{"t":"section","id":0,"name":"main","base":0}"#,
            "\n",
            r#"{"t":"symbol","name":"far","kind":"label","section":0,"offset":16,"#,
            r#""space":{"bank":126}}"#,
        );
        let info = DebugInfo::read(legacy).expect("a withdrawn shape must not fail the file");
        assert_eq!(info.addr_of("far", None), Some(16));
        let SymbolKind::Label { space, .. } = &info.symbols[0].kind else {
            panic!("expected a label");
        };
        assert!(matches!(space, Some(Space::Unknown(_))), "got {space:?}");
    }

    /// The `page` is the join key and the `slot` is not — the case that proves
    /// it. A Spectrum 128 keeps bank 5 at `$4000` permanently *and* can select
    /// it into `$C000`, so one page is live at two CPU addresses at once. A
    /// consumer matching `Space::Paged { slot, page }` as a pair finds nothing
    /// for the second window, because the section records the slot the producer
    /// expected.
    #[test]
    fn a_page_live_in_two_slots_resolves_through_each_window() {
        let info = DebugInfo {
            sections: vec![Section {
                id: 0,
                name: "bank5".into(),
                base: None,
                space: Some(Space::Paged { slot: 1, page: 5 }),
            }],
            symbols: vec![Symbol {
                name: "screen_fill".into(),
                kind: SymbolKind::Label {
                    section: 0,
                    offset: 0x10,
                    space: Some(Space::Paged { slot: 1, page: 5 }),
                },
            }],
            ..Default::default()
        };

        // Match on the page alone; the address comes from the slot the caller
        // knows that page is currently in.
        let bases_for = |slot: u8, page: u16| -> BaseMap {
            info.sections
                .iter()
                .filter(|s| matches!(s.space, Some(Space::Paged { page: p, .. }) if p == page))
                .map(|s| (s.id, 0x4000_u64 * u64::from(slot)))
                .collect()
        };

        // The window the producer expected: bank 5 fixed at $4000.
        let fixed = bases_for(1, 5);
        assert_eq!(info.addr_of("screen_fill", Some(&fixed)), Some(0x4010));

        // The same page selected into slot 3. A `{ slot, page }` pair match
        // would yield an empty map here and resolve nothing.
        let selected = bases_for(3, 5);
        assert_eq!(info.addr_of("screen_fill", Some(&selected)), Some(0xC010));
        assert_eq!(
            info.symbol_at(0xC010, Some(&selected)).map(|s| &*s.name),
            Some("screen_fill")
        );
    }
}
