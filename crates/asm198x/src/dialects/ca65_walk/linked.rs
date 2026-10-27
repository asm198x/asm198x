//! What a linked program gives besides its image: its debug record by
//! segment, and its memory map.
//!
//! The engine keeps each section's records apart. Here each section is
//! named by the segment it belongs to, as ld65 numbers them: a segment's
//! number is its place in the layout, and its offsets count from where the
//! segment starts. A section `.org` gave an address of its own becomes a
//! section of the debug record with that address, placed in the file where
//! its bytes went. A segment with no bytes in the file, such as `ZEROPAGE`
//! or `BSS`, keeps its labels but has no lines.
//!
//! ld65 drops what a `bss` or `zp` segment holds, and warns once for each
//! such segment that holds a byte other than zero. The warning is given at
//! the first line that made one.

use std::collections::BTreeMap;

use super::{Linked, Opened};
use crate::contract::AssemblyResult;
use crate::dialects::ca65_layout::{Layout, ResolvedSeg, area_usage};
use crate::engine::{Assembly, DebugData, SectionRecord, Warning};
use crate::listing::DebugCaptureMulti;

/// The result of a linked assembly, and its debug record.
pub(crate) fn result(
    layout: &Layout,
    linked: &Linked,
    assembly: Assembly,
) -> (AssemblyResult, DebugCaptureMulti) {
    let resolved = &linked.resolved;
    let mut sections: Vec<debug198x::Section> = resolved
        .segs
        .iter()
        .enumerate()
        .filter(|(_, seg)| linked.lengths.contains_key(&seg.name))
        .map(|(id, seg)| section(id, seg, u64::from(seg.base)))
        .collect();
    let mut capture = DebugCaptureMulti {
        cycle_coverage: assembly.debug.cycle_coverage,
        file_offsets: resolved
            .segs
            .iter()
            .enumerate()
            .filter_map(|(id, seg)| Some((id as debug198x::SectionId, seg.file_at? as u64)))
            .collect(),
        sections: Vec::new(),
        symbols: Vec::new(),
        lines: Vec::new(),
        cycles: Vec::new(),
    };
    let mut spans = Spans {
        next: resolved.segs.len(),
        last: BTreeMap::new(),
        sections: Vec::new(),
    };
    // The engine's first record is what came before the first section,
    // which holds nothing; the rest are the sections the walk opened and
    // placed, in order.
    let records = assembly.sections.iter().filter(|r| !r.name.is_empty());
    let mut warnings = assembly.warnings;
    let mut warned = Vec::new();
    for (record, opened) in records.zip(&linked.opened) {
        let Some((id, seg)) = resolved
            .segs
            .iter()
            .enumerate()
            .find(|(_, seg)| seg.name == opened.segment)
        else {
            continue;
        };
        // The iNES header is the file's, not the program's: the CPU never
        // sees it, so it has no lines.
        let lines = seg.file_at.is_some() && seg.name != "HEADER";
        // A section `.org` gave an address is in the record only once
        // something is in it.
        if holds_something(record, lines) {
            let (id, delta) = spans.locate(id, seg, opened, &mut capture.file_offsets);
            add(&mut capture, record, id, delta, lines);
        } else {
            add_constants(&mut capture, record);
        }
        let reserves = layout
            .segments
            .iter()
            .any(|def| def.name == seg.name && !def.initialized);
        if reserves
            && !warned.contains(&seg.name)
            && let Some(line) = first_data(record)
        {
            warned.push(seg.name.clone());
            let name = &seg.name;
            warnings.push(
                Warning::new(
                    line.line as usize,
                    format!("Segment `{name}` with type `bss` contains initialized data"),
                )
                .in_file(line.file),
            );
        }
    }
    sections.extend(spans.sections);
    capture.sections = sections;
    // Constants come first, by name, and then labels as the program made
    // them.
    let (mut symbols, labels): (Vec<_>, Vec<_>) = std::mem::take(&mut capture.symbols)
        .into_iter()
        .partition(|symbol| matches!(symbol.kind, debug198x::SymbolKind::Const { .. }));
    symbols.sort_by(|a, b| a.name.cmp(&b.name));
    symbols.extend(labels);
    capture.symbols = symbols;
    let mut result = AssemblyResult::image_warned(assembly.bytes, warnings);
    result.section_debug = capture.section_debug();
    result.areas = area_usage(layout, resolved, &linked.lengths);
    (result, capture)
}

/// Whether an engine section has a label, or, if `lines`, a line.
fn holds_something(record: &SectionRecord, lines: bool) -> bool {
    let labels = record.debug.symbols.iter().any(|symbol| {
        !symbol.name.starts_with(':') && !matches!(symbol.kind, debug198x::SymbolKind::Const { .. })
    });
    labels || (lines && !record.debug.lines.is_empty())
}

/// Adds the constants an engine section's statements defined, which belong
/// to no section.
fn add_constants(capture: &mut DebugCaptureMulti, record: &SectionRecord) {
    capture.symbols.extend(
        record
            .debug
            .symbols
            .iter()
            .filter(|symbol| matches!(symbol.kind, debug198x::SymbolKind::Const { .. }))
            .cloned(),
    );
}

/// The first line of a section the image has no place for that made a byte
/// other than zero.
fn first_data(record: &SectionRecord) -> Option<&crate::engine::LineRec> {
    record.debug.lines.iter().find(|line| {
        let start = line.offset as usize;
        let end = start + line.length as usize;
        record
            .discarded
            .get(start..end)
            .is_some_and(|bytes| bytes.iter().any(|&b| b != 0))
    })
}

/// The debug section for segment number `id`, with its records' addresses
/// counting from `base`. A segment the CPU does not see, such as the iNES
/// header, has no base.
fn section(id: usize, seg: &ResolvedSeg, base: u64) -> debug198x::Section {
    debug198x::Section {
        id: id as debug198x::SectionId,
        name: seg.name.clone(),
        base: seg.cpu_addressable.then_some(base),
        space: None,
    }
}

/// The sections `.org` made, after the layout's own.
struct Spans {
    /// The number the next one takes.
    next: usize,
    /// The last one each segment made: its number, where in the segment it
    /// starts, and its address there.
    last: BTreeMap<usize, (usize, i64, i64)>,
    sections: Vec<debug198x::Section>,
}

impl Spans {
    /// The debug section an engine section's records go in, and how far
    /// into it they start.
    fn locate(
        &mut self,
        id: usize,
        seg: &ResolvedSeg,
        opened: &Opened,
        file_offsets: &mut BTreeMap<debug198x::SectionId, u64>,
    ) -> (usize, i64) {
        let physical = opened.offset;
        let logical = opened.org.unwrap_or(i64::from(seg.base) + physical);
        if logical == i64::from(seg.base) + physical {
            return (id, physical);
        }
        // An address that carries on from the last `.org` in this segment
        // stays in its section.
        if let Some(&(span, start, base)) = self.last.get(&id)
            && logical - base == physical - start
        {
            return (span, physical - start);
        }
        let span = self.next;
        self.next += 1;
        self.sections.push(section(span, seg, logical as u64));
        if let Some(at) = seg.file_at {
            file_offsets.insert(span as debug198x::SectionId, at as u64 + physical as u64);
        }
        self.last.insert(id, (span, physical, logical));
        (span, 0)
    }
}

/// Adds one engine section's records to `capture`, in debug section `id`,
/// `delta` into it, with its lines if `lines` says so.
fn add(
    capture: &mut DebugCaptureMulti,
    record: &SectionRecord,
    id: usize,
    delta: i64,
    lines: bool,
) {
    let id = id as debug198x::SectionId;
    let moved = |offset: u64| (offset as i64 + delta) as u64;
    let DebugData {
        symbols,
        lines: spans,
        cycles,
        ..
    } = &record.debug;
    for symbol in symbols {
        // An unnamed label has no name a person could look up.
        if symbol.name.starts_with(':') {
            continue;
        }
        let mut symbol = symbol.clone();
        if let debug198x::SymbolKind::Label {
            section, offset, ..
        }
        | debug198x::SymbolKind::Entry {
            section, offset, ..
        } = &mut symbol.kind
        {
            *section = id;
            *offset = moved(*offset);
        }
        capture.symbols.push(symbol);
    }
    if !lines {
        return;
    }
    for line in spans {
        capture
            .lines
            .push((line.file, line.line, id, moved(line.offset), line.length));
    }
    for cycle in cycles {
        let mut cycle = *cycle;
        cycle.offset = moved(cycle.offset);
        capture.cycles.push((id, cycle));
    }
}
