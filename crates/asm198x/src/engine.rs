//! The dialect-agnostic assembler engine.
//!
//! Everything here is independent of any one CPU or source dialect: the
//! two-pass driver, the symbol table, expression evaluation, the directive
//! semantics (origin, bytes, words), and byte emission. A [`Dialect`]
//! front-end parses source into the generic [`Statement`] stream this engine
//! consumes — resolving each instruction's addressing mode against its target
//! [`isa::InstructionSet`] at parse time, so instruction *size* never depends
//! on a (possibly forward) symbol value. The engine then lays bytes down using
//! only the shared spec. CPU/dialect knowledge stays in the front-end — see
//! [`crate::dialect`] and [`crate::dialects`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dialect::{Dialect, Oversize};
use crate::source::{SourceLoader, SourceMap};
use crate::span::{FileId, Span};

// The front-end/back-end boundary, re-exported so `crate::engine::*` paths keep
// working while the dialects still live in this crate.
pub use asm198x_ir::diagnostic::{AsmError, Warning, WarningKind};
pub use asm198x_ir::stream::{
    ArtifactFormat, BinOp, DataRange, DeviceSpec, DiagSeverity, Expr, FIX_BITS, NexCommand,
    Operation, OutputFormat, Piece, Place, RequestedOutput, Statement, TapeKind, align_pad,
    encoded_pieces, form, next_pc,
};

/// A file a source asked for **besides** the machine code — sjasmplus's
/// `SAVEBIN` and the container-writing directives after it.
///
/// The library describes one; it never writes it. A library call that touched
/// the filesystem because of a string in its input would make "assemble this
/// and see whether it parses" a filesystem operation, which no consumer can
/// defend against. See `decisions/multi-artifact-output.md`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Artifact {
    /// The name exactly as the source wrote it. Resolving it against an output
    /// directory, a prefix or anything else is the host's business.
    pub name: String,
    /// What the bytes are.
    pub format: ArtifactFormat,
    /// The bytes to write.
    pub bytes: Vec<u8>,
}

/// The result of a successful assembly: where it loads and the bytes to load.
#[derive(Debug, Clone)]
pub struct Assembly {
    /// Load address (first origin directive, or the lowest written address for
    /// a memory-image dialect). On a dialect
    /// whose toolchain reserves rather than materialises, a leading gap moves
    /// this past the unwritten region — see `reserved_prefix`.
    pub origin: u16,
    /// Address units of leading `org` gap or reservation dropped from the front
    /// of the image, so `origin` is that far above where the source's own
    /// addresses start (#90). Also keeps wrapped labels below the lowest
    /// emitted address representable without adding bytes to the image.
    ///
    /// Debug offsets are relative to the source's origin — `origin -
    /// reserved_prefix` — not to `origin`, because they are captured before the
    /// trim. A consumer splitting them into sections uses this as the boundary.
    pub reserved_prefix: u16,
    /// Assembled machine code, contiguous from `origin`.
    pub bytes: Vec<u8>,
    /// Memory-area accounting (#499) — see [`AreaUsage`].
    pub areas: Vec<AreaUsage>,
    /// The output file the source named with `!to`, if it named one. The
    /// command line still wins — this is what ACME does, and it warns rather
    /// than obeying the second choice.
    pub requested_output: Option<RequestedOutput>,
    /// The symbol-list file the source named with `!symbollist`, same rule.
    pub requested_symbols: Option<String>,
    /// Files the source asked for besides the machine code, in the order it
    /// asked. Empty for a source that asked for none, which is most of them.
    pub artifacts: Vec<Artifact>,
    /// Resolved labels, for listings and debugging. Values are `i64` to hold
    /// the 65816's 24-bit addresses and bank constants; 8-/16-bit CPUs use the
    /// low bits only.
    pub symbols: BTreeMap<String, i64>,
    /// The program's entry point, if an `end <addr>` directive gave one. Used by
    /// containers that carry a start address (a Spectrum `.sna`); `None` for a
    /// plain flat binary.
    pub start: Option<u16>,
    /// Non-fatal advisories raised during assembly (e.g. a byte truncated to fit
    /// its operand, sjasmplus-style). Empty for dialects that don't warn.
    pub warnings: Vec<Warning>,
    /// Debug-info captured during pass 2 — the line→address map and typed
    /// symbols the CLI renders into a `.debug198x` sidecar / `--sym` / `--listing`.
    /// Header-less (the CPU/dialect/source-file identity is the CLI's to add) and
    /// section-less (the flat engine is a single implicit section 0, based at
    /// `origin`). Capturing it never changes an emitted byte.
    pub debug: DebugData,
}

/// The engine's slice of a `.debug198x` record: typed symbols and line→address
/// spans, in the CPU's **address units** (a decle for the word-addressed CP1610,
/// a byte elsewhere) so a consumer's address lookups line up with the CPU's own
/// addressing. Header-less; the CLI wraps it with identity and the source
/// filename to form a full [`debug198x::DebugInfo`].
// No `Eq`: `debug198x::Symbol` may carry a `Space::Unknown` holding arbitrary
// JSON, which is `PartialEq` but not `Eq`. Nothing compares these as map keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DebugData {
    /// Every label (address), `equ`/`=` constant (value), and entry point.
    pub symbols: Vec<debug198x::Symbol>,
    /// Device placement at each address symbol's definition, keyed by the
    /// same name as `symbols`. Constants have no placement. This is captured
    /// before later PAGE/SLOT/DEVICE directives can change the mapping; absent
    /// means unknown, not page zero. Older payloads and flat assemblies omit it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub symbol_pages: BTreeMap<String, PagedLocation>,
    /// Bank of an address symbol in a banked section (currently RGBASM).
    /// Interpret together with the symbol's CPU address: ROM and RAM banks
    /// are separate spaces. This is not a linear physical address.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub symbol_banks: BTreeMap<String, u32>,
    /// One span per source-bearing statement that emitted bytes. Fill from `org`
    /// gaps and `align` carries no span (the padding rule).
    pub lines: Vec<LineRec>,
    /// One spec-sourced timing record per emitted instruction (#497), in
    /// emission order. Empty when nothing captured — a data-only program, or a
    /// path whose instructions carry no spec cycles (see
    /// [`DebugData::cycle_coverage`]). Additive: absent from older payloads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cycles: Vec<CycleRec>,
    /// What an empty `cycles` means — see [`CycleCoverage`]. Additive:
    /// omitted (as `none`) from payloads that predate the capture.
    #[serde(default, skip_serializing_if = "CycleCoverage::is_none")]
    pub cycle_coverage: CycleCoverage,
}

/// An address's actual placement in a paged device. The page size comes from
/// the selected device, never from an assumed CPU or exporter convention.
/// `page` is the durable identity; `slot` is the expected mapping at assembly
/// time. The offset is within the page, not within the emitted binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PagedLocation {
    /// Slot containing the logical address, not the slot selected by SLOT.
    pub slot: u8,
    /// Physical page mapped into that slot at the definition.
    pub page: u16,
    /// Size of a page (and slot), in bytes.
    pub page_size: u32,
    /// Byte offset within the physical page.
    pub offset: u32,
}

impl PagedLocation {
    /// The CPU address under the mapping used at the definition.
    #[must_use]
    pub fn logical_address(self) -> u64 {
        u64::from(self.slot) * u64::from(self.page_size) + u64::from(self.offset)
    }

    /// The linear address in device memory, independent of the mapped slot.
    #[must_use]
    pub fn physical_address(self) -> u64 {
        u64::from(self.page) * u64::from(self.page_size) + u64::from(self.offset)
    }
}

/// A line→address span before the source filename is attached: `length` address
/// units at section-relative `offset` were produced by `line`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LineRec {
    pub line: u32,
    pub offset: u64,
    pub length: u64,
    /// The file `line` counts within (language-surface U2): the root input for
    /// a single-file assemble, an include's `FileId` otherwise. Additive per
    /// KTD7 — absent means the root, and a root value is not serialized, so
    /// pre-multi-file payloads are byte-identical. U9 renders these; the
    /// engine just carries the data.
    #[serde(default, skip_serializing_if = "FileId::is_root")]
    pub file: FileId,
}

/// What an empty cycle capture means for this assembly (#497): the dialect's
/// declaration, carried on the result so a renderer — or a JSON consumer —
/// can tell "data only" from "instructions whose spec has no cycles yet"
/// (#498) without knowing the dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CycleCoverage {
    /// Every instruction has timing metadata: an absent record means data.
    /// Complete metadata may still describe an unbounded wait.
    Full,
    /// Some instructions pre-encode into pieces and capture nothing; an
    /// absent record is not proof of data, and totals are lower bounds.
    Partial,
    /// No cycle capture on this path — the spec carries no cycle data for
    /// the CPU (#498), or the driver does not capture yet.
    #[default]
    None,
}

impl CycleCoverage {
    fn is_none(&self) -> bool {
        matches!(self, CycleCoverage::None)
    }
}

/// One memory area's account (#499): capacity, occupancy, and the free
/// remainder, measured against the active layout. For a flat dialect the
/// whole address space is the one area; the ca65 path reports each layout
/// area with the segments placed into it. All figures are address units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaUsage {
    pub name: String,
    pub start: u32,
    pub size: u32,
    /// Units occupied by placed or reserved content.
    pub used: u32,
    /// `size - used` — the budgetable number (`free(<area>) >= N`).
    pub free: u32,
    /// The largest contiguous free run, which is what a new routine or table
    /// actually needs; a pinned segment (VECTORS at $FFFA) can split the free
    /// space so `free` overstates what fits in one piece.
    pub largest_free: u32,
    /// The segments placed in this area, in address order. Empty for the flat
    /// single-space row, whose occupancy is the program itself.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<SegmentUsage>,
}

/// One segment's placement inside an [`AreaUsage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentUsage {
    pub name: String,
    pub base: u32,
    pub length: u32,
}

/// Nominal processor-cycle bounds, excluding external stalls and called code.
/// `max: None` denotes a documented unbounded wait, not missing metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CycleBounds {
    /// Minimum nominal cost.
    pub min: u8,
    /// Finite maximum, or `None` when the instruction may wait indefinitely.
    pub max: Option<u8>,
}

/// Spec-sourced timing for one emitted instruction (#497): the `isa::Cycles`
/// triple, copied at the moment pass 2 resolved the form, plus the same
/// line/offset attribution a [`LineRec`] carries. Data emissions get no
/// record — absence means "nothing executes here", which a zero would
/// misstate. The honest range derives as `base ..= base + page_cross +
/// branch_taken` unless `bounds` is present. Computed instructions supply
/// explicit bounds, including runtime-dependent costs and unbounded waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CycleRec {
    /// 1-based source line, in `file`.
    pub line: u32,
    /// Section-relative offset in address units, exactly as [`LineRec::offset`].
    pub offset: u64,
    /// The spec's base cost.
    pub base: u8,
    /// Extra when an indexed access crosses a page boundary (0 when fixed).
    pub page_cross: u8,
    /// Extra when a branch is taken (0 for non-branches).
    pub branch_taken: u8,
    /// Authoritative bounds for computed instructions. When present, use
    /// these instead of deriving a range from the legacy form triple.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<CycleBounds>,
    /// The file `line` counts within, as [`LineRec::file`].
    #[serde(default, skip_serializing_if = "FileId::is_root")]
    pub file: FileId,
}

impl CycleRec {
    /// Nominal bounds, resolving explicit computed timing or the form triple.
    #[must_use]
    pub fn range(&self) -> (u64, Option<u64>) {
        self.bounds.map_or_else(
            || {
                (
                    u64::from(self.base),
                    Some(
                        u64::from(self.base)
                            + u64::from(self.page_cross)
                            + u64::from(self.branch_taken),
                    ),
                )
            },
            |b| (u64::from(b.min), b.max.map(u64::from)),
        )
    }
}

// ---------------------------------------------------------------------------
// The two-pass driver
// ---------------------------------------------------------------------------

/// Assemble `source` with `dialect` into a flat binary.
///
/// Two passes: pass one assigns addresses to labels; pass two emits bytes with
/// labels resolved. The dialect has already resolved each instruction's mode,
/// so form sizes are stable between the passes.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub(crate) fn assemble(source: &str, dialect: &dyn Dialect) -> Result<Assembly, AsmError> {
    let (statements, warnings) = dialect.parse_warned(source)?;
    let a = assemble_statements(statements, warnings, dialect)?;
    // Budget assertions (#497) hold wherever assembly happens, so they run
    // here rather than in any one front-end.
    crate::cycles::check_cycle_budgets(&[("input", source)], &a.debug, &a.areas)?;
    Ok(a)
}

/// Assemble a multi-file program (language-surface U2): the root is
/// `FileId(0)` in `map`, and the dialect's include-capable parse resolves
/// `INCLUDE` directives through `loader`, minting further ids as it goes. The
/// two-pass driver below is the same one [`assemble`] uses — only the parse
/// differs.
///
/// # Errors
/// As [`assemble`]; errors raised inside an included file carry that file's
/// `FileId` in their span.
pub(crate) fn assemble_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    dialect: &dyn Dialect,
) -> Result<Assembly, AsmError> {
    let (statements, warnings) = dialect.parse_multi_warned(map, loader)?;
    let a = assemble_statements(statements, warnings, dialect)?;
    let sources: Vec<(&str, &str)> = (0..)
        .map_while(|i| {
            let id = crate::span::FileId(i);
            Some((map.path(id)?, map.contents(id)?))
        })
        .collect();
    crate::cycles::check_cycle_budgets(&sources, &a.debug, &a.areas)?;
    Ok(a)
}

/// Lay a program's sections into one image.
///
/// The single placement implementation: the engine's own section path and the
/// dialects whose toolchain builds a container (ca65's NES ROM) both come
/// through here, so "where does a section go" is answered once
/// (`decisions/sections-in-the-shared-engine.md`).
///
/// Sections are placed **by image position, not by source order** — a section
/// may sit below one written before it, which is what an origin could never
/// express. A section's position is its own `at` where it has one, since a
/// banked or config-placed section is addressed somewhere the image does not
/// put it, and otherwise its address.
///
/// `image_base` fixes where the image starts when the container does rather
/// than the program; `image_size` pads it out. Both are `None`/no-op for a
/// dialect whose image is just what the source wrote.
pub(crate) fn lay_out(
    mut runs: Vec<Run>,
    gap_fill: u8,
    addr_unit: i64,
    image_base: Option<i64>,
    later_overwrites: bool,
    image_size: impl Fn(&[u8]) -> Option<usize>,
) -> Result<(i64, Vec<u8>), AsmError> {
    runs.retain(|r| !r.bytes.is_empty() && r.at != Place::Discard);
    if runs.is_empty() {
        return Ok((image_base.unwrap_or(0), Vec::new()));
    }
    // A `Next` section belongs immediately after the one before it, whatever
    // either is addressed at, so fold it in before anything is placed. What is
    // left all has a place of its own and sorts by it.
    let mut merged: Vec<Run> = Vec::new();
    for r in runs {
        match (r.at, merged.last_mut()) {
            (Place::Next, Some(prev)) => prev.bytes.extend_from_slice(&r.bytes),
            _ => merged.push(r),
        }
    }
    let mut runs = merged;

    let placed = |r: &Run| match r.at {
        Place::At(n) => n,
        Place::Discard => unreachable!("discarded runs were removed above"),
        _ => r.base,
    };
    if later_overwrites {
        let first = image_base.unwrap_or_else(|| runs.iter().map(placed).min().unwrap_or(0));
        let end = runs
            .iter()
            .map(|r| placed(r) + r.bytes.len() as i64 / addr_unit)
            .max()
            .unwrap_or(first);
        let mut image = vec![gap_fill; ((end - first) * addr_unit) as usize];
        // Deliberately retain source order: ACME's later segment wins where
        // two address ranges overlap.
        for r in &runs {
            let at = ((placed(r) - first) * addr_unit) as usize;
            image[at..at + r.bytes.len()].copy_from_slice(&r.bytes);
        }
        if let Some(size) = image_size(&image) {
            image.resize(size, gap_fill);
        }
        return Ok((image_base.unwrap_or(first), image));
    }

    runs.sort_by_key(placed);
    let origin = image_base.unwrap_or(runs[0].base);
    let first = image_base.unwrap_or_else(|| placed(&runs[0]));
    let mut image: Vec<u8> = Vec::new();
    for r in &runs {
        let at = ((placed(r) - first) * addr_unit) as usize;
        if at < image.len() {
            return Err(AsmError::new(
                0,
                format!(
                    "section `{}` at {:#06x} overlaps the section before it",
                    r.name,
                    placed(r)
                ),
            ));
        }
        image.resize(at, gap_fill);
        image.extend_from_slice(&r.bytes);
    }
    if let Some(size) = image_size(&image) {
        image.resize(size, gap_fill);
    }
    Ok((origin, image))
}

/// One section's placed bytes, closed off when the next section opens.
///
/// The engine's whole section model: a program is a list of these, and a
/// dialect with no section concept produces exactly one. Sections are internal
/// — they are laid into the flat [`Assembly`] before anything outside the
/// engine sees them (`decisions/sections-in-the-shared-engine.md`).
pub(crate) struct Run {
    pub(crate) name: String,
    /// The address the CPU sees these bytes at — what labels resolve to.
    pub(crate) base: i64,
    /// Where the bytes sit in the image, when that differs from `base`.
    pub(crate) at: Place,
    pub(crate) bytes: Vec<u8>,
}

// No written-range fields here: the reserve-rather-than-materialise trim
// (`Dialect::trims_trailing_gap`) belongs to the asl family, and no asl dialect
// has sections. A sectioned dialect that trimmed would need the range per
// section, and this is where it would go.

/// Reuse the ordinary encoder and device replay for a Lua read at a source
/// boundary. Forward values come from the previous SjASMPlus pass.
#[cfg(feature = "lua")]
pub(crate) fn lua_memory_snapshot(
    statements: &[Statement],
    seed: &BTreeMap<String, i64>,
    z80n: bool,
) -> Result<Vec<u8>, AsmError> {
    let mut prefix = Vec::new();
    for (name, value) in seed {
        if !statements.iter().any(|s| s.label.as_ref() == Some(name)) {
            prefix.push(Statement {
                line: 0,
                file: FileId(0),
                label: Some(name.clone()),
                op: Some(Operation::Equ(Expr::Num(*value))),
                operand_span: None,
                xor_mask: 0,
                instruction_set: None,
                extension_set: None,
            });
        }
    }
    prefix.extend(
        statements
            .iter()
            .filter(|s| {
                !matches!(
                    s.op,
                    Some(
                        Operation::SaveRaw { .. }
                            | Operation::SaveNex(_)
                            | Operation::SaveTape { .. }
                            | Operation::SaveCpr { .. }
                            | Operation::SymbolMap { .. }
                    )
                )
            })
            .cloned(),
    );
    prefix.push(Statement {
        line: 0,
        file: FileId(0),
        label: None,
        op: Some(Operation::SaveRaw {
            name: "lua-memory".into(),
            start: Expr::Num(0),
            length: Some(Expr::Num(65536)),
        }),
        operand_span: None,
        xor_mask: 0,
        instruction_set: None,
        extension_set: None,
    });
    let result = assemble_statements(prefix, Vec::new(), &crate::dialects::Sjasmplus { z80n })?;
    result
        .artifacts
        .into_iter()
        .last()
        .map(|a| a.bytes)
        .ok_or_else(|| AsmError::new(0, "Lua device memory was not captured"))
}

fn address_debug_base(debug: &DebugData, image_origin: u64) -> u64 {
    debug
        .symbols
        .iter()
        .filter_map(|symbol| match symbol.kind {
            debug198x::SymbolKind::Label { offset, .. }
            | debug198x::SymbolKind::Entry { offset, .. } => Some(offset),
            _ => None,
        })
        .chain(debug.lines.iter().map(|line| line.offset))
        .chain(debug.cycles.iter().map(|cycle| cycle.offset))
        .fold(image_origin, u64::min)
}

/// The shared two-pass driver over an already-parsed statement stream — the
/// single body behind [`assemble`] and [`assemble_multi`], so the single- and
/// multi-file paths cannot drift.
fn assemble_statements(
    mut statements: Vec<Statement>,
    parse_warnings: Vec<Warning>,
    dialect: &dyn Dialect,
) -> Result<Assembly, AsmError> {
    let default_set = dialect.instruction_set();
    let default_ext = dialect.extension_set();
    let cycle_coverage = dialect.cycle_coverage();

    // Pass 1 — assign addresses to labels.
    let require_origin = dialect.requires_explicit_origin();
    let org_moves_output = dialect.org_moves_output();
    let org_starts_address_run = dialect.org_starts_address_run();
    let wraps_address_space = dialect.wraps_address_space();
    let org_drops_unwritten_prefix = dialect.org_drops_unwritten_prefix();
    // Emitted bytes per address unit — 1 for the byte-addressed CPUs, 2 for the
    // word-addressed CP1610 (a decle is two bytes). The location counter advances
    // in address units, so a byte length is divided by this.
    let addr_unit = dialect.addr_unit();
    let mut symbols: BTreeMap<String, i64> = BTreeMap::new();
    // The names a redefinable binding owns, so a second one is allowed and a
    // label or `equ` over the top of one is not.
    let mut redefinable: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pc: i64 = 0;
    let mut nex_end = None;
    // ACME's `!pseudopc`: what to add to the real counter to get the address
    // the code claims. Zero outside any such block; a stack, because they
    // nest and each restores the one it opened inside.
    let mut pseudo: i64 = 0;
    let mut pseudo_stack: Vec<i64> = Vec::new();
    // Byte offset of the current OS-9 module. A file may contain successive
    // modules; each trailer covers its own module, not everything before it.
    let mut os9_module_start: Option<usize> = None;
    let mut origin: Option<i64> = None;
    for s in &mut statements {
        if let Some(Operation::Entry(expr)) = &s.op {
            nex_end = Some((expr.clone(), pc, s.line, s.file));
        }

        let (set, ext) = match s.instruction_set {
            Some(set) => (set, s.extension_set),
            None => (default_set, default_ext),
        };
        // `equ` binds the label to a value, not the current address, and emits
        // nothing — so it is handled before the address-label assignment below.
        if let Some(Operation::Equ(e)) = &s.op {
            let label = s
                .label
                .as_ref()
                .ok_or_else(|| s.err("`equ` needs a label"))?;
            let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
            if let Some(range) = dialect.equ_range()
                && !range.contains(&v)
            {
                return Err(s.err(format!(
                    "equ value {v} out of range {}..={}",
                    range.start(),
                    range.end()
                )));
            }
            if symbols.insert(label.clone(), v).is_some() || redefinable.contains(label) {
                return Err(s.err(format!("duplicate label `{label}`")));
            }
            continue;
        }
        // A redefinable binding (`set`). It may bind the same name again, so
        // long as the name is one this directive owns — a name already fixed
        // by a label or an `equ` is not, and lwasm calls that "Multiply
        // defined symbol" whichever of the two came first.
        if let Some(Operation::Set(e)) = &s.op {
            let label = s
                .label
                .as_ref()
                .ok_or_else(|| s.err("`set` needs a label"))?;
            let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
            if symbols.insert(label.clone(), v).is_some() && !redefinable.contains(label) {
                return Err(s.err(format!("duplicate label `{label}`")));
            }
            redefinable.insert(label.clone());
            continue;
        }
        if let Some(Operation::DefineSymbols(definitions)) = &s.op {
            for (name, value) in definitions {
                if let Some(range) = dialect.equ_range()
                    && !range.contains(value)
                {
                    return Err(s.err(format!(
                        "enum value {value} out of range {}..={}",
                        range.start(),
                        range.end()
                    )));
                }
                if symbols.insert(name.clone(), *value).is_some() || redefinable.contains(name) {
                    return Err(s.err(format!("duplicate label `{name}`")));
                }
            }
        }
        if let Some(label) = &s.label {
            if !(0..=0xFF_FFFF).contains(&pc) {
                return Err(s.err("address out of range"));
            }
            // Inside a `!pseudopc` block a label reads as the address the
            // code will run at, not the one it is being assembled at.
            if symbols.insert(label.clone(), pc + pseudo).is_some() || redefinable.contains(label) {
                return Err(s.err(format!("duplicate label `{label}`")));
            }
        }
        if let Some(Operation::DirectPage {
            direct,
            extended,
            expr,
            dp,
        }) = &s.op
        {
            let is_direct = expr.eval(&symbols, pc + pseudo, s.line).is_ok_and(|value| {
                (0..=0xFFFF).contains(&value) && ((value >> 8) & 0xFF) == i64::from(*dp)
            });
            let opcode = if is_direct { direct } else { extended };
            let width = if is_direct { 1 } else { 2 };
            let value = if is_direct {
                Expr::Lo(Box::new(expr.clone()))
            } else {
                expr.clone()
            };
            s.op = Some(Operation::ComputedInstruction(encoded_pieces(
                opcode, value, width,
            )));
        }
        match &s.op {
            None => {}
            Some(Operation::Os9Module { .. }) => {
                // `mod` carries the implicit `org 0` which makes every header
                // field and label module-relative.
                pc = 0;
                pseudo = 0;
                origin = Some(0);
                pc = next_pc(
                    s.op.as_ref().expect("matched operation"),
                    pc,
                    set,
                    ext,
                    addr_unit,
                    s.line,
                )?;
            }
            Some(Operation::Org(e)) => {
                let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                if !(0..=0xFFFF).contains(&v) {
                    return Err(s.err("origin address out of range"));
                }
                if origin.is_some() && !org_moves_output {
                    // The address moves and the output does not, so the move
                    // is an offset from the real counter — the same thing a
                    // relocated block holds, and it outlives the statement.
                    pseudo = v - pc;
                } else {
                    pc = v;
                    origin.get_or_insert(v);
                }
            }
            Some(Operation::Section {
                base: Some(base), ..
            }) => {
                pc = *base;
                origin.get_or_insert(*base);
            }
            // A section whose address its linker chooses: nothing to move to.
            Some(Operation::Section { base: None, .. }) => {}
            Some(
                Operation::Bytes(_)
                | Operation::Words(_)
                | Operation::Instruction { .. }
                | Operation::Encoded(_)
                | Operation::ComputedInstruction(_)
                | Operation::DirectPage { .. }
                | Operation::Binary(_)
                | Operation::Fill { .. }
                | Operation::Align { .. }
                | Operation::AlignTo { .. },
            ) if require_origin && origin.is_none() => {
                return Err(s.err(
                    "program counter undefined — set an origin (`*=`) before any code or data",
                ));
            }
            Some(Operation::PseudoPc(target)) => match target {
                Some(e) => {
                    let at = e
                        .eval(&symbols, pc + pseudo, s.line)
                        .map_err(|err| s.stamp(err))?;
                    pseudo_stack.push(pseudo);
                    pseudo = at - pc;
                }
                None => {
                    pseudo = pseudo_stack
                        .pop()
                        .ok_or_else(|| s.err("internal: `}` closed no `!pseudopc` block"))?;
                }
            },
            Some(op) => {
                pc = next_pc(op, pc, set, ext, addr_unit, s.line).map_err(|err| s.stamp(err))?;
            }
        }
        if wraps_address_space {
            pc &= 0xffff;
        }
    }
    let mut origin = origin.unwrap_or(0);
    let has_nex = statements
        .iter()
        .any(|s| matches!(s.op, Some(Operation::SaveNex(_))));
    let nex_start = if has_nex {
        nex_end
            .map(|(expr, pc, line, file)| {
                expr.eval(&symbols, pc, line)
                    .map(|v| v as u16)
                    .map_err(|err| AsmError::at(Span::in_file(file, line as u32, 0), err.message))
            })
            .transpose()?
    } else {
        None
    };

    // Pass 2 — emit.
    let byte_policy = dialect.oversized_byte_policy();
    let operand_policy = dialect.oversized_operand_policy();
    // One answer per dialect, used at every width — see `accepts_negative_values`.
    let floor = |bytes: u32| -> i64 {
        if dialect.accepts_negative_values() {
            -(1i64 << (bytes * 8 - 1))
        } else {
            0
        }
    };
    let gap_fill = dialect.gap_fill();
    // The image length as of the last operation that actually *wrote* data.
    // Reservations and `org` gaps advance the counter without contributing, so
    // on a dialect whose toolchain reserves rather than materialises, anything
    // past this point is trimmed (see `Dialect::trims_trailing_gap`).
    let mut written_len = 0usize;
    // Where the first written byte landed. Anything before it is `org` gap or
    // reservation, which `p2bin` does not put in the image at all — it starts
    // the file at the lowest *written* address, so a leading gap shifts the
    // load address instead of padding it.
    let mut written_start: Option<usize> = None;
    let mut wrote_any = false;
    // The parse's advisories come first: they describe the source, and the
    // layout's describe what the source turned into.
    let mut warnings: Vec<Warning> = parse_warnings;

    // `!initmem` is assembly-wide and position-independent: a reservation
    // *earlier* in the file still takes its value, so it is read here rather
    // than applied as the walk reaches it. The first one wins; ACME answers a
    // second with "Memory already initialised" and keeps the first.
    let mut gap_fill = gap_fill;
    let mut seen_init = false;
    // The source naming its own files. ACME takes the **first** and warns
    // that the name was "already chosen" for any later one — including when
    // the command line already chose, which is why these are only a request.
    let mut requested_output: Option<RequestedOutput> = None;
    let mut requested_symbols: Option<String> = None;
    // Source-requested files, captured and retained in source order.
    let mut artifacts: Vec<Artifact> = Vec::new();
    let mut nex_session: Option<NexSession> = None;
    let mut device_memory: Option<DeviceMemory> = None;
    let mut inactive_devices: BTreeMap<String, DeviceMemory> = BTreeMap::new();
    let mut export_symbols = BTreeMap::new();
    let mut symbol_maps: Vec<(String, ArtifactFormat, bool, u32)> = Vec::new();
    for s in &statements {
        if let Some(Operation::InitMem(v)) = &s.op {
            if seen_init {
                warnings.push(
                    Warning::new(
                        s.line,
                        "memory already initialised; the first value stands".to_string(),
                    )
                    .in_file(s.file),
                );
            } else {
                gap_fill = *v;
                seen_init = true;
            }
        }
        match &s.op {
            Some(Operation::RequestOutput {
                path,
                format,
                defaulted_format,
            }) => {
                if *defaulted_format && requested_output.is_none() {
                    warnings.push(
                        Warning::new(
                            s.line,
                            "`!to` names no file format; defaulting to `cbm`".to_string(),
                        )
                        .in_file(s.file),
                    );
                }
                if requested_output.is_some() {
                    warnings.push(
                        Warning::new(
                            s.line,
                            "output file already chosen; the first name stands".to_string(),
                        )
                        .in_file(s.file),
                    );
                } else {
                    requested_output = Some(RequestedOutput {
                        path: path.clone(),
                        format: *format,
                    });
                }
            }
            Some(Operation::RequestSymbols { path }) => {
                if requested_symbols.is_some() {
                    warnings.push(
                        Warning::new(
                            s.line,
                            "symbol list file name already chosen; the first name stands"
                                .to_string(),
                        )
                        .in_file(s.file),
                    );
                } else {
                    requested_symbols = Some(path.clone());
                }
            }
            _ => {}
        }
    }
    let mut start: Option<u16> = None;
    let mut bytes: Vec<u8> = Vec::new();
    // Sections closed so far. A dialect with no section concept never opens
    // one, leaves this empty, and takes the single-run path below — which is
    // the flat engine unchanged.
    let mut runs: Vec<Run> = Vec::new();
    let mut section_name = String::new();
    let mut section_bank = None;
    let mut section_at = Place::ByAddress;
    let mut debug = DebugData {
        cycle_coverage,
        ..DebugData::default()
    };
    // The second counter again, rebuilt for pass 2 — the statements are the
    // same, so it retraces the same path.
    let mut pseudo: i64 = 0;
    let mut pseudo_stack: Vec<i64> = Vec::new();
    for s in &statements {
        let (set, ext) = match s.instruction_set {
            Some(set) => (set, s.extension_set),
            None => (default_set, default_ext),
        };
        // The location counter (`$`) is the address of this statement's start,
        // in address units (bytes divided by `addr_unit`). Inside a
        // `!pseudopc` block it reads as the address the code will run at.
        let real_pc = origin + bytes.len() as i64 / addr_unit;
        let pc = if wraps_address_space {
            (real_pc & 0xffff) + pseudo
        } else {
            real_pc + pseudo
        };
        // A label on a mapping directive belongs to the mapping before the
        // directive, just as its address is the pre-directive location counter.
        let label_page = device_memory.as_ref().and_then(|m| m.location(pc));
        let len_before = bytes.len();
        // The cycle triple pass 2 resolves for this statement, if it is a
        // form-model instruction; consumed at the LineRec push below so the
        // record shares the span's exact line/offset attribution (#497).
        let mut cycle_capture: Option<isa::Cycles> = None;
        match &s.op {
            None => {}
            Some(Operation::Org(e)) => {
                let target = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                let cur = origin + bytes.len() as i64 / addr_unit;
                if !org_moves_output {
                    // See pass one: the address moves, the output does not.
                    if org_drops_unwritten_prefix && !wrote_any && target < cur {
                        bytes.clear();
                        origin = target;
                        pseudo = 0;
                        written_len = 0;
                    } else {
                        pseudo = target - cur;
                    }
                } else {
                    if org_starts_address_run && target != cur {
                        let overlaps = runs.iter().any(|r| {
                            let end = r.base + r.bytes.len() as i64 / addr_unit;
                            target >= r.base && target < end
                        }) || (target >= origin && target < cur);
                        if overlaps && !wraps_address_space {
                            warnings.push(
                                Warning::new(
                                    s.line,
                                    "segment starts inside another one; later bytes overwrite it"
                                        .to_string(),
                                )
                                .in_file(s.file),
                            );
                        }
                        runs.push(Run {
                            name: format!("origin ${origin:04x}"),
                            base: origin,
                            at: Place::ByAddress,
                            bytes: std::mem::take(&mut bytes),
                        });
                        origin = target;
                        written_len = 0;
                        written_start = None;
                        wrote_any = false;
                        continue;
                    }
                    if target < cur {
                        return Err(s.err("cannot move origin backwards"));
                    }
                    bytes.resize(
                        bytes.len() + ((target - cur) * addr_unit) as usize,
                        gap_fill,
                    );
                }
            }
            Some(Operation::Section {
                name,
                base,
                at,
                bank,
            }) => {
                // Close the run so far and start one at the new base. Bytes
                // are per-section from here, so the location counter, the
                // written-range trims and the 64K check all measure within
                // this section rather than across the image.
                if base.is_some() || *at == Place::Next {
                    runs.push(Run {
                        name: std::mem::take(&mut section_name),
                        base: origin,
                        at: std::mem::replace(&mut section_at, Place::ByAddress),
                        bytes: std::mem::take(&mut bytes),
                    });
                    if let Some(base) = base {
                        origin = *base;
                    }
                    written_len = 0;
                    written_start = None;
                    wrote_any = false;
                }
                section_name = name.clone();
                section_at = *at;
                section_bank = *bank;
            }
            Some(Operation::Device(spec)) => {
                if let Some(memory) = device_memory.take() {
                    inactive_devices.insert(memory.spec.name.clone(), memory);
                }
                device_memory = spec.as_ref().map(|spec| {
                    inactive_devices
                        .remove(&spec.name)
                        .unwrap_or_else(|| DeviceMemory::new(spec.clone()))
                });
            }
            Some(Operation::SymbolMap {
                name,
                format,
                virtual_labels,
            }) => {
                if !matches!(
                    format,
                    ArtifactFormat::CspectMap | ArtifactFormat::LabelsList
                ) {
                    return Err(s.err("invalid symbol map format"));
                }
                let memory = device_memory
                    .as_ref()
                    .ok_or_else(|| s.err("symbol maps require DEVICE emulation mode"))?;
                symbol_maps.retain(|(_, previous, _, _)| previous != format);
                symbol_maps.push((
                    name.clone(),
                    *format,
                    *virtual_labels,
                    memory.spec.slot_size as u32,
                ));
            }
            Some(Operation::StructSymbol(name)) => {
                if let Some(symbol) = export_symbols.get_mut(name) {
                    let symbol: &mut crate::sjasm_symbols::Symbol = symbol;
                    symbol.kind = crate::sjasm_symbols::Kind::Structure;
                }
            }
            Some(Operation::DeviceSlot(slot)) => {
                let slot = slot
                    .eval(&symbols, pc, s.line)
                    .map_err(|err| s.stamp(err))?;
                if let Some(memory) = &mut device_memory {
                    memory
                        .select_slot(slot)
                        .map_err(|err| s.operand_err(err.message))?;
                }
            }
            Some(Operation::DevicePage(page)) => {
                let page = page
                    .eval(&symbols, pc, s.line)
                    .map_err(|err| s.stamp(err))?;
                if let Some(memory) = &mut device_memory {
                    memory
                        .map_page(page)
                        .map_err(|err| s.operand_err(err.message))?;
                }
                // PAGE is also a raw-output section boundary. Its address is
                // the live counter; placement remains source-consecutive.
                runs.push(Run {
                    name: std::mem::take(&mut section_name),
                    base: origin,
                    at: std::mem::replace(&mut section_at, Place::ByAddress),
                    bytes: std::mem::take(&mut bytes),
                });
                origin = real_pc;
                written_len = 0;
                written_start = None;
                wrote_any = false;
                section_name = format!("page {page}");
                section_at = Place::Next;
            }
            Some(Operation::SaveNex(command)) => {
                apply_nex(
                    command,
                    s,
                    device_memory.as_ref(),
                    &symbols,
                    pc,
                    nex_start,
                    &mut nex_session,
                    &mut artifacts,
                    &mut warnings,
                )?;
            }
            Some(Operation::SaveCpr { name, pages }) => {
                let pages = pages
                    .eval(&symbols, pc, s.line)
                    .map_err(|err| s.stamp(err))?;
                let memory = device_memory.as_ref().ok_or_else(|| {
                    s.err("SAVECPR only allowed in real device emulation mode (See DEVICE)")
                })?;
                if memory.spec.name != "AMSTRADCPCPLUS" {
                    return Err(s.err("[SAVECPR] Device must be AMSTRADCPCPLUS."));
                }
                let pages = usize::try_from(pages)
                    .ok()
                    .filter(|n| (1..=32).contains(n))
                    .ok_or_else(|| {
                        s.err("[SAVECPR] only a size from 1 (16KiB) to 32 (512KiB) is allowed")
                    })?;
                artifacts.push(Artifact {
                    name: name.clone(),
                    format: ArtifactFormat::Cpr,
                    bytes: cpr_image(&memory.pages[..pages]),
                });
            }
            Some(Operation::Equ(_) | Operation::DefineSymbols(_)) => {
                // Define symbols; emit nothing.
            }
            // Noted here and answered below: the span it names is against the
            // finished image, which does not exist yet.
            Some(Operation::SaveRaw {
                name,
                start,
                length,
            }) => {
                if device_memory.is_none() {
                    return Err(
                        s.err("SAVEBIN only allowed in real device emulation mode (See DEVICE)")
                    );
                }
                let at = start
                    .eval(&symbols, pc, s.line)
                    .map_err(|err| s.stamp(err))?;
                let len = match length {
                    Some(e) => Some(e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?),
                    None => None,
                };
                let len = len.filter(|len| *len > 0).unwrap_or(0x1_0000 - at);
                let memory = device_memory.as_ref().expect("device checked above");
                artifacts.push(Artifact {
                    name: name.clone(),
                    format: ArtifactFormat::Raw,
                    bytes: memory.read(at, len).map_err(|err| s.stamp(err))?,
                });
            }
            Some(Operation::SaveTape {
                file,
                kind,
                name,
                start,
                length,
            }) => {
                if device_memory.is_none() {
                    return Err(
                        s.err("SAVETAP only allowed in real device emulation mode (See DEVICE)")
                    );
                }
                let at = start
                    .eval(&symbols, pc, s.line)
                    .map_err(|err| s.stamp(err))?;
                let len = length
                    .eval(&symbols, pc, s.line)
                    .map_err(|err| s.stamp(err))?;
                let memory = device_memory.as_ref().expect("device checked above");
                let span = memory.read(at, len).map_err(|err| s.stamp(err))?;
                artifacts.push(Artifact {
                    name: file.clone(),
                    format: ArtifactFormat::Tap,
                    bytes: tape_image(*kind, name, at, span),
                });
            }
            // Rebinds where it stands, so a use below reads this value and a
            // use above kept the one pass one left. Emits nothing either way.
            Some(Operation::Set(e)) => {
                let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                if let Some(label) = &s.label {
                    symbols.insert(label.clone(), v);
                }
            }
            Some(Operation::Entry(e)) => {
                let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                if !(0..=0xFFFF).contains(&v) {
                    return Err(s.err("entry address out of range"));
                }
                start = Some(v as u16);
            }
            Some(Operation::Align {
                andmask,
                value,
                fill,
            }) => {
                let pad = (value - pc) & andmask;
                bytes.extend(std::iter::repeat_n(*fill, pad as usize));
            }
            Some(Operation::AlignTo { modulus, fill }) => {
                let pad = align_pad(pc, *modulus);
                for _ in 0..pad {
                    bytes.extend_from_slice(fill);
                }
            }
            Some(Operation::Assert {
                cond,
                fatal,
                message,
            }) if cond
                .eval(&symbols, pc, s.line)
                .map_err(|err| s.stamp(err))?
                == 0 =>
            {
                if *fatal {
                    return Err(s.err(message));
                }
                warnings.push(Warning::new(s.line, message.clone()).in_file(s.file));
            }
            // An assertion that holds says nothing.
            Some(Operation::Assert { .. }) => {}
            Some(Operation::Unsupported(message)) => {
                return Err(s
                    .err(message)
                    .with_code(crate::contract::Code::UnsupportedFeature));
            }
            Some(Operation::Diagnose { severity, message }) => match severity {
                DiagSeverity::Error => return Err(s.err(message)),
                DiagSeverity::Warning | DiagSeverity::Note => warnings.push(
                    Warning::new(s.line, message.clone())
                        .in_file(s.file)
                        .with_kind(match severity {
                            DiagSeverity::Note => WarningKind::Note,
                            _ => WarningKind::Advisory,
                        }),
                ),
            },
            Some(Operation::Bytes(items)) => {
                for e in items {
                    let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                    emit_byte(&mut bytes, v, floor(1), byte_policy, &mut warnings, s)?;
                }
            }
            Some(Operation::Reserve(count)) => {
                bytes.extend(std::iter::repeat_n(gap_fill, count * addr_unit as usize));
            }
            Some(Operation::Fill { count, value }) => {
                let count = count
                    .eval(&symbols, pc, s.line)
                    .map_err(|err| s.stamp(err))?;
                let count =
                    usize::try_from(count).map_err(|_| s.err("fill count must not be negative"))?;
                bytes.extend(std::iter::repeat_n(*value, count * addr_unit as usize));
            }
            Some(Operation::Binary(payload)) => {
                // Asset data, laid down verbatim — never through the per-byte
                // oversize policy (it is data, not source values). An empty
                // payload is legal (a zero-length incbin) but advisory, the
                // posture sjasmplus takes (`requested to include no data`).
                if payload.is_empty() {
                    warnings.push(
                        Warning::new(s.line, "binary inclusion included no data".to_string())
                            .in_file(s.file),
                    );
                }
                bytes.extend_from_slice(payload);
                // Pad the final partial address unit so the location counter
                // and the byte stream stay in step (pass 1 counted whole units).
                let slack = (addr_unit - payload.len() as i64 % addr_unit) % addr_unit;
                bytes.extend(std::iter::repeat_n(0u8, slack as usize));
            }
            // Read before the passes begin, because they apply to the whole
            // assembly rather than from here on.
            Some(Operation::InitMem(_))
            | Some(Operation::RequestOutput { .. })
            | Some(Operation::RequestSymbols { .. }) => {}
            Some(Operation::PseudoPc(target)) => match target {
                Some(e) => {
                    let at = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                    pseudo_stack.push(pseudo);
                    pseudo = at - real_pc;
                }
                None => {
                    pseudo = pseudo_stack
                        .pop()
                        .ok_or_else(|| s.err("internal: `}` closed no `!pseudopc` block"))?;
                }
            },
            Some(Operation::Sized {
                width,
                big_endian,
                range,
                values,
            }) => {
                for e in values {
                    let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                    let width = *width;
                    let max = (1i64 << (u32::from(width) * 8)) - 1;
                    let min = match range {
                        DataRange::SignedOrUnsigned => floor(u32::from(width)),
                        DataRange::Unsigned => 0,
                    };
                    if !(min..=max).contains(&v) {
                        match byte_policy {
                            Oversize::Error => {
                                return Err(s.operand_err(format!(
                                    "value {v} does not fit in {width} byte(s)"
                                )));
                            }
                            Oversize::Truncate => {}
                            Oversize::TruncateWarn => warnings.push(
                                Warning::new(
                                    s.line,
                                    format!("value {v} truncated to {width} byte(s)"),
                                )
                                .in_file(s.file),
                            ),
                        }
                    }
                    // Most significant byte first when the directive says so,
                    // whatever the CPU's own order is: `!be24` on a 6502 is
                    // big-endian, which is the reason the directive exists.
                    for i in 0..width {
                        let shift = if *big_endian {
                            (width - 1 - i) * 8
                        } else {
                            i * 8
                        };
                        bytes.push(((v >> u32::from(shift)) & 0xFF) as u8);
                    }
                }
            }
            Some(Operation::Words(items)) => {
                for e in items {
                    let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                    push_word(
                        &mut bytes,
                        v,
                        s,
                        set.endianness,
                        floor(2),
                        byte_policy,
                        &mut warnings,
                    )?;
                }
            }
            Some(Operation::Os9Module { fields }) => {
                debug_assert!(matches!(fields.len(), 4 | 6));
                os9_module_start = Some(len_before);
                let values = fields
                    .iter()
                    .map(|e| e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err)))
                    .collect::<Result<Vec<_>, _>>()?;
                bytes.extend_from_slice(&[0x87, 0xCD]);
                emit_value(&mut bytes, values[0], 2, false, isa::Endianness::Big, s)?;
                emit_value(&mut bytes, values[1], 2, false, isa::Endianness::Big, s)?;
                emit_value(&mut bytes, values[2], 1, false, isa::Endianness::Big, s)?;
                emit_value(&mut bytes, values[3], 1, false, isa::Endianness::Big, s)?;
                let check = !bytes[len_before..].iter().fold(0u8, |acc, b| acc ^ b);
                bytes.push(check);
                if values.len() == 6 {
                    emit_value(&mut bytes, values[4], 2, false, isa::Endianness::Big, s)?;
                    emit_value(&mut bytes, values[5], 2, false, isa::Endianness::Big, s)?;
                }
            }
            Some(Operation::Os9EndModule) => {
                let start = os9_module_start
                    .take()
                    .ok_or_else(|| s.err("internal: `emod` has no open OS-9 module"))?;
                let crc = os9_crc24(&bytes[start..]);
                bytes.extend_from_slice(&crc);
            }
            Some(Operation::Instruction {
                mnemonic,
                mode,
                operands,
            }) => {
                let f = form(set, ext, mnemonic, mode, s.line).map_err(|err| s.stamp(err))?;
                cycle_capture = Some(f.cycles);
                if operands.len() != f.operands.len() {
                    return Err(s.err(format!(
                        "internal: `{mnemonic}` {mode} takes {} operand value(s), got {}",
                        f.operands.len(),
                        operands.len()
                    )));
                }
                // The address the branch is *measured from*, which inside a
                // `!pseudopc` block is the claimed one — the label it targets
                // is claimed too, so measuring one against the other would be
                // off by the block's offset.
                let next_addr = origin + pseudo + bytes.len() as i64 + f.len() as i64;
                bytes.extend_from_slice(f.opcode);
                for (slot, e) in f.operands.iter().zip(operands.iter()) {
                    let v = e.eval(&symbols, pc, s.line).map_err(|err| s.stamp(err))?;
                    match slot.kind {
                        // Immediates and addresses lay down a value of the
                        // slot's width; only the width matters on the wire, so
                        // they share a path. (A 6502 immediate is always one
                        // byte; a Z80 `LD BC,nn` immediate is two.)
                        isa::OperandKind::Immediate | isa::OperandKind::Address => {
                            match slot.bytes {
                                1 => emit_byte(
                                    &mut bytes,
                                    v,
                                    floor(1),
                                    operand_policy,
                                    &mut warnings,
                                    s,
                                )?,
                                2 => push_word(
                                    &mut bytes,
                                    v,
                                    s,
                                    set.endianness,
                                    floor(2),
                                    operand_policy,
                                    &mut warnings,
                                )?,
                                // 24-bit address (65816 long addressing).
                                3 => push_addr24(&mut bytes, v, s, set.endianness)?,
                                other => {
                                    return Err(s.err(format!("unsupported operand width {other}")));
                                }
                            }
                        }
                        // A big-endian immediate (Z80N `push nn`): high byte
                        // first, regardless of the set's little-endian default.
                        isa::OperandKind::ImmediateBe => {
                            push_word(
                                &mut bytes,
                                v,
                                s,
                                isa::Endianness::Big,
                                floor(2),
                                operand_policy,
                                &mut warnings,
                            )?;
                        }
                        // A signed index displacement, e.g. the `d` in (IX+d).
                        isa::OperandKind::Displacement => {
                            if !(-128..=127).contains(&v) {
                                return Err(s.operand_err(format!(
                                    "displacement {v} out of range (-128..=127)"
                                )));
                            }
                            bytes.push(v as i8 as u8);
                        }
                        isa::OperandKind::RelativePc => {
                            let offset = v - next_addr;
                            match slot.bytes {
                                1 => {
                                    if !(-128..=127).contains(&offset) {
                                        return Err(s.operand_err(format!(
                                            "branch target out of range ({offset} bytes; must be -128..=127)"
                                        )).with_code(crate::contract::Code::BranchOutOfRange));
                                    }
                                    bytes.push(offset as i8 as u8);
                                }
                                // 16-bit relative (65816 brl/per).
                                2 => {
                                    if !(-32768..=32767).contains(&offset) {
                                        return Err(s.operand_err(format!(
                                            "long branch target out of range ({offset} bytes; must be -32768..=32767)"
                                        )).with_code(crate::contract::Code::BranchOutOfRange));
                                    }
                                    push_word(
                                        &mut bytes,
                                        offset & 0xFFFF,
                                        s,
                                        set.endianness,
                                        floor(2),
                                        byte_policy,
                                        &mut warnings,
                                    )?;
                                }
                                other => {
                                    return Err(
                                        s.err(format!("unsupported relative width {other}"))
                                    );
                                }
                            }
                        }
                    }
                }
                // Trailing opcode bytes after the operands (Z80 DD CB / FD CB).
                bytes.extend_from_slice(f.suffix);
            }
            Some(Operation::Encoded(pieces) | Operation::ComputedInstruction(pieces)) => {
                for piece in pieces {
                    match piece {
                        Piece::Lit(b) => bytes.push(*b),
                        Piece::Val {
                            expr,
                            bytes: width,
                            rel,
                            signed,
                        } => {
                            let raw = expr
                                .eval(&symbols, pc, s.line)
                                .map_err(|err| s.stamp(err))?;
                            // A branch offset is relative to the address that
                            // follows this value (the next instruction) — the
                            // claimed address inside a relocated block, for the
                            // reason `next_addr` is above: the target label is
                            // claimed too, so measuring one against the other
                            // would be off by the block's offset.
                            let next = origin + pseudo + bytes.len() as i64 + i64::from(*width);
                            let v = if *rel { raw - next } else { raw };
                            emit_value(&mut bytes, v, *width, *rel || *signed, set.endianness, s)
                                .map_err(|error| {
                                if *rel {
                                    error.with_code(crate::contract::Code::BranchOutOfRange)
                                } else {
                                    error
                                }
                            })?;
                        }
                        Piece::Packed {
                            expr,
                            bytes: width,
                            scale,
                            min,
                            max,
                            mask,
                            or_bits,
                            what,
                        } => {
                            let raw = expr
                                .eval(&symbols, pc, s.line)
                                .map_err(|err| s.stamp(err))?;
                            if *scale != 1 && raw % *scale != 0 {
                                return Err(s.operand_err(format!(
                                    "{what} ({raw}) is not a multiple of {scale}"
                                )));
                            }
                            let v = raw / *scale;
                            if !(*min..=*max).contains(&v) {
                                return Err(s.operand_err(format!(
                                    "{what} out of range ({v}; must be {min}..={max})"
                                )));
                            }
                            let packed = i64::from((v as u32 & *mask) | *or_bits);
                            emit_value(&mut bytes, packed, *width, false, set.endianness, s)?;
                        }
                        Piece::Branch {
                            target,
                            base,
                            dir_bit,
                            what,
                        } => {
                            let tgt = target
                                .eval(&symbols, pc, s.line)
                                .map_err(|err| s.stamp(err))?;
                            // The CP1610 measures from the address after both
                            // words (opcode + magnitude) — two address units past
                            // this instruction's start (`pc`).
                            let d = tgt - (pc + 2);
                            let (word1, mag) = if d >= 0 {
                                (i64::from(*base), d)
                            } else {
                                (i64::from(*base | *dir_bit), -d - 1)
                            };
                            if !(0..=0xFFFF).contains(&mag) {
                                return Err(s
                                    .operand_err(format!("{what} out of range ({d} words)"))
                                    .with_code(crate::contract::Code::BranchOutOfRange));
                            }
                            emit_value(&mut bytes, word1, 2, false, set.endianness, s)?;
                            emit_value(&mut bytes, mag, 2, false, set.endianness, s)?;
                        }
                    }
                }
            }
            Some(Operation::DirectPage { .. }) => {
                return Err(s.err("internal: unresolved direct-page operand"));
            }
        }

        // --- Debug capture (U2). Reads only `pc`/`bytes.len()`/`symbols`; it
        // never influences an emitted byte (AE2). Addresses are section-relative
        // offsets in address units, section 0 based at `origin`. ---
        if let Some(Operation::DefineSymbols(definitions)) = &s.op {
            debug
                .symbols
                .extend(definitions.iter().map(|(name, value)| debug198x::Symbol {
                    name: name.clone(),
                    kind: debug198x::SymbolKind::Const {
                        value: *value as u64,
                    },
                }));
        }
        if let Some(label) = &s.label {
            if !label.starts_with(|c: char| c.is_ascii_control() || c.is_ascii_digit()) {
                let value = symbols.get(label).copied().unwrap_or(pc);
                let (kind, page) = match &s.op {
                    Some(Operation::Equ(_)) => (
                        crate::sjasm_symbols::Kind::Constant,
                        device_memory.as_ref().and_then(|m| m.location(value)),
                    ),
                    Some(Operation::Set(_)) => (
                        crate::sjasm_symbols::Kind::Variable,
                        device_memory.as_ref().and_then(|m| m.location(value)),
                    ),
                    _ => (crate::sjasm_symbols::Kind::Label, label_page),
                };
                export_symbols.insert(
                    label.clone(),
                    crate::sjasm_symbols::Symbol {
                        value,
                        page: page.map(|p| p.page),
                        kind,
                    },
                );
            }
            let kind = if matches!(&s.op, Some(Operation::Equ(_))) {
                // An `equ`/`=` constant: its value, not an address, and no space.
                let value = symbols.get(label).copied().unwrap_or_default();
                debug198x::SymbolKind::Const {
                    value: value as u64,
                }
            } else {
                if let Some(location) = label_page {
                    debug.symbol_pages.insert(label.clone(), location);
                }
                if let Some(bank) = section_bank {
                    debug.symbol_banks.insert(label.clone(), bank);
                }
                // A label lives at this statement's address (`pc`).
                debug198x::SymbolKind::Label {
                    section: 0,
                    offset: if org_starts_address_run {
                        pc as u64
                    } else {
                        (pc - origin) as u64
                    },
                    space: None,
                }
            };
            debug.symbols.push(debug198x::Symbol {
                name: label.clone(),
                kind,
            });
        }
        // A source-bearing statement that emitted bytes gets a line span; `org`
        // gaps and `align` fill do not (the padding rule).
        let source_bearing = matches!(
            &s.op,
            Some(
                Operation::Bytes(_)
                    | Operation::Words(_)
                    | Operation::Sized { .. }
                    | Operation::Os9Module { .. }
                    | Operation::Os9EndModule
                    | Operation::Instruction { .. }
                    | Operation::Encoded(_)
                    | Operation::ComputedInstruction(_)
                    | Operation::DirectPage { .. }
                    | Operation::Binary(_)
                    | Operation::Reserve(_)
            )
        );
        // ACME's `!xor`, applied to what this statement *wrote*. A
        // reservation and an `org` gap are both excluded, for the same reason
        // they are excluded from `written_len` below: nothing wrote those
        // bytes, so there is nothing to mask. Probed — `!xor $ff` around a
        // gap gives `15 00 00 15`, the `nop`s masked and the gap not.
        if s.xor_mask != 0
            && !matches!(s.op, Some(Operation::Reserve(_)) | Some(Operation::Org(_)))
            && bytes.len() > len_before
        {
            for b in &mut bytes[len_before..] {
                *b ^= s.xor_mask;
            }
        }
        if bytes.len() > len_before
            && !matches!(s.op, Some(Operation::Org(_)) | Some(Operation::Reserve(_)))
            && let Some(memory) = &mut device_memory
        {
            memory
                .write(pc, &bytes[len_before..])
                .map_err(|err| s.stamp(err))?;
        }
        if !matches!(s.op, Some(Operation::Org(_)) | Some(Operation::Reserve(_))) {
            written_len = bytes.len();
            if bytes.len() > len_before {
                written_start.get_or_insert(len_before);
                wrote_any = true;
            }
        }
        if source_bearing && bytes.len() > len_before {
            let offset = if org_starts_address_run {
                pc as u64
            } else {
                (pc - origin) as u64
            };
            let mut length = ((bytes.len() - len_before) as i64 / addr_unit) as u64;
            let mut at = offset;
            while length > 0 {
                let part = if wraps_address_space {
                    length.min(0x1_0000 - at)
                } else {
                    length
                };
                debug.lines.push(LineRec {
                    line: s.line as u32,
                    offset: at,
                    length: part,
                    file: s.file,
                });
                length -= part;
                at = 0;
            }
            let bounds = if matches!(s.op, Some(Operation::ComputedInstruction(_))) {
                let timing = dialect.computed_timing(&bytes[len_before..]);
                if timing.is_none() {
                    debug.cycle_coverage = CycleCoverage::Partial;
                }
                timing
            } else {
                None
            };
            if cycle_capture.is_some() || bounds.is_some() {
                let cy = cycle_capture.unwrap_or(isa::Cycles::fixed(0));
                debug.cycles.push(CycleRec {
                    line: s.line as u32,
                    offset,
                    base: bounds.map_or(cy.base, |b| b.min),
                    page_cross: cy.page_cross,
                    branch_taken: cy.branch_taken,
                    bounds,
                    file: s.file,
                });
            }
        }
        // The entry point (`end <addr>`) is an Entry symbol. When it targets a
        // bare label, upgrade that label's kind in place (the entry *is* that
        // location) rather than emitting a second same-named symbol; otherwise
        // record a fresh `@entry`.
        if let (Some(Operation::Entry(e)), Some(v)) = (&s.op, start) {
            let entry = debug198x::SymbolKind::Entry {
                section: 0,
                offset: if org_starts_address_run {
                    u64::from(v)
                } else {
                    (i64::from(v) - origin) as u64
                },
                space: None,
            };
            let existing = match e {
                Expr::Sym(n) => debug.symbols.iter_mut().find(|s| {
                    s.name == *n && matches!(s.kind, debug198x::SymbolKind::Label { .. })
                }),
                _ => None,
            };
            if let Some(sym) = existing {
                sym.kind = entry;
            } else {
                let name = match e {
                    Expr::Sym(n) => n.clone(),
                    _ => "@entry".to_string(),
                };
                if let Some(location) = device_memory
                    .as_ref()
                    .and_then(|m| m.location(i64::from(v)))
                {
                    debug.symbol_pages.insert(name.clone(), location);
                }
                debug.symbols.push(debug198x::Symbol { name, kind: entry });
            }
        }

        if wraps_address_space {
            let end = origin + bytes.len() as i64;
            if pc + (bytes.len().saturating_sub(len_before)) as i64 > 0x1_0000 {
                warnings.push(
                    Warning::new(s.line, "64KB limit passed inside instruction".to_string())
                        .in_file(s.file),
                );
            }
            if end >= 0x1_0000 {
                // Close physical address runs at the boundary and reuse the
                // shared overwrite placement. Compact after each wrap, keeping
                // the accumulated memory image at no more than 64 KiB.
                let pending = std::mem::take(&mut bytes);
                let mut at = origin;
                let mut remaining = pending.as_slice();
                while !remaining.is_empty() {
                    let count = remaining.len().min((0x1_0000 - at) as usize);
                    runs.push(Run {
                        name: "wrapped address run".to_string(),
                        base: at,
                        at: Place::ByAddress,
                        bytes: remaining[..count].to_vec(),
                    });
                    let (base, image) =
                        lay_out(std::mem::take(&mut runs), gap_fill, 1, None, true, |_| None)?;
                    runs.push(Run {
                        name: "memory image".to_string(),
                        base,
                        at: Place::ByAddress,
                        bytes: image,
                    });
                    remaining = &remaining[count..];
                    at = 0;
                }
                origin = end & 0xffff;
                written_len = 0;
                written_start = None;
                wrote_any = false;
            }
        } else if origin + bytes.len().div_ceil(addr_unit as usize) as i64 > 0x1_0000 {
            return Err(s.err("program exceeds the 64K address space"));
        }
    }

    if let Some(session) = nex_session {
        let bytes = session
            .image
            .finish()
            .map_err(|e| AsmError::at(session.span, e.to_string()))?;
        artifacts.push(Artifact {
            name: session.name,
            format: ArtifactFormat::Nex,
            bytes,
        });
    }

    for (name, format, virtual_labels, page_size) in symbol_maps {
        let text = match format {
            ArtifactFormat::CspectMap => crate::sjasm_symbols::cspect(&export_symbols, page_size),
            ArtifactFormat::LabelsList => crate::sjasm_symbols::labels(
                &export_symbols,
                device_memory
                    .as_ref()
                    .map(|m| (m.spec.name.as_str(), m.spec.slot_size as u32)),
                virtual_labels,
            ),
            _ => unreachable!("only symbol-map formats enter symbol_maps"),
        };
        artifacts.push(Artifact {
            name,
            format,
            bytes: text.into_bytes(),
        });
    }

    // Lay the sections into one image. Only a dialect that opened a section
    // reaches this; the flat path below is unchanged for the other twenty.
    if !runs.is_empty() {
        runs.push(Run {
            name: section_name,
            base: origin,
            at: section_at,
            bytes,
        });
        let (origin_of_image, image) = lay_out(
            runs,
            gap_fill,
            addr_unit,
            dialect.image_base(),
            dialect.later_run_overwrites(),
            |img| dialect.image_size(img),
        )?;
        let debug_base = if wraps_address_space {
            address_debug_base(&debug, origin_of_image as u64)
        } else {
            origin_of_image as u64
        };
        if org_starts_address_run {
            let base = debug_base;
            for symbol in &mut debug.symbols {
                match &mut symbol.kind {
                    debug198x::SymbolKind::Label { offset, .. }
                    | debug198x::SymbolKind::Entry { offset, .. } => *offset -= base,
                    _ => {}
                }
            }
            for line in &mut debug.lines {
                line.offset -= base;
            }
            for c in &mut debug.cycles {
                c.offset -= base;
            }
        }
        return Ok(Assembly {
            requested_output,
            requested_symbols,
            artifacts,
            areas: flat_space_usage(
                origin_of_image as u32,
                (image.len() as i64 / addr_unit) as u32,
            ),
            origin: origin_of_image as u16,
            reserved_prefix: if wraps_address_space {
                (origin_of_image as u64 - debug_base) as u16
            } else {
                0
            },
            bytes: image,
            symbols,
            start,
            warnings,
            debug,
        });
    }

    // asl reserves rather than materialises: `p2bin` fills only the gaps *inside*
    // the written range. Outside it, both ends fall away — a trailing
    // reservation is absent from the image, and a leading one moves where the
    // image starts rather than padding it. The two trims are the same rule read
    // from each end, so they live together (#66, #90).
    let mut reserved_prefix = 0u16;
    if dialect.trims_trailing_gap() {
        bytes.truncate(written_len);
        match written_start {
            Some(first) if first > 0 => {
                bytes.drain(..first);
                let units = first as i64 / addr_unit;
                origin += units;
                reserved_prefix = units as u16;
            }
            // Nothing was ever written, so `truncate` already emptied the image
            // and there is no load address to move.
            _ => {}
        }
    }

    if org_starts_address_run {
        let base = if wraps_address_space {
            address_debug_base(&debug, origin as u64)
        } else {
            origin as u64
        };
        if wraps_address_space {
            reserved_prefix = (origin as u64 - base) as u16;
        }
        for symbol in &mut debug.symbols {
            match &mut symbol.kind {
                debug198x::SymbolKind::Label { offset, .. }
                | debug198x::SymbolKind::Entry { offset, .. } => *offset -= base,
                _ => {}
            }
        }
        for line in &mut debug.lines {
            line.offset -= base;
        }
        for c in &mut debug.cycles {
            c.offset -= base;
        }
    }

    let areas = flat_space_usage(
        if wraps_address_space {
            origin as u32
        } else {
            origin as u32 - u32::from(reserved_prefix)
        },
        (bytes.len() as i64 / addr_unit) as u32
            + if wraps_address_space {
                0
            } else {
                u32::from(reserved_prefix)
            },
    );
    Ok(Assembly {
        requested_output,
        requested_symbols,
        artifacts,
        areas,
        origin: origin as u16,
        reserved_prefix,
        bytes,
        symbols,
        start,
        warnings,
        debug,
    })
}

/// The flat dialects' memory account (#499): one area — the 64K address
/// space — occupied by the program's contiguous span. `start` and `used` are
/// address units. The free space splits into below-origin and above-top, so
/// the largest contiguous run is whichever side is bigger.
fn flat_space_usage(start: u32, used: u32) -> Vec<AreaUsage> {
    const SPACE: u32 = 0x1_0000;
    let end = start.saturating_add(used).min(SPACE);
    let below = start;
    let above = SPACE - end;
    vec![AreaUsage {
        name: "address space".to_string(),
        start: 0,
        size: SPACE,
        used: end - start,
        free: below + above,
        largest_free: below.max(above),
        segments: Vec::new(),
    }]
}

/// SjASMPlus's live device memory, distinct from its raw emission log.
struct NexSession {
    name: String,
    image: crate::nex::Image,
    span: Span,
}

#[allow(clippy::too_many_arguments)]
fn apply_nex(
    command: &NexCommand,
    s: &Statement,
    memory: Option<&DeviceMemory>,
    symbols: &BTreeMap<String, i64>,
    pc: i64,
    start: Option<u16>,
    session: &mut Option<NexSession>,
    artifacts: &mut Vec<Artifact>,
    warnings: &mut Vec<Warning>,
) -> Result<(), AsmError> {
    let memory = memory
        .filter(|m| m.spec.name == "ZXSPECTRUMNEXT")
        .ok_or_else(|| s.err("[SAVENEX] is allowed only in ZXSPECTRUMNEXT device mode"))?;
    let eval = |e: &Expr| e.eval(symbols, pc, s.line).map_err(|e| s.stamp(e));
    let warn = |warnings: &mut Vec<Warning>, message: String| {
        warnings.push(Warning::new(s.line, message).in_file(s.file))
    };
    match command {
        NexCommand::Open { name, values } => {
            if session.is_some() {
                return Err(s.err("[SAVENEX] NEX file is already open"));
            }
            let v = values.iter().map(eval).collect::<Result<Vec<_>, _>>()?;
            let entry_pc = v.first().copied().unwrap_or(i64::from(start.unwrap_or(0)));
            let stack = v.get(1).copied().unwrap_or(0xfffe);
            let entry = v.get(2).copied().unwrap_or(0);
            let version = v.get(3).copied().unwrap_or(0);
            match version {
                0 | 2 => {}
                3 => {
                    return Err(s
                        .err("SAVENEX V1.3 output is not implemented")
                        .with_code(crate::contract::Code::UnsupportedFeature));
                }
                _ => return Err(s.err("[SAVENEX] only file version 2 or 3 can be enforced")),
            }
            let entry = usize::try_from(entry)
                .ok()
                .filter(|v| *v < 112)
                .ok_or_else(|| s.err("[SAVENEX] entry bank must be in 0..111"))?;
            for value in [entry_pc, stack] {
                if !(-32768..=65535).contains(&value) {
                    warn(
                        warnings,
                        format!(
                            "value {value} is truncated to 16bit value: 0x{:04X}",
                            value as u16
                        ),
                    );
                }
            }
            if start.is_some_and(|start| i64::from(start) != entry_pc) {
                warn(
                    warnings,
                    "[SAVENEX] Start address was also defined by END, OPEN argument used instead"
                        .into(),
                );
            }
            let sp = stack as u16;
            if 0 < sp && sp < 0x400a {
                warn(
                    warnings,
                    "[SAVENEX] stackAddress is too close to ROM area".into(),
                );
            } else {
                let pages = [0, 0, 10, 11, 4, 5, entry * 2, entry * 2 + 1];
                if (1..=10u16).any(|n| {
                    let at = sp.wrapping_sub(n) as usize;
                    memory.pages[pages[at / 8192]][at % 8192] != 0
                }) {
                    warn(
                        warnings,
                        "[SAVENEX] stack will overwrite nonzero memory".into(),
                    );
                }
            }
            *session = Some(NexSession {
                name: name.clone(),
                image: crate::nex::Image::new(entry_pc as u16, sp, entry)
                    .map_err(|e| s.err(e.to_string()))?,
                span: Span::in_file(s.file, s.line as u32, 0),
            });
        }
        NexCommand::Core(values) => {
            let active = session
                .as_mut()
                .ok_or_else(|| s.err("[SAVENEX] NEX file is not open"))?;
            let v = [eval(&values[0])?, eval(&values[1])?, eval(&values[2])?];
            if !(0..=15).contains(&v[0]) || !(0..=15).contains(&v[1]) || !(0..=255).contains(&v[2])
            {
                warn(
                    warnings,
                    "[SAVENEX] values are not within 0..15,0..15,0..255 ranges".into(),
                );
            }
            active.image.core(v.map(|v| v as u8));
        }
        NexCommand::Auto(values) => {
            let active = session
                .as_mut()
                .ok_or_else(|| s.err("[SAVENEX] NEX file is not open"))?;
            let v = values
                .iter()
                .map(|e| {
                    usize::try_from(eval(e)?).map_err(|_| s.err("[SAVENEX] bank must be in 0..111"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            active
                .image
                .auto(v.first().copied(), v.get(1).copied(), &memory.pages)
                .map_err(|e| s.err(e.to_string()))?;
        }
        NexCommand::Close => {
            let active = session
                .take()
                .ok_or_else(|| s.err("[SAVENEX] NEX file is not open"))?;
            let bytes = active.image.finish().map_err(|e| s.err(e.to_string()))?;
            artifacts.push(Artifact {
                name: active.name,
                format: ArtifactFormat::Nex,
                bytes,
            });
        }
    }
    Ok(())
}

struct DeviceMemory {
    spec: DeviceSpec,
    slots: Vec<usize>,
    current_slot: usize,
    pages: Vec<Vec<u8>>,
}

impl DeviceMemory {
    fn location(&self, address: i64) -> Option<PagedLocation> {
        let address = usize::try_from(address).ok()?;
        let slot = address / self.spec.slot_size;
        Some(PagedLocation {
            slot: u8::try_from(slot).ok()?,
            page: u16::try_from(*self.slots.get(slot)?).ok()?,
            page_size: u32::try_from(self.spec.slot_size).ok()?,
            offset: u32::try_from(address % self.spec.slot_size).ok()?,
        })
    }

    fn new(spec: DeviceSpec) -> Self {
        // SjASMPlus 1.21.0's initial slot mapping, not sequential bank IDs.
        // Native CSPECTMAP probes cover every slot of every built-in device.
        let slots = match spec.name.as_str() {
            "ZXSPECTRUMNEXT" => vec![14, 15, 10, 11, 4, 5, 0, 1],
            "ZXSPECTRUM128" | "ZXSPECTRUM256" | "ZXSPECTRUM512" | "ZXSPECTRUM1024"
            | "ZXSPECTRUM2048" | "ZXSPECTRUM4096" | "ZXSPECTRUM8192" => vec![7, 5, 2, 0],
            _ => (0..spec.slots).collect(),
        };
        let mut memory = Self {
            slots,
            current_slot: spec.slots - 1,
            pages: vec![vec![0; spec.slot_size]; spec.pages],
            spec,
        };
        if matches!(
            memory.spec.name.as_str(),
            "ZXSPECTRUM48"
                | "ZXSPECTRUM128"
                | "ZXSPECTRUM256"
                | "ZXSPECTRUM512"
                | "ZXSPECTRUM1024"
                | "ZXSPECTRUM2048"
                | "ZXSPECTRUM4096"
                | "ZXSPECTRUM8192"
        ) {
            seed_spectrum_48k(&mut memory.pages, &memory.slots);
        }
        memory
    }

    fn select_slot(&mut self, slot: i64) -> Result<(), AsmError> {
        let slot = usize::try_from(slot)
            .ok()
            .filter(|slot| *slot < self.spec.slots)
            .ok_or_else(|| {
                AsmError::new(
                    0,
                    format!(
                        "slot {slot} is outside `{}`, which has {} slots",
                        self.spec.name, self.spec.slots
                    ),
                )
            })?;
        self.current_slot = slot;
        Ok(())
    }

    fn map_page(&mut self, page: i64) -> Result<(), AsmError> {
        let page = usize::try_from(page)
            .ok()
            .filter(|page| *page < self.spec.pages)
            .ok_or_else(|| {
                AsmError::new(
                    0,
                    format!(
                        "page {page} is outside `{}`, which has {} pages",
                        self.spec.name, self.spec.pages
                    ),
                )
            })?;
        self.slots[self.current_slot] = page;
        Ok(())
    }

    fn write(&mut self, address: i64, bytes: &[u8]) -> Result<(), AsmError> {
        for (offset, byte) in bytes.iter().copied().enumerate() {
            let address = address + offset as i64;
            let address = usize::try_from(address)
                .ok()
                .filter(|address| *address < 0x1_0000)
                .ok_or_else(|| {
                    AsmError::new(0, format!("Write outside of device memory at: {address}"))
                })?;
            let slot = address / self.spec.slot_size;
            let within = address % self.spec.slot_size;
            self.pages[self.slots[slot]][within] = byte;
        }
        Ok(())
    }

    fn read(&self, address: i64, len: i64) -> Result<Vec<u8>, AsmError> {
        let end = address.checked_add(len).ok_or_else(|| {
            AsmError::new(0, "device-memory save range overflows its address space")
        })?;
        if address < 0 || len < 0 || end > 0x1_0000 {
            return Err(AsmError::new(
                0,
                format!("device-memory save range ${address:04X}..${end:04X} is outside 64K"),
            ));
        }
        Ok((address..end)
            .map(|address| {
                let address = address as usize;
                self.pages[self.slots[address / self.spec.slot_size]][address % self.spec.slot_size]
            })
            .collect())
    }
}

/// The 48K ROM's deterministic START initialisation, derived in
/// `syntheses/sinclair-zx-spectrum/post-boot-ram.md`. Larger classic Spectrum devices
/// expose this same initial 64K window and leave every additional page zero.
fn seed_spectrum_48k(pages: &mut [Vec<u8>], slots: &[usize]) {
    pages[slots[1]][0x1800..0x1b00].fill(0x38);

    const SYSVARS: &[(usize, u8)] = &[
        (0x00, 0xff),
        (0x04, 0xff),
        (0x09, 0x14),
        (0x0a, 0x01),
        (0x10, 0x01),
        (0x12, 0x06),
        (0x14, 0x0b),
        (0x16, 0x01),
        (0x18, 0x01),
        (0x1a, 0x06),
        (0x1c, 0x10),
        (0x37, 0x3c),
        (0x38, 0x40),
        (0x3a, 0xff),
        (0x3b, 0xcc),
        (0x3c, 0x01),
        (0x3d, 0x58),
        (0x3e, 0x5d),
        (0x48, 0x38),
        (0x4b, 0xcb),
        (0x4c, 0x5c),
        (0x4f, 0xb6),
        (0x50, 0x5c),
        (0x51, 0xb6),
        (0x52, 0x5c),
        (0x53, 0xcb),
        (0x54, 0x5c),
        (0x57, 0xca),
        (0x58, 0x5c),
        (0x59, 0xcc),
        (0x5a, 0x5c),
        (0x5b, 0xcc),
        (0x5c, 0x5c),
        (0x5d, 0xcc),
        (0x5e, 0x5c),
        (0x61, 0xce),
        (0x62, 0x5c),
        (0x63, 0xce),
        (0x64, 0x5c),
        (0x65, 0xce),
        (0x66, 0x5c),
        (0x68, 0x92),
        (0x69, 0x5c),
        (0x6a, 0x10),
        (0x6b, 0x02),
        (0x7b, 0x58),
        (0x7c, 0xff),
        (0x7f, 0x21),
        (0x80, 0x5b),
        (0x82, 0x21),
        (0x83, 0x17),
        (0x85, 0x40),
        (0x86, 0xe0),
        (0x87, 0x50),
        (0x88, 0x21),
        (0x89, 0x18),
        (0x8a, 0x21),
        (0x8b, 0x17),
        (0x8c, 0x01),
        (0x8d, 0x38),
        (0x8f, 0x38),
        (0xb2, 0x5b),
        (0xb3, 0x5d),
        (0xb4, 0xff),
        (0xb5, 0xff),
        (0xb6, 0xf4),
        (0xb7, 0x09),
        (0xb8, 0xa8),
        (0xb9, 0x10),
        (0xba, 0x4b),
        (0xbb, 0xf4),
        (0xbc, 0x09),
        (0xbd, 0xc4),
        (0xbe, 0x15),
        (0xbf, 0x53),
        (0xc0, 0x81),
        (0xc1, 0x0f),
        (0xc2, 0xc4),
        (0xc3, 0x15),
        (0xc4, 0x52),
        (0xc5, 0xf4),
        (0xc6, 0x09),
        (0xc7, 0xc4),
        (0xc8, 0x15),
        (0xc9, 0x50),
        (0xca, 0x80),
        (0xcb, 0x80),
        (0xcc, 0x0d),
        (0xcd, 0x80),
        (0x158, 0x03),
        (0x159, 0x13),
        (0x15b, 0x3e),
    ];
    for &(offset, value) in SYSVARS {
        pages[slots[1]][0x1c00 + offset] = value;
    }

    const UDG_A_TO_U: [u8; 168] = [
        0, 60, 66, 66, 126, 66, 66, 0, 0, 124, 66, 124, 66, 66, 124, 0, 0, 60, 66, 64, 64, 66, 60,
        0, 0, 120, 68, 66, 66, 68, 120, 0, 0, 126, 64, 124, 64, 64, 126, 0, 0, 126, 64, 124, 64,
        64, 64, 0, 0, 60, 66, 64, 78, 66, 60, 0, 0, 66, 66, 126, 66, 66, 66, 0, 0, 62, 8, 8, 8, 8,
        62, 0, 0, 2, 2, 2, 66, 66, 60, 0, 0, 68, 72, 112, 72, 68, 66, 0, 0, 64, 64, 64, 64, 64,
        126, 0, 0, 66, 102, 90, 66, 66, 66, 0, 0, 66, 98, 82, 74, 70, 66, 0, 0, 60, 66, 66, 66, 66,
        60, 0, 0, 124, 66, 66, 124, 64, 64, 0, 0, 60, 66, 66, 82, 74, 60, 0, 0, 124, 66, 66, 124,
        68, 66, 0, 0, 60, 64, 60, 2, 66, 60, 0, 0, 254, 16, 16, 16, 16, 16, 0, 0, 66, 66, 66, 66,
        66, 60, 0,
    ];
    pages[slots[3]][0x3f58..].copy_from_slice(&UDG_A_TO_U);
}

/// RIFF CPR layout from `reference/by-system/amstrad-cpc/formats.md` §CPR.
fn cpr_image(pages: &[Vec<u8>]) -> Vec<u8> {
    let payload_len = 4 + pages.iter().map(|page| 8 + page.len()).sum::<usize>();
    let mut out = Vec::with_capacity(payload_len + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(payload_len as u32).to_le_bytes());
    out.extend_from_slice(b"AMS!");
    for (index, page) in pages.iter().enumerate() {
        out.extend_from_slice(format!("cb{index:02}").as_bytes());
        out.extend_from_slice(&(page.len() as u32).to_le_bytes());
        out.extend_from_slice(page);
    }
    out
}

/// Wrap a span as a Spectrum tape: a ROM header block naming it, then the span.
///
/// The two parameter words are the tape's, not ours, and SjASMPlus 1.21.0 was
/// measured setting them: for `Code` the first is the start address and the
/// second is `$8000`; for a `Program` the first is `$8000` — the value that
/// means "no auto-start line", whatever start the directive was given — and
/// the second is the length again.
fn tape_image(kind: TapeKind, name: &str, at: i64, span: Vec<u8>) -> Vec<u8> {
    use format198x_sinclair_zx_spectrum_tap as tap;
    let len = span.len() as u16;
    let start = at as u16;
    let header = match kind {
        TapeKind::Code => tap::Header::new(tap::HeaderKind::Code, name, len, start, 0x8000),
        TapeKind::Program => tap::Header::new(tap::HeaderKind::Program, name, len, 0x8000, len),
    };
    tap::encode(&[header.block(), tap::TapBlock::data(span)])
}

/// Emit a [`Piece::Val`]: `width` bytes of `v` in the CPU's endianness. `signed`
/// range-checks as two's-complement (branch offsets, index displacements);
/// otherwise as an unsigned address/immediate (a byte also accepts `-128..=-1`).
fn emit_value(
    bytes: &mut Vec<u8>,
    v: i64,
    width: u8,
    signed: bool,
    endianness: isa::Endianness,
    s: &Statement,
) -> Result<(), AsmError> {
    // `signed` (branch offsets, signed index displacements) range-checks as
    // two's-complement. Otherwise the value is an address/immediate/large index
    // offset, accepted as either-signed across the full width: a 16-bit indexed
    // offset is often a base address ≥ `$8000` yet a small one may be negative.
    let (lo, hi) = match width {
        1 if signed => (-128, 127),
        1 => (-128, 0xFF),
        2 if signed => (-32768, 32767),
        2 => (-32768, 0xFFFF),
        // Three bytes: sjasmplus's `d24`, and the width a 65816 long address
        // wants. Ranged like the others — either sign across the full width.
        3 if signed => (-0x80_0000, 0x7F_FFFF),
        3 => (-0x80_0000, 0xFF_FFFF),
        4 if signed => (i64::from(i32::MIN), i64::from(i32::MAX)),
        4 => (i64::from(i32::MIN), i64::from(u32::MAX)),
        other => {
            return Err(s.err(format!("unsupported value width {other}")));
        }
    };
    if !(lo..=hi).contains(&v) {
        return Err(s.operand_err(format!("value {v} out of range for a {width}-byte operand")));
    }
    let b = v.to_le_bytes();
    match (width, endianness) {
        (1, _) => bytes.push(b[0]),
        (2, isa::Endianness::Little) => bytes.extend_from_slice(&b[..2]),
        (2, isa::Endianness::Big) => bytes.extend_from_slice(&[b[1], b[0]]),
        (3, isa::Endianness::Little) => bytes.extend_from_slice(&b[..3]),
        (3, isa::Endianness::Big) => bytes.extend_from_slice(&[b[2], b[1], b[0]]),
        (4, isa::Endianness::Little) => bytes.extend_from_slice(&b[..4]),
        (4, isa::Endianness::Big) => bytes.extend_from_slice(&[b[3], b[2], b[1], b[0]]),
        _ => unreachable!("width validated above"),
    }
    Ok(())
}

/// Emit a byte value, applying the dialect's over-range `policy`. A value in
/// `-128..=255` fits and is emitted as-is; beyond that, the policy decides —
/// error, silently keep the low 8 bits (pasmo), or keep them with a warning
/// (sjasmplus).
fn emit_byte(
    bytes: &mut Vec<u8>,
    v: i64,
    floor: i64,
    policy: Oversize,
    warnings: &mut Vec<Warning>,
    s: &Statement,
) -> Result<(), AsmError> {
    // `floor` is 0 for a reference that takes no negative literal at any
    // width — ca65 — and the signed minimum for the six that do. Hardcoding
    // -128 here accepted `.byte -1` that ca65 refuses (asm198x#290).
    if !(floor..=0xFF).contains(&v) {
        match policy {
            Oversize::Error => {
                return Err(s.operand_err(format!("value {v} does not fit in a byte")));
            }
            Oversize::Truncate => {}
            Oversize::TruncateWarn => {
                warnings.push(
                    Warning::new(s.line, format!("value {v} truncated to a byte")).in_file(s.file),
                );
            }
        }
    }
    bytes.push((v & 0xFF) as u8);
    Ok(())
}

fn push_word(
    bytes: &mut Vec<u8>,
    v: i64,
    s: &Statement,
    endianness: isa::Endianness,
    floor: i64,
    policy: Oversize,
    warnings: &mut Vec<Warning>,
) -> Result<(), AsmError> {
    // The same shape as `emit_byte`, and for the same reason: what counts as
    // in range is the dialect's answer, not the width's. Before this, a word
    // was `0..=0xFFFF` for everyone — which refused `!word -1` that ACME,
    // asl, lwasm, vasm, sjasmplus, pasmo and rgbasm all accept, and refused
    // the truncation five of them perform above the range (asm198x#290).
    if !(floor..=0xFFFF).contains(&v) {
        match policy {
            Oversize::Error => {
                return Err(s.operand_err(format!("value {v} does not fit in a word")));
            }
            Oversize::Truncate => {}
            Oversize::TruncateWarn => {
                warnings.push(
                    Warning::new(s.line, format!("value {v} truncated to a word")).in_file(s.file),
                );
            }
        }
    }
    let lo = (v & 0xFF) as u8;
    let hi = ((v >> 8) & 0xFF) as u8;
    match endianness {
        isa::Endianness::Little => {
            bytes.push(lo);
            bytes.push(hi);
        }
        isa::Endianness::Big => {
            bytes.push(hi);
            bytes.push(lo);
        }
    }
    Ok(())
}

/// Emit a 24-bit address (the 65816 long-addressing operand).
fn push_addr24(
    bytes: &mut Vec<u8>,
    v: i64,
    s: &Statement,
    endianness: isa::Endianness,
) -> Result<(), AsmError> {
    if !(0..=0xFF_FFFF).contains(&v) {
        return Err(s.operand_err(format!("value {v} does not fit in a 24-bit address")));
    }
    let b = [
        (v & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        ((v >> 16) & 0xFF) as u8,
    ];
    match endianness {
        isa::Endianness::Little => bytes.extend_from_slice(&b),
        isa::Endianness::Big => bytes.extend_from_slice(&[b[2], b[1], b[0]]),
    }
    Ok(())
}

/// CRC-24 used by OS-9 module trailers, transliterated from the NitrOS-9
/// routine used by lwtools. The stored value is the complemented accumulator.
fn os9_crc24(image: &[u8]) -> [u8; 3] {
    let mut crc = [0xFFu8; 3];
    for &input in image {
        let mut byte = input ^ crc[0];
        crc[0] = crc[1];
        crc[1] = crc[2];
        crc[1] ^= byte >> 7;
        crc[2] = byte << 1;
        crc[1] ^= byte >> 2;
        crc[2] ^= byte << 6;
        byte ^= byte << 1;
        byte ^= byte << 2;
        byte ^= byte << 4;
        if byte & 0x80 != 0 {
            crc[0] ^= 0x80;
            crc[2] ^= 0x21;
        }
    }
    [crc[0] ^ 0xFF, crc[1] ^ 0xFF, crc[2] ^ 0xFF]
}
