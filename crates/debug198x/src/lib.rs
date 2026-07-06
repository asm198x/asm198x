//! **debug198x** — the 198x family's cross-CPU debug-info format.
//!
//! One machine-readable sidecar describes what an assembled image *means*: which
//! source line produced each byte range (the line map), every symbol with its
//! kind and address, the sections/segments the image is laid out in, and — where
//! it matters — the address space each address lives in (a flat 16-bit space, a
//! 65816 bank, a paged/banked slot). Asm198x writes it; Emu198x (and any other
//! consumer) reads it for symbolized disassembly and source-anchored breakpoints.
//!
//! ## Serialization — NDJSON
//!
//! One JSON object per line, discriminated by a `t` field ([`Record`]). Every
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
pub type BaseMap = BTreeMap<SectionId, u64>;

/// The address space an address lives in. Absent (`None` on a record) means the
/// ordinary flat space — flat CPUs emit nothing extra. The populated shapes are
/// the 65816's [`Bank`](Space::Bank) and the [`Paged`](Space::Paged) form for
/// banked machines (Spectrum 128 slots, NES mappers).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Space {
    /// A 65816-style bank byte — the high 8 bits of a 24-bit address.
    Bank { bank: u8 },
    /// A banked/paged location: a hardware `slot` filled by a `page` (bank) of
    /// memory. The (slot, page) pair distinguishes two symbols that share the
    /// same low address in different pages.
    Paged { slot: u8, page: u16 },
}

/// The kind of a [`Symbol`], with the fields that kind carries. Address kinds
/// (`Label`, `Entry`) carry a `(section, offset)` location and an optional
/// address-space qualifier; a `Const` carries a plain value and no space — so a
/// bank-`$7E` label and a constant sharing its low bits stay distinguishable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub id: SectionId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<u64>,
}

/// A symbol: a name and its [`SymbolKind`] (which carries the location or value).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Default)]
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
    /// section bases for relocatable images.
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
    /// the constant's value. `None` if the name is unknown or its section base is.
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
                },
                Section {
                    id: 1,
                    name: "data".into(),
                    base: None,
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
            }],
            symbols: vec![
                Symbol {
                    name: "far".into(),
                    kind: SymbolKind::Label {
                        section: 0,
                        offset: 0x1234,
                        space: Some(Space::Bank { bank: 0x7E }),
                    },
                },
                Symbol {
                    name: "paged".into(),
                    kind: SymbolKind::Label {
                        section: 0,
                        offset: 0xC000,
                        space: Some(Space::Paged { slot: 3, page: 7 }),
                    },
                },
            ],
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
    }
}
