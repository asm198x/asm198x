//! ca65 V2.19 through Lang198x's ca65 front end.
//!
//! `lang198x-ca65` reads ca65 source, runs its preprocessing, and lowers
//! each line to statements. This walk does what that crate leaves to an
//! assembler, the way PasmoNext's walk does for the `pasmo` dialect: it
//! keeps what ca65 knows of each name as it reads the program once, from
//! the top, so lowering can choose an operand's size as ca65 does, and it
//! keeps ca65's scopes, cheap locals and unnamed labels. The engine then
//! assembles the statements in two passes.
//!
//! This is step 6 of `docs/plans/2026-10-27-ca65-walk/plan.md`. The old
//! reader in `dialects/ca65` stays the default until step 7.
//!
//! # Scopes
//!
//! A name's full name joins its scopes with `::`, as ca65 writes them:
//! `inner::tail`. A name at the top has no prefix, and an unnamed scope
//! takes a number, which no name can start with. ca65 binds a name written
//! without a scope at the end of the program, to the innermost scope
//! around the reference that defines it. Until then, what it knows of the
//! name is what it knows of the innermost one defined so far, with a
//! constant from further out known only by its address size.
//!
//! # Segments
//!
//! A program starts in `CODE`, and each segment holds its own bytes, which
//! continue where they left off when the program comes back to it. A label
//! takes its segment's address size: the zero page in `ZEROPAGE`, absolute
//! elsewhere unless `.segment` says otherwise. The engine gets a section
//! each time the segment changes. Where each section goes is only known
//! once every segment's length is, so the walk places them at the end, from
//! the linker's layout (`ca65_layout`): the address the CPU sees it at, and
//! where its bytes go in the image, or nowhere for RAM. A flat link has one
//! segment, `CODE`, at 0, as Asm198x's recorded verdicts were linked.
//!
//! In a linked program, `.org` opens a section too, at the address it
//! gives, with its bytes placed where the segment had got to. ca65 moves
//! only what addresses read as, not where bytes go, and a section carries
//! both. A flat program has no sections, so there `.org` moves the
//! engine's `PseudoPc` instead.
//!
//! # What a link gives besides its image
//!
//! The engine keeps each section's debug records apart, and `linked` names
//! them by segment, as the old reader does, for the sidecar, the listing
//! and the memory map. A linked program's `asm198x:` budgets are checked
//! against that, once the walk's caller has it.

mod linked;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};

use lang198x_ca65::Profile;
use lang198x_ca65::ast::{Expr as SourceExpr, NameRef};
use lang198x_ca65::expand::{ActiveLine, Expander, Host};
use lang198x_ca65::lower::{self, Binary, Known, Scope, Segment, Size, Value, Visibility, Visible};
use lang198x_ca65::syntax::{self, DiagnosticKind};

use super::ca65_flat::{Resolution, load_binary, load_include, slice_incbin};
use super::ca65_layout::{Layout, ResolvedLayout};
use crate::ast::{map_sym_expr, map_syms};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Place, Statement, Warning, align_pad, next_pc};
use crate::source::{SourceLoader, SourceMap};
use crate::span::{FileId, Span};

const PROFILE: Profile = Profile::CA65_2_19_6502;

/// The ca65 dialect through Lang198x's front end, for the 6502.
pub(crate) struct Ca65Walk {
    /// Where the linker puts each segment, or `None` for a flat link, which
    /// has only `CODE`, at 0.
    layout: Option<Layout>,
    /// Where the link put each segment, and how long each is, once the walk
    /// has placed them.
    linked: RefCell<Option<Linked>>,
    /// The equates of the last program ca65 refuses as circular only where
    /// they are used.
    lazy: RefCell<HashSet<String>>,
}

/// What a link made of the program's segments.
pub(crate) struct Linked {
    /// Where each segment went.
    pub(crate) resolved: ResolvedLayout,
    /// How long each segment the program used is, by name.
    pub(crate) lengths: BTreeMap<String, u32>,
    /// Each section the walk opened and placed, in order.
    pub(crate) opened: Vec<Opened>,
}

/// A section the walk opened.
pub(crate) struct Opened {
    /// Its segment.
    pub(crate) segment: String,
    /// How far into the segment it starts.
    pub(crate) offset: i64,
    /// The address `.org` gave it, if it gave one.
    pub(crate) org: Option<i64>,
}

impl Ca65Walk {
    /// A flat link: `CODE` at 0, written as it is, as Asm198x's recorded
    /// verdicts were linked.
    pub(crate) fn flat() -> Self {
        Self {
            layout: None,
            linked: RefCell::new(None),
            lazy: RefCell::new(HashSet::new()),
        }
    }

    /// The NES curriculum's link: the iNES header, 32K of PRG and 8K of CHR,
    /// with the RAM segments in their areas.
    pub(crate) fn nes() -> Self {
        Self::with_layout(Layout::nes_default())
    }

    /// The link a project's own ld65 configuration describes.
    pub(crate) fn with_layout(layout: Layout) -> Self {
        Self {
            layout: Some(layout),
            linked: RefCell::new(None),
            lazy: RefCell::new(HashSet::new()),
        }
    }

    /// Keeps what the engine asks of a program read, and gives it the
    /// statements and warnings.
    fn keep(&self, ran: Ran) -> (Vec<Statement>, Vec<Warning>) {
        self.linked.replace(ran.linked);
        self.lazy.replace(ran.lazy);
        (ran.statements, ran.warnings)
    }

    /// The result of a linked assembly, with its debug record by segment.
    /// A flat program gives its result as the engine made it, and no
    /// record.
    pub(crate) fn linked_result(
        &self,
        assembly: crate::engine::Assembly,
    ) -> (
        crate::AssemblyResult,
        Option<crate::listing::DebugCaptureMulti>,
    ) {
        match (&self.layout, self.linked.take()) {
            (Some(layout), Some(linked)) => {
                let (result, capture) = linked::result(layout, &linked, assembly);
                (result, Some(capture))
            }
            _ => (crate::AssemblyResult::from(assembly), None),
        }
    }
}

impl Dialect for Ca65Walk {
    fn cycle_coverage(&self) -> crate::dialect::CycleCoverage {
        crate::dialect::CycleCoverage::Full
    }

    /// ca65 refuses a negative value at every width; lowering marks its data
    /// unsigned, and this keeps the engine's other checks in step.
    fn accepts_negative_values(&self) -> bool {
        false
    }

    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::mos6502::SET
    }

    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        self.parse_warned(source).map(|(statements, _)| statements)
    }

    fn parse_warned(&self, source: &str) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
        let ran = Walk::default().run(source, self.layout.as_ref())?;
        Ok(self.keep(ran))
    }

    /// A program whose root is `FileId(0)` in `map`, reading `.include` and
    /// `.incbin` files through `loader`, from the requesting file's folder
    /// and then each folder of the files that included it, as ca65 does.
    fn parse_multi_warned(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
        let root = map.contents(FileId(0)).unwrap_or_default().to_owned();
        let walk = Walk {
            files: Some(Files { map, loader }),
            ..Walk::default()
        };
        let ran = walk.run(&root, self.layout.as_ref())?;
        Ok(self.keep(ran))
    }

    fn equ_label_colon(&self) -> bool {
        false
    }

    /// A layout's image starts at the start of the file.
    fn image_base(&self) -> Option<i64> {
        self.layout.as_ref().map(|_| 0)
    }

    /// A layout's image is its file areas, filled to their size.
    fn image_size(&self, _: &[u8]) -> Option<usize> {
        self.linked
            .borrow()
            .as_ref()
            .map(|linked| linked.resolved.image_size)
    }

    /// ca65 works out an equate of a name defined further on once that name
    /// is known.
    fn defers_equates(&self) -> bool {
        true
    }

    fn leaves_equate_unresolved(&self, label: &str) -> bool {
        self.lazy.borrow().contains(label)
    }

    /// A linked program's budgets are checked against its link.
    fn engine_checks_budgets(&self) -> bool {
        self.layout.is_none()
    }

    /// ca65 counts addresses in 24 bits, so `.org` may give a linked
    /// section any address below $1000000. A flat link is checked as the
    /// recorded verdicts were.
    fn address_bits(&self) -> u32 {
        if self.layout.is_some() { 24 } else { 16 }
    }

    /// ca65's `.res` with a value and `.align` are written in the source,
    /// so the bytes they make belong to their lines.
    fn fill_has_lines(&self) -> bool {
        true
    }

    fn gap_fill(&self) -> u8 {
        self.layout.as_ref().map_or(0, |layout| layout.fill)
    }
}

/// What reading a program gives.
struct Ran {
    statements: Vec<Statement>,
    /// What its link made, if it was linked.
    linked: Option<Linked>,
    /// What ca65 warned of.
    warnings: Vec<Warning>,
    /// The equates ca65 refuses as circular only where they are used.
    lazy: HashSet<String>,
}

/// A segment the program has used.
struct Seg {
    /// The address size its labels take.
    size: Size,
    /// How many bytes it holds so far.
    length: i64,
}

/// The address size ca65 gives one of its own segments, which a program
/// cannot change.
fn built_in_size(name: &str) -> Option<Size> {
    match name {
        "ZEROPAGE" => Some(Size::Zero),
        "CODE" | "RODATA" | "DATA" | "BSS" => Some(Size::Absolute),
        _ => None,
    }
}

/// A scope that is open.
struct Frame {
    /// Its full name.
    path: String,
    /// Its name as written, for messages: `None` if it has none.
    name: Option<String>,
    /// Whether `.proc` opened it.
    procedure: bool,
    /// How many bytes the program had made when it opened.
    start: i64,
}

/// A name written without a scope, in a scope that did not define it when
/// it was read. ca65 binds it at the end of the program.
struct Pending {
    /// The full name it has if its own scope defines it.
    here: String,
    /// Its scope.
    path: String,
    /// The name as written.
    name: String,
}

/// A name the program declared to a linker, in the scope it was declared
/// in.
struct Declaration {
    kind: Visible,
    /// Its address size, absolute unless one was written.
    size: Size,
    /// Its scope.
    path: String,
    /// The name as written.
    name: String,
    /// Where it was first declared.
    at: Span,
    /// How many names were declared before it.
    order: usize,
}

/// The files a program may read, for the multi-file entry points.
struct Files<'f> {
    map: &'f mut SourceMap,
    loader: &'f dyn SourceLoader,
}

/// What the walk knows as it reads the program.
#[derive(Default)]
struct Walk<'f> {
    /// The files the program may read, or `None` when it is one file.
    files: Option<Files<'f>>,
    /// Whether the program is linked from a layout rather than flat.
    linking: bool,
    /// For each file the expander read, by its number there: its id in the
    /// source map, and the file that read it in. The root is first.
    read: Vec<(FileId, Option<usize>)>,
    /// Each name declared to a linker, by its full name in the scope that
    /// declared it.
    declared: HashMap<String, Declaration>,
    /// The full names `.set` defined, which ca65 does not let a program
    /// export.
    variables: HashSet<String>,
    /// What is known of each name defined so far, by its full name.
    names: HashMap<String, Value>,
    /// Each equate whose value was not known when it was defined, by its
    /// full name, with the names it reads bound.
    equates: HashMap<String, Expr>,
    /// The equates being worked out, so one that reads itself stops.
    evaluating: RefCell<HashSet<String>>,
    /// The bytes each label's line made, by its full name, for `.sizeof`.
    sizes: HashMap<String, i64>,
    /// Every scope opened so far, by its full name, with its size once it
    /// has closed.
    scopes: HashMap<String, Option<i64>>,
    /// The open scopes, innermost last.
    open: Vec<Frame>,
    /// How many unnamed scopes have opened.
    unnamed_scopes: usize,
    /// Names read so far, for `.ifref`.
    referenced: HashSet<String>,
    /// Names `.ifconst` asked about, or lowering read, with where. ca65
    /// counts each as a use, so each must be defined by the end.
    asked: Vec<(String, Span)>,
    /// Names lowering has read, as written, since they were last recorded.
    reads: RefCell<Vec<String>>,
    /// Names bound at the end of the program.
    pending: Vec<Pending>,
    /// The segments the layout only reserves: `bss` and `zp`.
    reserving: HashSet<String>,
    /// What ca65 warned of so far.
    warnings: Vec<Warning>,
    /// Equates in a scope that read their own name while a name further
    /// out was defined. ca65 refuses the circle only where one is used.
    lazy: HashSet<String>,
    /// Where the line being read is, for what the host is asked about it.
    here: Option<Span>,
    /// The address, while `.org` makes it known. ca65 keeps it across a
    /// change of segment, counting every segment's bytes, until `.reloc`.
    org: Option<i64>,
    /// A section the line being read opens in a linked program: at the
    /// address `.org` gave, or where the segment has got to after `.reloc`.
    opens: Option<Option<i64>>,
    /// How many `PseudoPc` blocks `.org` has opened in a flat program.
    pseudo: usize,
    /// What `.reloc` leaves behind in a flat program: one end for each
    /// `PseudoPc` block.
    after: Vec<Operation>,
    /// How many bytes the program has made.
    made: i64,
    /// The full name of the last label that was not a cheap local, which a
    /// cheap local belongs to.
    cheap: String,
    /// How many unnamed labels have been defined.
    unnamed: usize,
    /// The segment lines go into.
    segment: String,
    /// Each segment used so far.
    segments: HashMap<String, Seg>,
    /// The segments something has been put in, by a label or a command.
    used: HashSet<String>,
    /// The segments `.pushseg` saved, last on top.
    saved: Vec<String>,
    /// Each statement that opens a section, with its segment, how far into
    /// the segment it starts, and the address `.org` gave it, if any, for
    /// placing at the end.
    sections: Vec<(usize, String, i64, Option<i64>)>,
}

/// `name` in the scope whose full name is `path`.
fn qualify(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_owned()
    } else {
        format!("{path}::{name}")
    }
}

/// `path` and each scope around it, out to the top, innermost first.
fn outwards(path: &str) -> impl Iterator<Item = &str> {
    let mut next = Some(path);
    std::iter::from_fn(move || {
        let path = next?;
        next = match path.rsplit_once("::") {
            Some((outer, _)) => Some(outer),
            None if path.is_empty() => None,
            None => Some(""),
        };
        Some(path)
    })
}

/// How a name as written is found.
enum Lookup {
    /// A full name: a scope was written, or it is a cheap local or an
    /// unnamed label.
    Full(String),
    /// A plain name, found from the innermost scope outwards.
    Plain(String),
}

impl Walk<'_> {
    /// The program's statements, what the link `layout` describes made of
    /// them, if there is a layout, and what ca65 warned of.
    fn run(mut self, source: &str, layout: Option<&Layout>) -> Result<Ran, AsmError> {
        let mut expander = Expander::new(source, PROFILE)
            .map_err(|_| AsmError::new(0, "the program is too long to parse"))?;
        self.read = vec![(FileId(0), None)];
        let result = self.read_lines(&mut expander, layout);
        // Recorded whether or not the program assembles, so a problem in an
        // included file can name the chain of files that read it.
        self.record_includes(&expander);
        let (statements, linked) = result?;
        Ok(Ran {
            statements,
            linked,
            warnings: self.warnings,
            lazy: self.lazy,
        })
    }

    /// Reads and lowers every line `expander` hands on.
    fn read_lines(
        &mut self,
        expander: &mut Expander,
        layout: Option<&Layout>,
    ) -> Result<(Vec<Statement>, Option<Linked>), AsmError> {
        let mut statements = Vec::new();
        let sectioned = layout.is_some();
        self.linking = sectioned;
        if let Some(layout) = layout {
            self.reserving = layout
                .segments
                .iter()
                .filter(|def| !def.initialized)
                .map(|def| def.name.clone())
                .collect();
        }
        let start = Span::in_file(FileId(0), 1, 1);
        self.use_segment("CODE", None, sectioned, &mut statements, &start)?;
        loop {
            let line = expander.next_line(self);
            self.check(expander)?;
            let Some(line) = line else { break };
            let at = self.site(expander, line.span());
            let made_at = self.statement_site(expander, line.span());
            self.here = Some(at.clone());
            let lowered = lower::line(&line, PROFILE, &*self);
            self.record_reads();
            let lowered = lowered.map_err(|diagnostic| {
                let span = syntax::Span::new(line.text(), diagnostic.range());
                let message = match diagnostic.kind() {
                    DiagnosticKind::Unsupported => {
                        format!("{} (Lang198x's ca65 front end)", diagnostic.message())
                    }
                    _ => diagnostic.message().to_owned(),
                };
                AsmError::at(self.site(expander, span), message)
            })?;
            let made = self.made;
            let mut labels = Vec::new();
            for item in lowered.items {
                // What a label is known as comes from the line as written,
                // before its names are bound.
                let value = item.op.as_ref().map(|op| self.value_of(op));
                let waits = item.op.as_ref().is_some_and(|op| self.reads_unknown(op));
                // ca65 reads an assignment's value before its name is
                // defined, which matters to a cheap local, and a label
                // before the rest of its line.
                let sized = !matches!(item.op, Some(Operation::Equ(_) | Operation::Set(_)));
                if let (Some(name), Some(op)) = (&item.label, &item.op)
                    && self.reads_itself_in_a_scope(name, op)
                {
                    self.lazy.insert(qualify(self.path(), name));
                }
                let (label, op) = if sized {
                    let label = item.label.map(|name| self.define_label(&name));
                    (label, item.op.map(|op| self.names_in(op)))
                } else {
                    let op = item.op.map(|op| self.names_in(op));
                    (item.label.map(|name| self.define_label(&name)), op)
                };
                let op = self.apply(label.as_deref(), value, waits, op, &at)?;
                if let (Some(label), true) = (&label, sized) {
                    labels.push(label.clone());
                }
                if sized && (label.is_some() || op.is_some()) {
                    self.used.insert(self.segment.clone());
                }
                statements.push(statement(&made_at, label, op));
                if let Some(address) = self.opens.take() {
                    self.open_section(address, &mut statements, &made_at);
                }
                for op in std::mem::take(&mut self.after) {
                    statements.push(statement(&made_at, None, Some(op)));
                }
            }
            if let Some(binary) = &lowered.binary {
                let op = self.binary(expander, &line, binary, &at)?;
                let op = self.apply(None, None, false, Some(op), &at)?;
                statements.push(statement(&made_at, None, op));
            }
            let line_size = self.made - made;
            for label in labels {
                self.sizes.insert(label, line_size);
            }
            if let Some(change) = lowered.scope {
                self.change_scope(change, &at)?;
            }
            if let Some(change) = lowered.segment {
                self.change_segment(change, sectioned, &mut statements, &at)?;
            }
            if let Some(declared) = lowered.visibility {
                self.declare(declared, &at)?;
            }
        }
        if let Some(frame) = self.open.last() {
            let name = frame.name.as_deref().unwrap_or("");
            return Err(AsmError::at(
                self.here
                    .clone()
                    .unwrap_or_else(|| Span::in_file(FileId(0), 0, 0)),
                format!("Local scope `{name}` was not closed"),
            ));
        }
        let bound = self.bind_pending();
        self.check_declarations()?;
        if !bound.is_empty() {
            for statement in &mut statements {
                if let Some(op) = statement.op.take() {
                    statement.op = Some(map_syms(op, &mut |name| {
                        Expr::Sym(bound.get(&name).cloned().unwrap_or(name))
                    }));
                }
            }
        }
        if let Some((name, at)) = self.asked.iter().find(|(name, _)| {
            let name = bound.get(name).unwrap_or(name);
            !self.names.contains_key(name)
        }) {
            return Err(AsmError::at(
                at.clone(),
                format!("Symbol `{name}` is undefined"),
            ));
        }
        let linked = self.place(layout, &mut statements)?;
        Ok((statements, linked))
    }

    /// Places each section from `layout`, now that every segment's length
    /// is known, and says what the link made. With no layout, the link is
    /// flat: `CODE` alone, at 0.
    fn place(
        &self,
        layout: Option<&Layout>,
        statements: &mut [Statement],
    ) -> Result<Option<Linked>, AsmError> {
        let unplaced = |name: &str| {
            AsmError::new(
                0,
                format!("Missing memory area assignment for segment `{name}`"),
            )
        };
        // ld65 places only the segments that hold bytes: an empty one it has
        // no area for is not an error.
        let placed = |name: &str| layout.is_some_and(|l| l.segments.iter().any(|d| d.name == name));
        if let Some((name, _)) = self.segments.iter().find(|(name, seg)| {
            seg.length > 0 && !placed(name) && (layout.is_some() || *name != "CODE")
        }) {
            return Err(unplaced(name));
        }
        let Some(layout) = layout else {
            return Ok(None);
        };
        let mut lengths = BTreeMap::new();
        for (name, seg) in &self.segments {
            if placed(name) {
                let length = u32::try_from(seg.length).map_err(|_| unplaced(name))?;
                lengths.insert(name.clone(), length);
            }
        }
        let resolved = layout.resolve(&lengths)?;
        layout.validate_extents(&resolved, &lengths)?;
        let mut opened = Vec::new();
        for (index, name, offset, org) in &self.sections {
            let Some(seg) = resolved.seg(name) else {
                // An empty segment with no area: nothing to place.
                if let Some(statement) = statements.get_mut(*index) {
                    statement.op = None;
                }
                continue;
            };
            let at = match seg.file_at {
                Some(file_at) => Place::At(file_at as i64 + offset),
                None => Place::Discard,
            };
            opened.push(Opened {
                segment: name.clone(),
                offset: *offset,
                org: *org,
            });
            if let Some(statement) = statements.get_mut(*index) {
                statement.op = Some(Operation::Section {
                    name: name.clone(),
                    base: Some(org.unwrap_or(i64::from(seg.base) + offset)),
                    at,
                    bank: None,
                });
            }
        }
        // What the program put nothing in is not part of its link's record.
        lengths.retain(|name, _| self.used.contains(name));
        Ok(Some(Linked {
            resolved,
            lengths,
            opened,
        }))
    }

    /// How far into the current segment the next byte goes.
    fn offset(&self) -> i64 {
        self.segments.get(&self.segment).map_or(0, |seg| seg.length)
    }

    /// The address size of a label in the current segment.
    fn segment_size(&self) -> Size {
        self.segments
            .get(&self.segment)
            .map_or(Size::Absolute, |seg| seg.size)
    }

    /// Makes `name` the segment later lines go into. With a layout, a new
    /// section opens where the segment left off, placed at the end.
    fn use_segment(
        &mut self,
        name: &str,
        size: Option<Size>,
        sectioned: bool,
        statements: &mut Vec<Statement>,
        at: &Span,
    ) -> Result<(), AsmError> {
        let mismatch = || AsmError::at(at.clone(), "Segment attribute mismatch");
        match (self.segments.get(name), size) {
            (Some(seg), Some(size)) if seg.size != size => return Err(mismatch()),
            (Some(_), _) => {}
            (None, size) => {
                let size = match (built_in_size(name), size) {
                    (Some(fixed), Some(size)) if fixed != size => return Err(mismatch()),
                    (Some(fixed), _) => fixed,
                    (None, size) => size.unwrap_or(Size::Absolute),
                };
                self.segments
                    .insert(name.to_owned(), Seg { size, length: 0 });
            }
        }
        if self.segment == name {
            return Ok(());
        }
        self.segment = name.to_owned();
        if sectioned {
            self.open_section(self.org, statements, at);
        }
        Ok(())
    }

    /// Opens a section in the current segment where it has got to, placed
    /// at the end. `org` is the address `.org` gave it, if it gave one.
    fn open_section(&mut self, org: Option<i64>, statements: &mut Vec<Statement>, at: &Span) {
        statements.push(statement(
            at,
            None,
            Some(Operation::Section {
                name: self.segment.clone(),
                base: None,
                at: Place::ByAddress,
                bank: None,
            }),
        ));
        self.sections.push((
            statements.len() - 1,
            self.segment.clone(),
            self.offset(),
            org,
        ));
    }

    /// `.segment`, its short forms, `.pushseg` or `.popseg`.
    fn change_segment(
        &mut self,
        change: Segment,
        sectioned: bool,
        statements: &mut Vec<Statement>,
        at: &Span,
    ) -> Result<(), AsmError> {
        match change {
            Segment::Use { name, size } => self.use_segment(&name, size, sectioned, statements, at),
            Segment::Push => {
                self.saved.push(self.segment.clone());
                Ok(())
            }
            Segment::Pop => {
                let name = self
                    .saved
                    .pop()
                    .ok_or_else(|| AsmError::at(at.clone(), "Segment stack is empty"))?;
                self.use_segment(&name, None, sectioned, statements, at)
            }
            _ => Err(AsmError::at(
                at.clone(),
                "this change of segment is not supported",
            )),
        }
    }

    /// The full name of the innermost open scope: empty at the top.
    fn path(&self) -> &str {
        self.open.last().map_or("", |frame| frame.path.as_str())
    }

    /// The scope a qualified name starts with, found from the innermost
    /// scope outwards. ca65 takes one not opened yet to be in this scope.
    fn scope_named(&self, first: &str) -> String {
        outwards(self.path())
            .map(|path| qualify(path, first))
            .find(|full| self.scopes.contains_key(full))
            .unwrap_or_else(|| qualify(self.path(), first))
    }

    /// How `name`, as written, is found here.
    fn lookup(&self, name: &str) -> Lookup {
        if let Some(rest) = name.strip_prefix(':')
            && !rest.starts_with(':')
        {
            let back = rest.chars().filter(|&c| c == '-').count();
            let forward = rest.chars().filter(|&c| c == '+').count();
            let index = if back > 0 {
                self.unnamed.checked_sub(back)
            } else {
                Some(self.unnamed + forward.saturating_sub(1))
            };
            return Lookup::Full(match index {
                Some(index) => unnamed(index),
                // No unnamed label that far back: a name nothing defines,
                // so the engine reports it.
                None => name.to_owned(),
            });
        }
        if let Some(rest) = name.strip_prefix("::") {
            return Lookup::Full(rest.to_owned());
        }
        if let Some((first, rest)) = name.split_once("::") {
            return Lookup::Full(format!("{}::{rest}", self.scope_named(first)));
        }
        if name.starts_with('@') {
            return Lookup::Full(qualify(self.path(), &format!("{}{name}", self.cheap)));
        }
        Lookup::Plain(name.to_owned())
    }

    /// The innermost full name of plain `name` that is defined, from this
    /// scope outwards.
    fn defined(&self, name: &str) -> Option<String> {
        outwards(self.path())
            .map(|path| qualify(path, name))
            .find(|full| self.names.contains_key(full))
    }

    /// The full name a reference to `name` stands for, recorded as read. A
    /// plain name this scope has not defined is bound at the end.
    fn reference(&mut self, name: &str) -> String {
        let full = match self.lookup(name) {
            Lookup::Full(full) => full,
            Lookup::Plain(name) => {
                let here = qualify(self.path(), &name);
                if !self.path().is_empty() && !self.names.contains_key(&here) {
                    self.pending.push(Pending {
                        here: here.clone(),
                        path: self.path().to_owned(),
                        name,
                    });
                }
                here
            }
        };
        self.referenced.insert(full.clone());
        full
    }

    /// Records where each file was included from. Lowering reads a file
    /// before it knows the line that asked, so this is done at the end.
    fn record_includes(&mut self, expander: &Expander) {
        let sites: Vec<(FileId, Span)> = expander
            .files()
            .iter()
            .filter_map(|file| {
                let number = usize::try_from(file.id().get()).ok()?;
                let id = self.read.get(number)?.0;
                Some((id, self.site(expander, file.site())))
            })
            .collect();
        if let Some(files) = &mut self.files {
            for (id, site) in sites {
                files.map.set_included_from(id, site.file, site.line);
            }
        }
    }

    /// Stops at the expander's first problem.
    fn check(&self, expander: &Expander) -> Result<(), AsmError> {
        match expander.diagnostics().first() {
            Some((text, diagnostic)) => {
                let span = syntax::Span::new(*text, diagnostic.range());
                Err(AsmError::at(
                    self.site(expander, span),
                    diagnostic.message(),
                ))
            }
            None => Ok(()),
        }
    }

    /// The line a statement made from `span` belongs to: where it was
    /// written, or for a line a macro made, the outermost call, as ca65's
    /// debug information gives it. A repeat's lines stay its own.
    fn statement_site(&self, expander: &Expander, span: syntax::Span) -> Span {
        self.site(expander, expander.macro_call(span).unwrap_or(span))
    }

    /// Where `span` was written: its file, line and column.
    fn site(&self, expander: &Expander, span: syntax::Span) -> Span {
        match expander.locate(span) {
            Some(at) => Span::in_file(self.file_id(at.file.get()), at.line, at.column),
            None => Span::in_file(FileId(0), 0, 0),
        }
    }

    /// The source map's id for the expander's file `number`.
    fn file_id(&self, number: u32) -> FileId {
        usize::try_from(number)
            .ok()
            .and_then(|n| self.read.get(n))
            .map_or(FileId(0), |(id, _)| *id)
    }

    /// The files a request from the expander's file `number` is looked for
    /// beside, as `load_include` takes them: the root first and that file
    /// last.
    fn chain(&self, number: usize) -> Vec<FileId> {
        let mut chain = Vec::new();
        let mut at = Some(number);
        while let Some(n) = at {
            let Some(&(id, parent)) = self.read.get(n) else {
                break;
            };
            chain.push(id);
            at = parent;
        }
        chain.reverse();
        chain
    }

    /// The bytes `.incbin` puts here, read beside the file the line is in.
    fn binary(
        &self,
        expander: &Expander,
        line: &ActiveLine,
        binary: &Binary,
        at: &Span,
    ) -> Result<Operation, AsmError> {
        let Some(files) = &self.files else {
            return Err(AsmError::at(
                at.clone(),
                format!(
                    "cannot resolve `.incbin \"{}\"` here — the single-source API assembles \
                     one file; use the multi-file entry point (the CLI resolves binary \
                     inclusions automatically)",
                    binary.name
                ),
            ));
        };
        let number = expander
            .locate(line.span())
            .and_then(|at| usize::try_from(at.file.get()).ok())
            .unwrap_or(0);
        let chain = self.chain(number);
        let data = load_binary(
            files.map,
            files.loader,
            &binary.name,
            &chain,
            Resolution::AncestorChain,
        )
        .map_err(|error| AsmError::at(at.clone(), error.to_string()))?;
        let bytes = slice_incbin(&data, binary.offset, binary.size)
            .map_err(|message| AsmError::at(at.clone(), format!("`{}`: {message}", binary.name)))?;
        Ok(Operation::Binary(bytes))
    }

    /// Records the names lowering read as uses, each of which must be
    /// defined by the end. A name only `.const` read is in no statement, so
    /// this is the only record of it.
    fn record_reads(&mut self) {
        let here = self
            .here
            .clone()
            .unwrap_or_else(|| Span::in_file(FileId(0), 0, 0));
        for name in self.reads.take() {
            let full = self.reference(&name);
            self.asked.push((full, here.clone()));
        }
    }

    /// The full name a read of `name` is recorded under, as `reference`
    /// records it.
    fn read_as(&self, name: &str) -> String {
        match self.lookup(name) {
            Lookup::Full(full) => full,
            Lookup::Plain(name) => qualify(self.path(), &name),
        }
    }

    /// Whether `name` has been read: before this line, or on it so far.
    fn was_read(&self, name: &str) -> bool {
        let read = |full: &str| {
            self.referenced.contains(full)
                || self.reads.borrow().iter().any(|r| self.read_as(r) == full)
        };
        match self.lookup(name) {
            Lookup::Full(full) => read(&full),
            Lookup::Plain(name) => outwards(self.path()).any(|path| read(&qualify(path, &name))),
        }
    }

    /// Whether `name` is defined so far.
    fn has(&self, name: &str) -> bool {
        match self.lookup(name) {
            Lookup::Full(full) => self.names.contains_key(&full),
            Lookup::Plain(name) => self.defined(&name).is_some(),
        }
    }

    /// `op` with each name it reads bound, and recorded as read.
    fn names_in(&mut self, op: Operation) -> Operation {
        map_syms(op, &mut |name| Expr::Sym(self.reference(&name)))
    }

    /// The full name a label defines, which moves the cheap-local scope or
    /// counts an unnamed label.
    fn define_label(&mut self, name: &str) -> String {
        if name == ":" {
            self.unnamed += 1;
            return unnamed(self.unnamed - 1);
        }
        let full = match self.lookup(name) {
            Lookup::Full(full) => full,
            Lookup::Plain(name) => qualify(self.path(), &name),
        };
        if !name.starts_with('@') {
            self.cheap.clone_from(&full);
        }
        full
    }

    /// What a label on `op`'s line is known as: the value of `name = value`,
    /// or the address.
    fn value_of(&self, op: &Operation) -> Value {
        match op {
            Operation::Equ(value) | Operation::Set(value) => lower::value(value, self),
            _ => self.pc(),
        }
    }

    /// Whether `op` is an equate in a scope that reads its own name `name`,
    /// written plain, while a name further out is defined.
    fn reads_itself_in_a_scope(&self, name: &str, op: &Operation) -> bool {
        let Operation::Equ(value) = op else {
            return false;
        };
        if self.path().is_empty() || !matches!(self.lookup(name), Lookup::Plain(_)) {
            return false;
        }
        let mut itself = false;
        map_sym_expr(value.clone(), &mut |read| {
            itself |= read == name;
            Expr::Sym(read)
        });
        itself && self.defined(name).is_some()
    }

    /// Whether `op` is an equate that reads a name nothing is known of yet.
    fn reads_unknown(&self, op: &Operation) -> bool {
        let Operation::Equ(value) = op else {
            return false;
        };
        let mut unknown = false;
        map_sym_expr(value.clone(), &mut |name| {
            unknown |= matches!(self.name(&name), Value::Unknown);
            Expr::Sym(name)
        });
        unknown
    }

    /// Records what a statement tells the walk: its label's value, and the
    /// address. `.org` opens a section in a linked program, and moves the
    /// engine's `PseudoPc` in a flat one. `waits` says the label is an
    /// equate that read a name nothing was known of yet.
    fn apply(
        &mut self,
        label: Option<&str>,
        value: Option<Value>,
        waits: bool,
        op: Option<Operation>,
        at: &Span,
    ) -> Result<Option<Operation>, AsmError> {
        let refuse = |message: String| AsmError::at(at.clone(), message);
        if let Some(label) = label {
            let value = value.unwrap_or_else(|| self.pc());
            let redefinable = matches!(op, Some(Operation::Set(_)));
            if !redefinable && self.names.contains_key(label) {
                return Err(refuse(format!("Symbol `{label}` is already defined")));
            }
            if let Some(declaration) = self.declared.get(label) {
                if is_import(declaration.kind) {
                    let name = &declaration.name;
                    return Err(refuse(format!("Symbol `{name}` is already an import")));
                }
                if redefinable && declaration.kind == Visible::Export {
                    let name = &declaration.name;
                    return Err(refuse(format!(
                        "Var symbol `{name}` cannot be an export or global symbol"
                    )));
                }
            }
            if redefinable {
                self.variables.insert(label.to_owned());
            }
            self.names.insert(label.to_owned(), value);
            // ca65 works out what an equate is when it is used, so one of a
            // name defined further on is known once that name is.
            match &op {
                Some(Operation::Equ(e)) if waits => {
                    self.equates.insert(label.to_owned(), e.clone());
                }
                _ => {
                    self.equates.remove(label);
                }
            }
            self.check_zero_page(label, at);
        }
        let op = match op {
            Some(Operation::Org(address)) => {
                let value = match lower::value(&address, self) {
                    Value::Constant(value) => value,
                    _ => return Err(refuse("Constant expression expected".to_owned())),
                };
                // ca65 keeps addresses in 24 bits.
                if !(0..=0xFF_FFFF).contains(&value) {
                    return Err(refuse("Range error".to_owned()));
                }
                self.org = Some(value);
                // A linked program's sections carry their own addresses, so
                // the address `.org` gives opens one. A flat program has no
                // sections and moves only what addresses read as.
                if self.linking {
                    self.opens = Some(Some(value));
                    return Ok(None);
                }
                self.pseudo += 1;
                return Ok(Some(Operation::PseudoPc(Some(Expr::Num(value)))));
            }
            // `.reloc` ends what `.org` began: addresses count from where the
            // segment has got to again.
            Some(Operation::PseudoPc(None)) => {
                if self.org.take().is_some() && self.linking {
                    self.opens = Some(None);
                }
                self.after = vec![Operation::PseudoPc(None); std::mem::take(&mut self.pseudo)];
                return Ok(None);
            }
            // ld65 drops what a `bss` or `zp` segment holds, and warns of
            // what the program wrote there. A reservation is not written,
            // so it makes zeros, whatever the layout fills with.
            Some(Operation::Reserve(count)) if self.reserving.contains(&self.segment) => {
                Some(Operation::Fill {
                    count: Expr::Num(count as i64),
                    value: 0,
                })
            }
            // ca65 aligns within the segment, wherever ld65 places it, so
            // `.align 3` in a segment at $8000 pads to offset 3.
            Some(Operation::AlignTo { modulus, fill }) => Some(Operation::Fill {
                count: Expr::Num(align_pad(self.offset(), modulus)),
                value: fill.first().copied().unwrap_or(0),
            }),
            other => other,
        };
        if let Some(op) = &op
            && !matches!(op, Operation::Equ(_) | Operation::Set(_))
        {
            let pc = self.org.unwrap_or_else(|| self.offset());
            let next = next_pc(op, pc, &isa::mos6502::SET, None, 1, at.line as usize)
                .map_err(|error| refuse(error.message))?;
            self.made += next - pc;
            if let Some(seg) = self.segments.get_mut(&self.segment) {
                seg.length += next - pc;
            }
            if self.org.is_some() {
                self.org = Some(next);
            }
        }
        Ok(op)
    }

    /// Declares names to a linker. Each is declared in the current scope,
    /// and declaring one again must agree with what was declared before.
    fn declare(&mut self, declared: Visibility, at: &Span) -> Result<(), AsmError> {
        let refuse = |message: String| AsmError::at(at.clone(), message);
        let kind = declared.kind;
        for name in declared.names {
            let size = name.size.unwrap_or(Size::Absolute);
            let full = qualify(self.path(), &name.name);
            let name = name.name;
            if is_import(kind) && self.names.contains_key(&full) {
                return Err(refuse(format!("Symbol `{name}` is already defined")));
            }
            if kind == Visible::Export && self.variables.contains(&full) {
                return Err(refuse(format!("Var symbol `{name}` cannot be exported")));
            }
            if kind == Visible::Export {
                // ca65 counts an export as a use of the name.
                self.referenced.insert(full.clone());
            }
            let check = full.clone();
            match self.declared.get_mut(&full) {
                None => {
                    let path = self.path().to_owned();
                    let order = self.declared.len();
                    self.declared.insert(
                        full,
                        Declaration {
                            kind,
                            size,
                            path,
                            name,
                            at: at.clone(),
                            order,
                        },
                    );
                }
                Some(before) => {
                    if is_import(kind) && before.kind == Visible::Export {
                        return Err(refuse(format!("Cannot import exported symbol `{name}`")));
                    }
                    if kind == Visible::Export && is_import(before.kind) {
                        return Err(refuse(format!("Symbol `{name}` is already an import")));
                    }
                    if before.size != size {
                        return Err(refuse(format!("Address size mismatch for symbol `{name}`")));
                    }
                    // `.global` takes the kind of a declaration beside it.
                    if before.kind == Visible::Global
                        || (before.kind == Visible::Import && kind == Visible::ForceImport)
                    {
                        before.kind = kind;
                    }
                }
            }
            self.check_zero_page(&check, at);
        }
        Ok(())
    }

    /// Warns, as ca65 does, of a name exported to the zero page whose
    /// address size is absolute, once both its declaration and its
    /// definition have been read. A `.globalzp` the program defines is an
    /// export. A constant warns by its size, as a label does.
    fn check_zero_page(&mut self, full: &str, at: &Span) {
        let Some(declaration) = self.declared.get(full) else {
            return;
        };
        if declaration.size != Size::Zero
            || !matches!(declaration.kind, Visible::Export | Visible::Global)
        {
            return;
        }
        let size = match self.names.get(full) {
            None => return,
            Some(Value::Constant(value)) => Size::of(*value),
            Some(Value::Address(size)) => *size,
            Some(_) => Size::Absolute,
        };
        if size != Size::Zero {
            let name = &declaration.name;
            self.warnings.push(
                Warning::new(
                    at.line as usize,
                    format!("Symbol `{name}` is absolute but exported zeropage"),
                )
                .in_file(at.file),
            );
        }
    }

    /// At the end, what ld65 checks of the declarations in one program. An
    /// export must be defined, in its own scope or one further out, and not
    /// by `.set`. A name forced in is never defined, since defining an
    /// import is refused. A `.global` the program defined is an export, and
    /// any other import is resolved only if it is used, which the engine
    /// refuses as an undefined name.
    fn check_declarations(&self) -> Result<(), AsmError> {
        let mut declarations: Vec<&Declaration> = self.declared.values().collect();
        declarations.sort_by_key(|d| d.order);
        for declaration in declarations {
            let name = &declaration.name;
            let refuse = |message: String| AsmError::at(declaration.at.clone(), message);
            let defined = outwards(&declaration.path)
                .map(|path| qualify(path, name))
                .find(|full| self.names.contains_key(full));
            match (declaration.kind, defined) {
                (Visible::ForceImport, _) => {
                    return Err(refuse(format!("Unresolved external `{name}`")));
                }
                (Visible::Export, None) => {
                    return Err(refuse(format!(
                        "Exported symbol `{name}` was never defined"
                    )));
                }
                (Visible::Export | Visible::Global, Some(full))
                    if self.variables.contains(&full) =>
                {
                    return Err(refuse(format!("Var symbol `{name}` cannot be exported")));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Opens or closes a scope.
    fn change_scope(&mut self, change: Scope, at: &Span) -> Result<(), AsmError> {
        let refuse = |message: String| AsmError::at(at.clone(), message);
        match change {
            Scope::Open { name, procedure } => {
                let path = match &name {
                    Some(name) => qualify(self.path(), name),
                    None => {
                        self.unnamed_scopes += 1;
                        qualify(self.path(), &self.unnamed_scopes.to_string())
                    }
                };
                if self.scopes.contains_key(&path) {
                    let name = name.as_deref().unwrap_or("");
                    return Err(refuse(format!("Duplicate scope `{name}`")));
                }
                self.scopes.insert(path.clone(), None);
                self.open.push(Frame {
                    path,
                    name,
                    procedure,
                    start: self.made,
                });
            }
            Scope::Close { procedure } => {
                let frame = match self.open.pop() {
                    Some(frame) if frame.procedure == procedure => frame,
                    _ => {
                        let word = if procedure { ".PROC" } else { ".SCOPE" };
                        return Err(refuse(format!("No open {word}")));
                    }
                };
                self.scopes
                    .insert(frame.path, Some(self.made - frame.start));
            }
            _ => return Err(refuse("this change of scope is not supported".to_owned())),
        }
        Ok(())
    }

    /// Binds each pending name to the innermost scope around its reference
    /// that defined it, as ca65 does at the end. Returns the names that bind
    /// further out than their own scope.
    fn bind_pending(&self) -> HashMap<String, String> {
        let mut bound = HashMap::new();
        for pending in &self.pending {
            if self.names.contains_key(&pending.here) {
                continue;
            }
            if let Some(full) = outwards(&pending.path)
                .skip(1)
                .map(|path| qualify(path, &pending.name))
                .find(|full| self.names.contains_key(full))
            {
                bound.insert(pending.here.clone(), full);
            }
        }
        bound
    }
}

/// Whether `kind` asks a linker for the names.
fn is_import(kind: Visible) -> bool {
    matches!(kind, Visible::Import | Visible::ForceImport)
}

/// The name the `index`th unnamed label defines. ca65 gives them no names a
/// person can write, and neither does this.
fn unnamed(index: usize) -> String {
    format!(":{index}")
}

impl Walk<'_> {
    /// What is known of the name whose full name is `full`. An equate whose
    /// value was not known when it was defined is worked out again, from
    /// the names it read.
    fn known(&self, full: &str) -> Value {
        let Some(&value) = self.names.get(full) else {
            return Value::Unknown;
        };
        let Some(expr) = self.equates.get(full) else {
            return value;
        };
        if !self.evaluating.borrow_mut().insert(full.to_owned()) {
            return value;
        }
        let now = lower::value(expr, &FullNames(self));
        self.evaluating.borrow_mut().remove(full);
        now
    }
}

/// The walk, asked about names that are already full names, as the names
/// an equate read are once they are bound.
struct FullNames<'w, 'f>(&'w Walk<'f>);

impl Known for FullNames<'_, '_> {
    fn name(&self, name: &str) -> Value {
        self.0.known(name)
    }

    fn pc(&self) -> Value {
        self.0.pc()
    }

    fn size_of(&self, name: &str) -> Option<i64> {
        self.0
            .scopes
            .get(name)
            .copied()
            .flatten()
            .or_else(|| self.0.sizes.get(name).copied())
    }

    fn is_defined(&self, name: &str) -> bool {
        self.0.names.contains_key(name)
    }

    fn is_referenced(&self, name: &str) -> bool {
        self.0.referenced.contains(name)
    }

    /// The names were read when the equate was.
    fn read(&self, _: &str) {}
}

impl Known for Walk<'_> {
    /// A plain name this scope has not defined is known by its address size
    /// only, from the innermost scope further out that has.
    fn name(&self, name: &str) -> Value {
        match self.lookup(name) {
            Lookup::Full(full) => self.known(&full),
            Lookup::Plain(name) => match self.defined(&name) {
                Some(full) if full == qualify(self.path(), &name) => self.known(&full),
                Some(full) => match self.known(&full) {
                    Value::Constant(value) => Value::Address(Size::of(value)),
                    other => other,
                },
                None => Value::Unknown,
            },
        }
    }

    /// The address under `.org`, and otherwise its segment's address size.
    fn pc(&self) -> Value {
        match self.org {
            Some(pc) => Value::Constant(pc),
            None => Value::Address(self.segment_size()),
        }
    }

    /// A scope's size once it has closed, and otherwise a label's. ca65
    /// looks for a scope first.
    fn size_of(&self, name: &str) -> Option<i64> {
        let full = match self.lookup(name) {
            Lookup::Full(full) => full,
            Lookup::Plain(name) => {
                let scope = outwards(self.path())
                    .map(|path| qualify(path, &name))
                    .find(|full| self.scopes.contains_key(full));
                match scope {
                    Some(scope) => return self.scopes[&scope],
                    None => self.defined(&name)?,
                }
            }
        };
        match self.scopes.get(&full) {
            Some(size) => *size,
            None => self.sizes.get(&full).copied(),
        }
    }

    fn is_defined(&self, name: &str) -> bool {
        self.has(name)
    }

    fn is_referenced(&self, name: &str) -> bool {
        self.was_read(name)
    }

    fn read(&self, name: &str) {
        self.reads.borrow_mut().push(name.to_owned());
    }
}

impl Host for Walk<'_> {
    /// A file `.include` reads, looked for beside the file that asks and
    /// then beside each file that included it. A single file has none.
    fn include(&mut self, name: &str, from: syntax::FileId) -> Option<String> {
        let from = usize::try_from(from.get()).ok()?;
        let chain = self.chain(from);
        let files = self.files.as_mut()?;
        let id = load_include(
            files.map,
            files.loader,
            name,
            &chain,
            0,
            Resolution::AncestorChain,
        )
        .ok()?;
        let text = files.map.contents(id)?.to_owned();
        self.read.push((id, Some(from)));
        Some(text)
    }

    /// A condition or count, which ca65 needs now: a constant expression.
    fn evaluate(&mut self, expr: &SourceExpr) -> Result<i64, String> {
        let lowered = lower::expression(expr, &*self);
        self.record_reads();
        let lowered = lowered.map_err(|d| d.message().to_owned())?;
        match lower::value(&lowered, &*self) {
            Value::Constant(value) => Ok(value),
            _ => Err("Constant expression expected".to_owned()),
        }
    }

    fn is_defined(&mut self, name: &NameRef) -> bool {
        self.has(&name.text())
    }

    fn is_referenced(&mut self, name: &NameRef) -> bool {
        self.was_read(&name.text())
    }

    /// `.ifconst`, which ca65 counts as a use of each name it reads.
    fn is_constant(&mut self, expr: &SourceExpr) -> bool {
        self.evaluate(expr).is_ok()
    }
}

fn statement(at: &Span, label: Option<String>, op: Option<Operation>) -> Statement {
    Statement {
        line: at.line as usize,
        file: at.file,
        label,
        op,
        operand_span: Some(at.clone()),
        xor_mask: 0,
        instruction_set: None,
        extension_set: None,
    }
}
