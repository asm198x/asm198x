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

use crate::directives::Category;
use lang198x_ca65::ast::{Expr as SourceExpr, NameRef, View};
use lang198x_ca65::expand::{ActiveLine, Expander, Host, Location, Reading};
use lang198x_ca65::lower::{
    self, Binary, CpuStack, Known, Record, RecordKind, Scope, Segment, Size, Value, Visibility,
    Visible,
};
use lang198x_ca65::syntax::{self, DiagnosticKind, TextId, TextRange};

use super::ca65_flat::{Resolution, load_binary, load_include, slice_incbin};
use super::ca65_layout::{Layout, ResolvedLayout};
use crate::ast::{map_sym_expr, map_syms};
use crate::dialect::Dialect;
use crate::engine::{
    AsmError, BinOp, DiagSeverity, Expr, Operation, Place, Statement, Warning, align_pad, next_pc,
};
use crate::source::{SourceLoader, SourceMap};
use crate::span::{ExpansionFrame, FileId, Span};

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
    /// ca65 reports every error, and stops only at `.fatal`.
    fn reports_every_error(&self) -> bool {
        true
    }

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
    /// Its number, in the order the program first used it.
    number: u32,
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
    /// Whether a record opened it, which only the record's end closes.
    record: bool,
    /// How many bytes the program had made when it opened.
    start: i64,
}

impl RecordFrame {
    /// Takes `bytes` for a member: a struct's next member follows it, and
    /// a union is as big as its biggest member.
    fn take(&mut self, bytes: i64) {
        if self.kind == RecordKind::Union {
            self.size = self.size.max(bytes);
        } else {
            self.offset += bytes;
            self.size += bytes;
        }
    }
}

/// A record being read: a `.struct`, `.union` or `.enum`.
struct RecordFrame {
    kind: RecordKind,
    /// Whether it has a name, and so opened a scope.
    scoped: bool,
    /// Where its next member goes. A union's members all go here.
    offset: i64,
    /// How big it is so far.
    size: i64,
    /// The full name of the member whose storage it is, in the record
    /// around it.
    field: Option<String>,
    /// An enum's last value written, and how many members have followed it.
    base: Expr,
    count: i64,
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
    /// Its address size as an import or `.global`: the size written, or
    /// its segment's. An export alone gives it none.
    size: Option<Size>,
    /// The address size written for it as an export, if one was.
    export: Option<Size>,
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
    /// Every equate's value, with the names it reads bound, and where it
    /// was defined, for the circles ca65 finds as it reads a value.
    definitions: HashMap<String, (Expr, Span)>,
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
    /// How many CPUs `.pushcpu` has kept. ca65 keeps eight at most.
    cpus: usize,
    /// The records being read, innermost last.
    records: Vec<RecordFrame>,
    /// The errors found so far. ca65 reports each and carries on.
    errors: Vec<AsmError>,
    /// How many of the expander's problems are among `errors`.
    expander_seen: usize,
    /// Each one-byte value whose size ca65 checks at the end, once every
    /// name is known: a `.byte`'s values and an immediate operand, with the
    /// address of its line and where it is.
    fits: Vec<(Expr, Value, Span, Fit)>,
    /// The full names of the scopes structs and unions opened, which
    /// `.tag` takes.
    record_scopes: HashSet<String>,
    /// Equates in a scope that read their own name while a name further
    /// out was defined. ca65 refuses the circle only where one is used.
    lazy: HashSet<String>,
    /// Where the line being read is, for what the host is asked about it.
    here: Option<Span>,
    /// The full name the line being lowered gives a value to. ca65 makes
    /// the name before it reads the value.
    binding: Option<String>,
    /// Whether the whole program has been read, when what is left of each
    /// `.global` is an import.
    ended: bool,
    /// Why the last `.include` that failed could not be read.
    missing_include: Option<String>,
    /// The CPU the line being read is for, once one is.
    cpu: Option<lang198x_ca65::Cpu>,
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

/// A full name as ca65 writes it in a message: a cheap local by its own
/// name, without the label it belongs to.
fn as_written(full: &str) -> &str {
    let last = full.rsplit_once("::").map_or(full, |(_, last)| last);
    last.find('@').map_or(full, |at| &last[at..])
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
            let before = self.errors.len();
            let found = self.gather_expander(expander);
            let Some(line) = line else { break };
            // A word ca65 has that the walk does not implement, or that ca65
            // itself refuses, is reported as the declared surface says, in
            // place of whatever reading it found.
            if let Some(message) = surface_refusal(&line) {
                let range = line.line().syntax().text_range();
                let mut index = before;
                for (text, at) in &found {
                    if *text == line.text() && range.contains_range(*at) {
                        self.errors.remove(index);
                    } else {
                        index += 1;
                    }
                }
                let at = self.error_site(expander, line.span());
                self.errors.push(AsmError::at(at, message));
                continue;
            }
            if let Some(name) = unknown_word(&line) {
                let message = format!(
                    "unknown instruction `{name}`: a name at the start of a line \
                     needs a `:` after it to make it a label"
                );
                for error in &mut self.errors[before..] {
                    if error.message == UNLABELLED {
                        error.message.clone_from(&message);
                    }
                }
            }
            // ca65 abandons a line it cannot read, reports the error and
            // carries on with the next one.
            let range = line.line().syntax().text_range();
            if found
                .iter()
                .any(|(text, at)| *text == line.text() && range.contains_range(*at))
            {
                continue;
            }
            if let Err(error) = self.read_line(expander, &line, sectioned, &mut statements) {
                self.errors.push(error);
            }
        }
        // ca65 warns at the last line it read.
        if self.cpus > 0
            && let Some(at) = &self.here
        {
            self.warnings.push(
                Warning::new(at.line as usize, "CPU stack is not empty".to_owned())
                    .in_file(at.file),
            );
        }
        let last = self
            .here
            .clone()
            .unwrap_or_else(|| Span::in_file(FileId(0), 0, 0));
        if let Some(frame) = self.records.last() {
            let end = match frame.kind {
                RecordKind::Struct => "Expected `.ENDSTRUCT` but found end-of-file",
                RecordKind::Union => "Expected `.ENDUNION` but found end-of-file",
                _ => "`.ENDENUM` expected but found end-of-file",
            };
            self.errors.push(AsmError::at(last.clone(), end));
        } else if let Some(frame) = self.open.last() {
            let name = frame.name.as_deref().unwrap_or("");
            self.errors.push(AsmError::at(
                last,
                format!("Local scope `{name}` was not closed"),
            ));
        }
        // ca65 checks the names and the data at the end only if it found
        // no error on the way, and the data only if the names are sound.
        self.stop_if_errors()?;
        let bound = self.bind_pending();
        // The names a use of an import stands for once it is linked. Its
        // size is still the one declared for it.
        let mut linked = bound.clone();
        linked.extend(self.link_imports());
        if let Err(error) = self.check_declarations() {
            self.errors.push(error);
        }
        if !linked.is_empty() {
            for statement in &mut statements {
                if let Some(op) = statement.op.take() {
                    statement.op = Some(map_syms(op, &mut |name| {
                        Expr::Sym(linked.get(&name).cloned().unwrap_or(name))
                    }));
                }
            }
        }
        // Each name used but never defined, once, where it was first used.
        let mut undefined = HashSet::new();
        for (name, at) in &self.asked {
            let full = linked.get(name).unwrap_or(name);
            if !self.names.contains_key(full) && undefined.insert(full.clone()) {
                self.errors.push(AsmError::at(
                    at.clone(),
                    format!("Symbol `{}` is undefined", as_written(name)),
                ));
            }
        }
        self.stop_if_errors()?;
        self.ended = true;
        self.check_fits(&bound);
        self.stop_if_errors()?;
        let linked = self.place(layout, &mut statements)?;
        Ok((statements, linked))
    }

    /// Keeps each value of `op` that takes one byte, for [`check_fits`].
    ///
    /// [`check_fits`]: Self::check_fits
    fn note_fits(&mut self, op: &Operation, at: &Span) {
        let pc = self.pc();
        let (values, distance): (&[Expr], Option<&Expr>) = match op {
            Operation::Sized {
                width: 1, values, ..
            } => (values, None),
            Operation::Instruction {
                mode: "immediate",
                operands,
                ..
            } => (operands, None),
            Operation::Instruction {
                mode: "relative",
                operands,
                ..
            } => (&[], operands.first()),
            Operation::Instruction {
                mode: "zeropage,relative",
                operands,
                ..
            } => (operands.get(..1).unwrap_or(&[]), operands.get(1)),
            _ => return,
        };
        self.fits.extend(
            values
                .iter()
                .map(|value| (value.clone(), pc, at.clone(), Fit::Value)),
        );
        // ca65 reads a branch as the distance to its target, and that has
        // to be known before the link.
        if let Some(target) = distance {
            let distance = Expr::Bin(BinOp::Sub, Box::new(target.clone()), Box::new(Expr::Pc));
            self.fits.push((distance, pc, at.clone(), Fit::Distance));
        }
    }

    /// Refuses a one-byte value that is not known until the link and whose
    /// address size is bigger than a byte, as ca65 V2.19 does once it has
    /// read the program: `.byte lab` for a label in a segment whose
    /// addresses take two bytes. A value whose names are not all defined is
    /// left for the error that says so.
    fn check_fits(&mut self, bound: &HashMap<String, String>) {
        // An equate worked out when it is used reads its names as they are
        // bound at the end.
        for value in self.equates.values_mut() {
            *value = map_sym_expr(value.clone(), &mut |name| {
                Expr::Sym(bound.get(&name).cloned().unwrap_or(name))
            });
        }
        let mut found = Vec::new();
        for (value, pc, at, fit) in &self.fits {
            let value = map_sym_expr(value.clone(), &mut |name| {
                Expr::Sym(bound.get(&name).cloned().unwrap_or(name))
            });
            let known = AtPc {
                walk: self,
                pc: *pc,
            };
            let mut undefined = false;
            map_sym_expr(value.clone(), &mut |name| {
                undefined |= matches!(known.name(&name), Value::Unknown);
                Expr::Sym(name)
            });
            if undefined
                || matches!(
                    lower::value(&value, &known),
                    Value::Constant(_) | Value::Measured { .. }
                )
            {
                continue;
            }
            let bytes = match (lower::size(&value, &known), fit) {
                (Size::Zero, Fit::Value) => continue,
                (Size::Zero | Size::Absolute, _) => 2,
                _ => 3,
            };
            found.push(AsmError::at(
                at.clone(),
                format!("Range error (Address size {bytes} does not match fragment size 1)"),
            ));
        }
        self.errors.extend(found);
    }

    /// Reads one line that runs: lowers it, and follows what it does.
    fn read_line(
        &mut self,
        expander: &mut Expander,
        line: &ActiveLine,
        sectioned: bool,
        statements: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        let at = self.error_site(expander, line.span());
        let made_at = self.statement_site(expander, line.span());
        self.here = Some(at.clone());
        self.binding = self.binds(line);
        self.cpu = Some(line.cpu());
        // ca65 defines a line's label before it reads the rest of the line,
        // so an equate read there that reads the label sees it.
        let early = line
            .line()
            .label()
            .and_then(|label| label.name())
            .map(|name| self.as_read(&name.text()))
            .filter(|full| !self.names.contains_key(full) && !self.declared.contains_key(full));
        if let Some(full) = &early {
            self.names.insert(full.clone(), self.pc());
        }
        let lowered = lower::line(line, PROFILE, &*self);
        if let Some(full) = &early {
            self.names.remove(full);
        }
        self.binding = None;
        self.record_reads();
        let lowered = lowered.map_err(|diagnostic| {
            let span = syntax::Span::new(line.text(), diagnostic.range());
            let message = match diagnostic.kind() {
                DiagnosticKind::Unsupported => {
                    format!("{} (Lang198x's ca65 front end)", diagnostic.message())
                }
                _ => diagnostic.message().to_owned(),
            };
            AsmError::at(self.error_site(expander, span), message)
        })?;
        let made = self.made;
        // ca65 stops at `.fatal`, where it reports `.error` and carries on.
        let fatal = matches!(
            line.line().statement(),
            Some(lang198x_ca65::ast::Statement::Command(command))
                if command.word().is_some_and(|w| w.text().eq_ignore_ascii_case(".fatal"))
        );
        let mut items: Vec<(Option<String>, Option<Operation>)> = lowered
            .items
            .into_iter()
            .map(|item| match item.op {
                Some(Operation::Diagnose {
                    severity: DiagSeverity::Error,
                    message,
                }) if fatal => (
                    item.label,
                    Some(Operation::Diagnose {
                        severity: DiagSeverity::Fatal,
                        message,
                    }),
                ),
                op => (item.label, op),
            })
            .collect();
        let mut members = Vec::new();
        match lowered.record {
            Some(record) => self.change_record(record, &mut items, &mut members, &at)?,
            None => {
                if let Some(frame) = self.records.last() {
                    let busy = !items.is_empty()
                        || lowered.scope.is_some()
                        || lowered.segment.is_some()
                        || lowered.visibility.is_some()
                        || lowered.binary.is_some()
                        || lowered.cpu.is_some();
                    if busy {
                        let message = match frame.kind {
                            RecordKind::Enum => "Expected an identifier",
                            _ => "Expected a storage allocator",
                        };
                        return Err(AsmError::at(at, message));
                    }
                }
            }
        }
        let mut labels = Vec::new();
        for (item_label, item_op) in items {
            // What a label is known as comes from the line as written,
            // before its names are bound.
            let value = item_op.as_ref().map(|op| self.value_of(op));
            let waits = item_op.as_ref().is_some_and(|op| self.reads_unknown(op));
            // ca65 reads an assignment's value before its name is
            // defined, which matters to a cheap local, and a label
            // before the rest of its line.
            let sized = !matches!(item_op, Some(Operation::Equ(_) | Operation::Set(_)));
            // ca65 checks an equate for a circular definition at the end
            // only if its value had no address size of its own as it was
            // read. One that had is refused only where it is used.
            if let (Some(name), Some(Operation::Equ(value))) = (&item_label, &item_op)
                && lower::has_size(value, &*self)
            {
                self.lazy.insert(self.as_read(name));
            }
            let (label, op) = if sized {
                let label = item_label.map(|name| self.define_label(&name));
                (label, item_op.map(|op| self.names_in(op)))
            } else {
                let op = item_op.map(|op| self.names_in(op));
                (item_label.map(|name| self.define_label(&name)), op)
            };
            if let Some(op) = &op {
                self.note_fits(op, &at);
            }
            // ca65 studies an equate's value as it reads it, and refuses a
            // name in it whose definition comes back round to itself.
            // So it does the data and operands a line makes, once read.
            if let Some(op) = &op {
                // Under `.org`, `*` is the line's address; lowering has
                // already added each list item's offset to it.
                for value in values_of(op) {
                    if let Some((name, site)) = self.circle(value, self.org) {
                        return Err(circular(&name, site));
                    }
                }
            }
            if let Some(Operation::Equ(value)) = &op {
                self.study(value, &at)?;
                if let Some(label) = &label {
                    self.definitions
                        .insert(label.clone(), (value.clone(), at.clone()));
                }
            }
            let op = self.apply(label.as_deref(), value, waits, op, &at)?;
            if let (Some(label), true) = (&label, sized) {
                labels.push(label.clone());
            }
            if sized && (label.is_some() || op.is_some()) {
                self.used.insert(self.segment.clone());
            }
            let page = op.as_ref().and_then(page_check);
            statements.push(statement_at(self.sets(), &made_at, &at, label, op));
            if let Some(check) = page {
                statements.push(statement_at(self.sets(), &made_at, &at, None, Some(check)));
            }
            if let Some(address) = self.opens.take() {
                self.open_section(address, statements, &made_at);
            }
            for op in std::mem::take(&mut self.after) {
                statements.push(statement_at(self.sets(), &made_at, &at, None, Some(op)));
            }
        }
        if let Some(binary) = &lowered.binary {
            let op = self.binary(expander, line, binary, &at)?;
            let op = self.apply(None, None, false, Some(op), &at)?;
            statements.push(statement_at(self.sets(), &made_at, &at, None, op));
        }
        let line_size = self.made - made;
        for label in labels {
            self.sizes.insert(label, line_size);
        }
        self.sizes.extend(members);
        if let Some(change) = lowered.scope {
            self.change_scope(change, &at)?;
        }
        if let Some(change) = lowered.segment {
            self.change_segment(change, sectioned, statements, &at)?;
        }
        if let Some(declared) = lowered.visibility {
            self.declare(declared, &at)?;
        }
        match lowered.cpu {
            Some(CpuStack::Push) if self.cpus == 8 => {
                return Err(AsmError::at(at, "CPU stack overflow"));
            }
            Some(CpuStack::Push) => self.cpus += 1,
            Some(CpuStack::Pop) if self.cpus == 0 => {
                return Err(AsmError::at(at, "CPU stack is empty"));
            }
            Some(CpuStack::Pop) => self.cpus -= 1,
            _ => {}
        }
        Ok(())
    }

    /// Places each section from `layout`, now that every segment's length
    /// is known, and says what the link made. With no layout, the link is
    /// flat: `CODE` alone, at 0.
    fn place(
        &self,
        layout: Option<&Layout>,
        statements: &mut [Statement],
    ) -> Result<Option<Linked>, AsmError> {
        let known = || {
            layout.map_or_else(String::new, |layout| {
                let names: Vec<&str> = layout.segments.iter().map(|d| d.name.as_str()).collect();
                names.join(", ")
            })
        };
        let unplaced = |name: &str| {
            AsmError::new(
                0,
                if layout.is_some_and(|l| !l.from_config) {
                    format!(
                        "segment `{name}` is not in the NES config (valid: {}); this assembler \
                         links the curriculum's fixed NROM layout, which — like `ld65` with \
                         its `nes.cfg` — has no memory area for other segments",
                        known()
                    )
                } else {
                    format!(
                        "segment `{name}` is not in the linker config (valid: {}); like \
                         `ld65`, a segment outside SEGMENTS has no memory area to place it in",
                        known()
                    )
                },
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
        // The built-in layout places every segment where it always goes, so
        // an overlap is left to the check of extents, which names both.
        let resolved = if layout.from_config {
            layout.resolve(&lengths)?
        } else {
            layout.resolve(&BTreeMap::new())?
        };
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
                let number = u32::try_from(self.segments.len()).unwrap_or(u32::MAX);
                self.segments.insert(
                    name.to_owned(),
                    Seg {
                        number,
                        size,
                        length: 0,
                    },
                );
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
            // A cheap local belongs to the label before it, whatever scope
            // it is written in.
            return Lookup::Full(format!("{}{name}", self.cheap));
        }
        Lookup::Plain(name.to_owned())
    }

    /// The innermost full name of plain `name` that is defined, from this
    /// scope outwards.
    /// The full name `name` is found as, from this scope outwards: a name
    /// defined so far, or one imported, which hides a name further out as
    /// a definition does.
    fn defined(&self, name: &str) -> Option<String> {
        outwards(self.path())
            .map(|path| qualify(path, name))
            .find(|full| self.names.contains_key(full) || self.imported(full))
    }

    /// Whether `full` is imported, and not defined here.
    fn imported(&self, full: &str) -> bool {
        self.declared.get(full).is_some_and(|declaration| {
            // `.global` imports a name the program does not define.
            is_import(declaration.kind)
                || (declaration.kind == Visible::Global && !self.names.contains_key(full))
        })
    }

    /// The full name a reference to `name` stands for, as [`reference`]
    /// gives it, without recording it.
    ///
    /// [`reference`]: Self::reference
    fn as_read(&self, name: &str) -> String {
        match self.lookup(name) {
            Lookup::Full(full) => full,
            Lookup::Plain(name) => qualify(self.path(), &name),
        }
    }

    /// The full name a reference to `name` stands for, recorded as read. A
    /// plain name this scope has not defined is bound at the end.
    fn reference(&mut self, name: &str) -> String {
        let full = match self.lookup(name) {
            Lookup::Full(full) => full,
            Lookup::Plain(name) => {
                let here = qualify(self.path(), &name);
                if !self.path().is_empty()
                    && !self.names.contains_key(&here)
                    && !self.imported(&here)
                {
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

    /// Fails with every error found so far, if there are any.
    fn stop_if_errors(&mut self) -> Result<(), AsmError> {
        match AsmError::gather(std::mem::take(&mut self.errors)) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Adds the expander's new problems to the errors, and says where each
    /// one is.
    fn gather_expander(&mut self, expander: &Expander) -> Vec<(TextId, TextRange)> {
        let found = expander.diagnostics();
        let mut at = Vec::new();
        for (text, diagnostic) in &found[self.expander_seen.min(found.len())..] {
            let span = syntax::Span::new(*text, diagnostic.range());
            // ca65 reports a macro that calls itself too deeply at the call
            // as written, with no notes.
            let site = if diagnostic
                .message()
                .starts_with("too many nested macro calls")
            {
                self.site(expander, span)
            } else {
                self.error_site(expander, span)
            };
            let message = match self.missing_include.take() {
                Some(why) if diagnostic.message().starts_with("there is no file `") => why,
                // The expander reads `.macpack` itself, and ships no package.
                _ if diagnostic
                    .message()
                    .starts_with("there is no macro package")
                    || diagnostic.message().starts_with("`.macpack`") =>
                {
                    "`.macpack` is a real directive here and asm198x does not implement it \
                     yet — the source is valid and the gap is ours"
                        .to_owned()
                }
                _ => diagnostic.message().to_owned(),
            };
            self.errors.push(AsmError::at(site, message));
            at.push((*text, diagnostic.range()));
        }
        self.expander_seen = found.len();
        at
    }

    /// The line a statement made from `span` belongs to: where it was
    /// written, or for a line a macro made, the outermost call, as ca65's
    /// debug information gives it. A repeat's lines stay its own.
    fn statement_site(&self, expander: &Expander, span: syntax::Span) -> Span {
        self.site(expander, expander.macro_call(span).unwrap_or(span))
    }

    /// The instruction sets of the CPU the line being read is for.
    fn sets(
        &self,
    ) -> (
        &'static isa::InstructionSet,
        Option<&'static isa::InstructionSet>,
    ) {
        lower::sets(self.cpu.unwrap_or(lang198x_ca65::Cpu::Mos6502))
    }

    /// Where an error at `span` is reported. ca65 reports one in a line a
    /// macro made at the outermost call, and each call it came through
    /// notes where the macro is defined and where it was called.
    fn error_site(&self, expander: &Expander, span: syntax::Span) -> Span {
        self.reported(&expander.reading(span))
    }

    /// Where the expander says a problem on a line is reported, with its
    /// macro frames.
    fn reported(&self, reading: &Reading) -> Span {
        let place = |at: Option<Location>| match at {
            Some(at) => Span::in_file(self.file_id(at.file.get()), at.line, at.column),
            None => Span::in_file(FileId(0), 0, 0),
        };
        let mut at = place(reading.at);
        at.expansion_frames = reading
            .frames
            .iter()
            .map(|frame| {
                ExpansionFrame::new(
                    frame.name.clone(),
                    Some(place(frame.defined)),
                    place(frame.called),
                )
            })
            .collect();
        at
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
            Lookup::Plain(name) => match self.innermost(&name) {
                Some(full) => read(&full),
                None => outwards(self.path()).any(|path| read(&qualify(path, &name))),
            },
        }
    }

    /// The innermost full name plain `name` stands for that ca65 has made
    /// so far: one defined, declared to a linker, or used in this scope,
    /// which makes it here, or the name the line being lowered defines.
    fn innermost(&self, name: &str) -> Option<String> {
        let here = qualify(self.path(), name);
        if self.binding.as_deref() == Some(&*here) {
            return Some(here);
        }
        let used_here =
            !self.path().is_empty() && self.reads.borrow().iter().any(|read| read == name);
        outwards(self.path())
            .map(|path| qualify(path, name))
            .find(|full| {
                self.names.contains_key(full)
                    || self.declared.contains_key(full)
                    || (used_here && *full == here)
                    || self.pending.iter().any(|pending| pending.here == *full)
            })
    }

    /// Whether `name` is defined so far.
    fn has(&self, name: &str) -> bool {
        match self.lookup(name) {
            Lookup::Full(full) => self.names.contains_key(&full),
            // An import hides a name further out, as does a name used here
            // or being defined, though none of them is defined yet.
            Lookup::Plain(name) => self
                .innermost(&name)
                .is_some_and(|full| self.names.contains_key(&full)),
        }
    }

    /// The full name `line` gives a value to, if it is an assignment or an
    /// enum's member.
    fn binds(&self, line: &ActiveLine) -> Option<String> {
        use lang198x_ca65::ast::Statement as Source;
        let name = match line.line().statement()? {
            Source::Assignment(assignment) => assignment.name()?,
            Source::Member(member) if member.storage().is_none() => member.name()?,
            _ => return None,
        };
        Some(match self.lookup(&name.text()) {
            Lookup::Full(full) => full,
            Lookup::Plain(name) => qualify(self.path(), &name),
        })
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

    /// Refuses what ca65's `StudyExpr` refuses as it reads `value`, whose
    /// names are bound: a name whose definition comes back round to itself,
    /// and a division by zero it can work out.
    fn study(&self, value: &Expr, at: &Span) -> Result<(), AsmError> {
        if let Some((name, site)) = self.circle(value, self.org) {
            return Err(circular(&name, site));
        }
        let constant = |name: &str| match self.known(name) {
            Value::Constant(value) => Some(value),
            _ => None,
        };
        match value.eval_with(&constant, None, 0) {
            Err(error) if error.message.starts_with("division by zero") => {
                Err(AsmError::at(at.clone(), "Division by zero"))
            }
            Err(error) if error.message.starts_with("modulo by zero") => {
                Err(AsmError::at(at.clone(), "Modulo operation with zero"))
            }
            _ => Ok(()),
        }
    }

    /// The first equate `value` reaches that comes back round to itself,
    /// with where it was defined, following each equate's names in turn.
    /// Like ca65, it skips the side of `&&` and `||` a constant decides.
    /// `*` is `pc`, if it is known.
    fn circle(&self, value: &Expr, pc: Option<i64>) -> Option<(String, Span)> {
        fn reads(value: &Expr, pc: Option<i64>, names: &mut Vec<String>) {
            let decided = |lhs: &Expr| lhs.eval_with(&|_| None, pc, 0).ok();
            match value {
                Expr::Sym(name) => names.push(name.clone()),
                Expr::Bin(BinOp::LogAnd, lhs, _) if decided(lhs) == Some(0) => {
                    reads(lhs, pc, names)
                }
                Expr::Bin(BinOp::LogOr, lhs, _) if decided(lhs).is_some_and(|v| v != 0) => {
                    reads(lhs, pc, names);
                }
                Expr::Bin(_, lhs, rhs) => {
                    reads(lhs, pc, names);
                    reads(rhs, pc, names);
                }
                Expr::Lo(inner)
                | Expr::Hi(inner)
                | Expr::Bank(inner)
                | Expr::Neg(inner)
                | Expr::BitNot(inner)
                | Expr::FixRound(inner)
                | Expr::TrailingZeros(inner)
                | Expr::LogNot(inner) => reads(inner, pc, names),
                Expr::Num(_) | Expr::Pc => {}
            }
        }
        fn visit(
            walk: &Walk<'_>,
            value: &Expr,
            pc: Option<i64>,
            path: &mut Vec<String>,
        ) -> Option<(String, Span)> {
            let mut names = Vec::new();
            reads(value, pc, &mut names);
            for name in names {
                let Some((inner, site)) = walk.definitions.get(&name) else {
                    continue;
                };
                if path.contains(&name) {
                    return Some((name, site.clone()));
                }
                path.push(name);
                if let Some(found) = visit(walk, inner, None, path) {
                    return Some(found);
                }
                path.pop();
            }
            None
        }
        visit(self, value, pc, &mut Vec::new())
    }

    /// Whether `op` is an equate that reads a name nothing is known of yet.
    fn reads_unknown(&self, op: &Operation) -> bool {
        let Operation::Equ(value) = op else {
            return false;
        };
        // A name only sized for now, such as one a scope may yet define,
        // may change too.
        let mut unknown = false;
        map_sym_expr(value.clone(), &mut |name| {
            unknown |= matches!(self.name(&name), Value::Unknown | Value::Opaque(_));
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
                    Value::Constant(value) | Value::Measured { value, .. } => value,
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
            // With no fill of its own, the gap is room, as `.res` makes.
            Some(Operation::AlignTo { modulus, fill }) => {
                let count = align_pad(self.offset(), modulus);
                Some(match fill.first() {
                    Some(&value) => Operation::Fill {
                        count: Expr::Num(count),
                        value,
                    },
                    None if self.reserving.contains(&self.segment) => Operation::Fill {
                        count: Expr::Num(count),
                        value: 0,
                    },
                    None => Operation::Reserve(usize::try_from(count).unwrap_or(0)),
                })
            }
            other => other,
        };
        if let Some(op) = &op
            && !matches!(op, Operation::Equ(_) | Operation::Set(_))
        {
            let pc = self.org.unwrap_or_else(|| self.offset());
            let (set, extension) = self.sets();
            let next = next_pc(op, pc, set, extension, 1, at.line as usize)
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
    /// and declaring one again must agree with what was declared before,
    /// as ca65's `SymImport`, `SymExport` and `SymGlobal` check.
    fn declare(&mut self, declared: Visibility, at: &Span) -> Result<(), AsmError> {
        let refuse = |message: String| AsmError::at(at.clone(), message);
        let kind = declared.kind;
        for name in declared.names {
            let written = name.size;
            let full = qualify(self.path(), &name.name);
            let name = name.name;
            let defined = self.names.contains_key(&full);
            let variable = self.variables.contains(&full);
            // A size not written is the segment's, for an import.
            let size = written.unwrap_or_else(|| self.segment_size());
            let mismatch = || refuse(format!("Address size mismatch for symbol `{name}`"));
            let before = self
                .declared
                .get(&full)
                .map(|d| (d.kind, d.size.unwrap_or(Size::Absolute), d.export));
            let kept = self.declared.get(&full).and_then(|d| d.size);
            // An export written without a size takes the size of what it
            // exports, once that is defined.
            let actual = self.names.get(&full).map(|value| match value {
                Value::Constant(value) => Size::of(*value),
                Value::Address(size)
                | Value::Opaque(size)
                | Value::Placed { size, .. }
                | Value::Measured { size, .. } => *size,
                _ => Size::Absolute,
            });
            let export_size = written.or(actual);
            let (kind, size, export) = match (kind, before) {
                (Visible::Import | Visible::ForceImport, before) => {
                    if defined {
                        return Err(refuse(format!("Symbol `{name}` is already defined")));
                    }
                    match before {
                        Some((Visible::Export, ..)) => {
                            return Err(refuse(format!("Cannot import exported symbol `{name}`")));
                        }
                        Some((before @ (Visible::Import | Visible::ForceImport), was, _)) => {
                            if before != kind {
                                return Err(refuse(format!(
                                    "Redeclaration mismatch for symbol `{name}`"
                                )));
                            }
                            if size != was {
                                return Err(mismatch());
                            }
                        }
                        Some((Visible::Global, was, _)) if size != was => return Err(mismatch()),
                        _ => {}
                    }
                    (kind, Some(size), None)
                }
                (Visible::Export, before) => {
                    if variable {
                        return Err(refuse(format!("Var symbol `{name}` cannot be exported")));
                    }
                    match before {
                        Some((Visible::Import | Visible::ForceImport, ..)) => {
                            return Err(refuse(format!("Symbol `{name}` is already an import")));
                        }
                        Some((Visible::Global, _, was)) if was != written => {
                            return Err(mismatch());
                        }
                        Some((Visible::Export, _, was)) if !defined && was != written => {
                            return Err(mismatch());
                        }
                        _ => {}
                    }
                    // ca65 counts an export as a use of the name.
                    self.referenced.insert(full.clone());
                    (Visible::Export, kept, export_size)
                }
                (Visible::Global, before) => {
                    if variable {
                        return Err(refuse(format!("Var symbol `{name}` cannot be made global")));
                    }
                    match before {
                        Some((Visible::Import | Visible::ForceImport, was, _)) => {
                            if size != was {
                                return Err(mismatch());
                            }
                            continue;
                        }
                        Some((Visible::Export, _, was)) => {
                            if (!defined || written.is_some()) && written != was {
                                return Err(mismatch());
                            }
                            continue;
                        }
                        Some((Visible::Global, _, was)) => {
                            if written != was {
                                return Err(mismatch());
                            }
                            continue;
                        }
                        _ => {}
                    }
                    // A name already defined is exported at once.
                    if defined {
                        (Visible::Export, None, export_size)
                    } else {
                        (Visible::Global, Some(size), written)
                    }
                }
                (other, _) => (other, Some(size), written),
            };
            match self.declared.get_mut(&full) {
                Some(declaration) => {
                    declaration.kind = kind;
                    declaration.size = size;
                    declaration.export = export;
                }
                None => {
                    let order = self.declared.len();
                    self.declared.insert(
                        full.clone(),
                        Declaration {
                            kind,
                            size,
                            export,
                            path: self.path().to_owned(),
                            name,
                            at: at.clone(),
                            order,
                        },
                    );
                }
            }
            self.check_zero_page(&full, at);
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
        if declaration.export != Some(Size::Zero)
            || !matches!(declaration.kind, Visible::Export | Visible::Global)
        {
            return;
        }
        let size = match self.names.get(full) {
            None => return,
            Some(Value::Constant(value)) => Size::of(*value),
            Some(
                Value::Address(size)
                | Value::Opaque(size)
                | Value::Placed { size, .. }
                | Value::Measured { size, .. },
            ) => *size,
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
        // ld65 links by name, so two definitions exported under one name
        // are refused.
        let mut exports = HashMap::new();
        for declaration in self.declared.values() {
            let Some(full) = self.export_of(declaration) else {
                continue;
            };
            if let Some(other) = exports.insert(&declaration.name, full.clone())
                && other != full
            {
                return Err(AsmError::at(
                    declaration.at.clone(),
                    format!("Duplicate external identifier: `{}`", declaration.name),
                ));
            }
        }
        Ok(())
    }

    /// The definition `declaration` exports, if it exports one: an export,
    /// or a `.global` the program defines.
    fn export_of(&self, declaration: &Declaration) -> Option<String> {
        match declaration.kind {
            // An export may be defined in a scope further out.
            Visible::Export => outwards(&declaration.path)
                .map(|path| qualify(path, &declaration.name))
                .find(|full| self.names.contains_key(full)),
            // A `.global` its own scope does not define is an import.
            Visible::Global => Some(qualify(&declaration.path, &declaration.name))
                .filter(|full| self.names.contains_key(full)),
            _ => None,
        }
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
                    record: false,
                    start: self.made,
                });
            }
            Scope::Close { procedure } => {
                let frame = match self.open.pop() {
                    Some(frame) if frame.procedure == procedure && !frame.record => frame,
                    _ => {
                        let word = if procedure { ".PROC" } else { ".SCOPE" };
                        return Err(refuse(format!("No open {word}")));
                    }
                };
                // ca65 defines the size of a scope that made bytes, and
                // cheap locals then belong to that size's name.
                if self.made > frame.start {
                    self.cheap = format!("{}::.size", frame.path);
                }
                self.scopes
                    .insert(frame.path, Some(self.made - frame.start));
            }
            _ => return Err(refuse("this change of scope is not supported".to_owned())),
        }
        Ok(())
    }

    /// Follows a line of a record, as ca65's `struct.c` and `enum.c` read
    /// it. Each member becomes an equate of its offset, or an enum's
    /// value, in `items`, and its size goes in `members`. A `.tag` in the
    /// program makes room for the record.
    fn change_record(
        &mut self,
        record: Record,
        items: &mut Vec<(Option<String>, Option<Operation>)>,
        members: &mut Vec<(String, i64)>,
        at: &Span,
    ) -> Result<(), AsmError> {
        let refuse = |message: &str| AsmError::at(at.clone(), message.to_owned());
        match record {
            Record::Open { kind, name, field } => {
                let (start, field) = match self.records.last() {
                    Some(frame) if frame.kind == RecordKind::Enum => {
                        return Err(refuse("Expected an identifier"));
                    }
                    Some(_) if kind == RecordKind::Enum => {
                        return Err(refuse("Expected a storage allocator"));
                    }
                    Some(frame) => {
                        let offset = frame.offset;
                        let field = field.map(|field| {
                            let full = qualify(self.path(), &field);
                            items.push((Some(field), Some(Operation::Equ(Expr::Num(offset)))));
                            full
                        });
                        (if name.is_some() { 0 } else { offset }, field)
                    }
                    None => (0, None),
                };
                if let Some(name) = &name {
                    let path = qualify(self.path(), name);
                    if self.scopes.contains_key(&path) {
                        return Err(refuse(&format!("Duplicate scope `{name}`")));
                    }
                    self.scopes.insert(path.clone(), None);
                    self.open.push(Frame {
                        path,
                        name: Some(name.clone()),
                        procedure: false,
                        record: true,
                        start: self.made,
                    });
                }
                self.records.push(RecordFrame {
                    kind,
                    scoped: name.is_some(),
                    offset: start,
                    size: 0,
                    field,
                    base: Expr::Num(0),
                    count: 0,
                });
            }
            Record::Close(kind) => {
                let frame = match self.records.pop() {
                    Some(frame) if frame.kind == kind => frame,
                    Some(frame) => {
                        return Err(refuse(match frame.kind {
                            RecordKind::Struct => "Expected `.ENDSTRUCT`",
                            RecordKind::Union => "Expected `.ENDUNION`",
                            _ => "`.ENDENUM` expected",
                        }));
                    }
                    None => return Err(refuse("Unexpected end of a record")),
                };
                if frame.scoped
                    && let Some(scope) = self.open.pop()
                {
                    // An enum has no size, as in ca65. Defining the size of a
                    // struct or union moves cheap locals on, as a label does.
                    if kind != RecordKind::Enum {
                        self.scopes.insert(scope.path.clone(), Some(frame.size));
                        self.cheap = format!("{}::.size", scope.path);
                        self.record_scopes.insert(scope.path);
                    }
                }
                if let Some(field) = frame.field {
                    members.push((field, frame.size));
                }
                if let Some(parent) = self.records.last_mut() {
                    parent.take(frame.size);
                }
            }
            Record::Field { name, bytes } => self.member(name, bytes, items, members, at)?,
            Record::Tag {
                name,
                record,
                count,
            } => {
                let size = self.record_size(&record, "struct/union", at)?;
                // ca65 keeps a member under 16 MB.
                let bytes = size
                    .checked_mul(count)
                    .filter(|bytes| *bytes <= 0xFF_FFFF)
                    .ok_or_else(|| refuse("Range error"))?;
                self.member(name, bytes, items, members, at)?;
            }
            Record::Offset { name, offset } => {
                self.member(name, 0, items, members, at)?;
                if let Some(frame) = self.records.last_mut() {
                    frame.offset = offset;
                }
            }
            Record::Enumerator { name, value } => {
                // ca65 binds the names in a member's value as it reads it,
                // so the members after it count on from what they meant
                // there: a cheap local, say, before this member moves them.
                let bound = value.as_ref().map(|value| {
                    map_sym_expr(value.clone(), &mut |name| {
                        Expr::Sym(format!("::{}", self.as_read(&name)))
                    })
                });
                let Some(frame) = self.records.last_mut() else {
                    return Err(refuse("Expected an identifier"));
                };
                if let Some(bound) = bound {
                    frame.base = bound;
                    frame.count = 0;
                }
                let value = match (frame.count, value) {
                    (0, Some(value)) => value,
                    (0, None) => frame.base.clone(),
                    (count, _) => Expr::Bin(
                        BinOp::Add,
                        Box::new(frame.base.clone()),
                        Box::new(Expr::Num(count)),
                    ),
                };
                frame.count += 1;
                items.push((Some(name), Some(Operation::Equ(value))));
            }
            Record::Reserve { record, count } => {
                let size = self.record_size(&record, "struct", at)?;
                let bytes = usize::try_from(size * count).unwrap_or(usize::MAX);
                match items.last_mut() {
                    Some((_, op @ None)) => *op = Some(Operation::Reserve(bytes)),
                    _ => items.push((None, Some(Operation::Reserve(bytes)))),
                }
            }
            _ => return Err(refuse("this part of a record is not supported")),
        }
        Ok(())
    }

    /// A member of the struct or union being read, which takes `bytes`.
    fn member(
        &mut self,
        name: Option<String>,
        bytes: i64,
        items: &mut Vec<(Option<String>, Option<Operation>)>,
        members: &mut Vec<(String, i64)>,
        at: &Span,
    ) -> Result<(), AsmError> {
        let path = self.path().to_owned();
        let Some(frame) = self.records.last_mut() else {
            return Err(AsmError::at(at.clone(), "Expected a storage allocator"));
        };
        if frame.kind == RecordKind::Enum {
            return Err(AsmError::at(at.clone(), "Expected an identifier"));
        }
        if let Some(name) = name {
            members.push((qualify(&path, &name), bytes));
            items.push((Some(name), Some(Operation::Equ(Expr::Num(frame.offset)))));
        }
        frame.take(bytes);
        Ok(())
    }

    /// The size of the struct or union `name` names, as `.tag` reads it.
    /// `what` names what ca65 expected, for its messages.
    fn record_size(&self, name: &str, what: &str, at: &Span) -> Result<i64, AsmError> {
        let refuse = |message: String| AsmError::at(at.clone(), message);
        let full = match self.lookup(name) {
            Lookup::Full(full) => Some(full),
            Lookup::Plain(name) => outwards(self.path())
                .map(|path| qualify(path, &name))
                .find(|full| self.scopes.contains_key(full)),
        };
        let Some(full) = full.filter(|full| self.scopes.contains_key(full)) else {
            return Err(refuse(format!("Unknown {what}")));
        };
        if !self.record_scopes.contains(&full) {
            return Err(refuse(format!("Not a {what}")));
        }
        self.scopes[&full].ok_or_else(|| refuse("Size of struct/union is unknown".to_owned()))
    }

    /// Binds each pending name to the innermost scope around its reference
    /// that defined it, as ca65 does at the end. Returns the names that bind
    /// further out than their own scope.
    fn bind_pending(&self) -> HashMap<String, String> {
        let mut bound = HashMap::new();
        for pending in &self.pending {
            if self.names.contains_key(&pending.here) || self.imported(&pending.here) {
                continue;
            }
            if let Some(full) = outwards(&pending.path)
                .skip(1)
                .map(|path| qualify(path, &pending.name))
                .find(|full| self.names.contains_key(full) || self.imported(full))
            {
                bound.insert(pending.here.clone(), full);
            }
        }
        bound
    }

    /// Each import linked to the definition exported under its name, as
    /// ld65 links them, wherever in the program each is written.
    fn link_imports(&self) -> HashMap<String, String> {
        self.declared
            .iter()
            .filter(|(full, _)| self.imported(full))
            .filter_map(|(full, declaration)| {
                Some((full.clone(), self.exported(&declaration.name)?))
            })
            .collect()
    }

    /// The full name of the definition exported as `name`, if one is.
    fn exported(&self, name: &str) -> Option<String> {
        self.declared
            .values()
            .filter(|declaration| declaration.name == name)
            .find_map(|declaration| self.export_of(declaration))
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
    /// The address size of the name defined further out than `full`'s
    /// scope with the same spelling, if there is one.
    /// ca65 looks only while it reads in that scope.
    fn outer_size(&self, full: &str) -> Option<Size> {
        let (path, name) = full.rsplit_once("::")?;
        if path != self.path() {
            return None;
        }
        outwards(path)
            .skip(1)
            .find_map(|outer| self.names.get(&qualify(outer, name)))
            .map(|value| match value {
                Value::Constant(value) => Size::of(*value),
                Value::Address(size)
                | Value::Opaque(size)
                | Value::Placed { size, .. }
                | Value::Measured { size, .. } => *size,
                _ => Size::Absolute,
            })
    }

    /// What is known of the name whose full name is `full`. An equate whose
    /// value was not known when it was defined is worked out again, from
    /// the names it read.
    fn known(&self, full: &str) -> Value {
        let Some(&value) = self.names.get(full) else {
            // An import's address is the linker's, of the size declared.
            return match self.declared.get(full) {
                Some(declaration) if is_import(declaration.kind) => {
                    Value::Address(declaration.size.unwrap_or(Size::Absolute))
                }
                // `.global` makes an import only at the end, if nothing
                // defines the name by then.
                Some(declaration) if declaration.kind == Visible::Global => {
                    let size = declaration.size.unwrap_or(Size::Absolute);
                    if self.ended {
                        Value::Address(size)
                    } else {
                        Value::Opaque(size)
                    }
                }
                // A name its scope has not defined yet is sized, as ca65
                // sizes it, as the name of that spelling further out.
                _ if !self.ended => self.outer_size(full).map_or(Value::Unknown, Value::Opaque),
                _ => Value::Unknown,
            };
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

/// The walk at the end of the program, asked about a value of a line it
/// read earlier: its names are full names, and `*` is that line's address.
/// The parser's word for a name at the start of a line with no `:` after it.
const UNLABELLED: &str = "expected `:` after a name to make it a label";

/// The first word of `line` that starts with a `.`, as written.
fn command_word(line: &ActiveLine) -> Option<String> {
    line.line()
        .syntax()
        .descendants()
        .filter_map(syntax::SyntaxElement::into_token)
        .map(|token| token.text().to_owned())
        .find(|text| text.starts_with('.') && text.len() > 1)
}

/// How ca65's declared surface refuses the command on `line`, if it does:
/// a word ca65 has and the walk does not implement, or one ca65 refuses
/// itself.
fn surface_refusal(line: &ActiveLine) -> Option<String> {
    let word = command_word(line)?.to_ascii_lowercase();
    let entry = crate::directives::lookup(super::ca65::DIRECTIVES, &word)?;
    match entry.category {
        Category::KnownUnsupported => Some(format!(
            "`{word}` is a real directive here and asm198x does not implement it yet — \
             the source is valid and the gap is ours"
        )),
        Category::RefusedByReference(rule) => Some(crate::directives::refused_by_reference(
            "the ca65+ld65 pipeline",
            &word,
            rule,
        )),
        _ => None,
    }
}

/// The name at the start of `line`, if the parser found no `:` after it:
/// a word that is neither an instruction nor a macro here.
fn unknown_word(line: &ActiveLine) -> Option<String> {
    line.line()
        .syntax()
        .descendants()
        .filter_map(syntax::SyntaxElement::into_token)
        .find(|token| {
            !matches!(
                token.kind(),
                syntax::SyntaxKind::WHITESPACE | syntax::SyntaxKind::NEWLINE
            )
        })
        .map(|token| token.text().to_owned())
        .filter(|text| text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'))
}

/// What [`Walk::check_fits`] checks a value for.
#[derive(Clone, Copy)]
enum Fit {
    /// A byte: its address size has to be the zero page's.
    Value,
    /// A branch's distance: it has to be known before the link.
    Distance,
}

struct AtPc<'w, 'f> {
    walk: &'w Walk<'f>,
    pc: Value,
}

impl Known for AtPc<'_, '_> {
    fn name(&self, name: &str) -> Value {
        self.walk.known(name)
    }

    fn pc(&self) -> Value {
        self.pc
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
                // A name declared with an address size of its own, and not
                // defined yet, has that size.
                _ if let Some((full, declaration)) = self
                    .innermost(&name)
                    .filter(|full| !self.names.contains_key(full))
                    .and_then(|full| Some((full.clone(), self.declared.get(&full)?)))
                    && let Some(size) = declaration.size =>
                {
                    if is_import(declaration.kind) || declaration.kind == Visible::Global {
                        self.known(&full)
                    } else {
                        Value::Opaque(size)
                    }
                }
                Some(full) if full == qualify(self.path(), &name) => self.known(&full),
                // The name may yet be defined here, so ca65 folds it with
                // nothing, and sizes it as the name further out.
                Some(full) => match self.known(&full) {
                    Value::Constant(value) => Value::Opaque(Size::of(value)),
                    Value::Address(size)
                    | Value::Opaque(size)
                    | Value::Placed { size, .. }
                    | Value::Measured { size, .. } => Value::Opaque(size),
                    other => other,
                },
                None => Value::Unknown,
            },
        }
    }

    /// The address under `.org`, and otherwise how far into its segment
    /// it is. ca65 counts the bytes `.org` made, so labels on either side
    /// of `.org` and `.reloc` are still a known distance apart.
    fn pc(&self) -> Value {
        match (self.org, self.segments.get(&self.segment)) {
            (Some(pc), _) => Value::Constant(pc),
            (None, Some(seg)) => Value::Placed {
                segment: seg.number,
                offset: seg.length,
                size: seg.size,
            },
            (None, None) => Value::Opaque(self.segment_size()),
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
    /// Where the line the expander acts on next is, for the problems its
    /// questions find.
    fn reading(&mut self, reading: &Reading) {
        self.here = Some(self.reported(reading));
    }

    /// A file `.include` reads, looked for beside the file that asks and
    /// then beside each file that included it. A single file has none.
    fn include(&mut self, name: &str, from: syntax::FileId) -> Option<String> {
        let from = usize::try_from(from.get()).ok()?;
        let chain = self.chain(from);
        let at = self
            .here
            .clone()
            .unwrap_or_else(|| Span::in_file(FileId(0), 0, 0));
        let Some(files) = self.files.as_mut() else {
            self.errors.push(AsmError::at(
                at,
                format!(
                    "cannot resolve `.include \"{name}\"` here — the single-source API \
                     assembles one file; use the multi-file entry point (the CLI resolves \
                     includes automatically)"
                ),
            ));
            return Some(String::new());
        };
        let id = match load_include(
            files.map,
            files.loader,
            name,
            &chain,
            0,
            Resolution::AncestorChain,
        ) {
            Ok(id) => id,
            Err(error) => {
                // The loader says why, which the expander's report takes on.
                self.missing_include = Some(error.to_string());
                return None;
            }
        };
        // ca65 has no check of its own and fails when it runs out of open
        // files; a file that includes itself is refused at once instead.
        if chain.contains(&id) {
            let path = |file: &FileId| files.map.path(*file).unwrap_or("?").to_owned();
            let cycle: Vec<String> = chain.iter().chain(std::iter::once(&id)).map(path).collect();
            self.errors.push(AsmError::at(
                at,
                format!("include cycle: {}", cycle.join(" -> ")),
            ));
            return Some(String::new());
        }
        let text = files.map.contents(id)?.to_owned();
        self.read.push((id, Some(from)));
        Some(text)
    }

    /// A condition or count, which ca65 needs now: a constant expression.
    fn evaluate(&mut self, expr: &SourceExpr) -> Result<i64, String> {
        let lowered = lower::expression(expr, &*self);
        self.record_reads();
        let lowered = lowered.map_err(|d| d.message().to_owned())?;
        // ca65 studies a condition as it does any value.
        let bound = map_sym_expr(lowered.clone(), &mut |name| Expr::Sym(self.as_read(&name)));
        let at = self
            .here
            .clone()
            .unwrap_or_else(|| Span::in_file(FileId(0), 0, 0));
        if let Err(error) = self.study(&bound, &at) {
            self.errors.push(error);
        }
        match lower::value(&lowered, &*self) {
            Value::Constant(value) | Value::Measured { value, .. } => Ok(value),
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

/// The check ca65 adds to a 6502 `jmp (abs)`, whose pointer the 6502
/// cannot read across a page: the link fails if the pointer's low byte is
/// `$FF`.
fn page_check(op: &Operation) -> Option<Operation> {
    let Operation::Instruction {
        mnemonic,
        mode: "indirect",
        operands,
    } = op
    else {
        return None;
    };
    let pointer = operands
        .first()
        .filter(|_| mnemonic.eq_ignore_ascii_case("jmp"))?;
    Some(Operation::Assert {
        cond: Expr::Bin(
            BinOp::Ne,
            Box::new(Expr::Lo(Box::new(pointer.clone()))),
            Box::new(Expr::Num(0xFF)),
        ),
        fatal: true,
        message: "\"jmp (abs)\" across page border".to_owned(),
    })
}

/// ca65's error for a name whose definition comes back round to itself,
/// at that definition.
fn circular(name: &str, site: Span) -> AsmError {
    AsmError::at(
        site,
        format!(
            "Circular reference in definition of symbol `{}`",
            name.rsplit("::").next().unwrap_or(name)
        ),
    )
}

/// The values a data or instruction line reads.
fn values_of(op: &Operation) -> &[Expr] {
    match op {
        Operation::Sized { values, .. } => values,
        Operation::Instruction { operands, .. } => operands,
        _ => &[],
    }
}

/// A statement placed at `placed`, whose errors the engine reports at
/// `reported`, with its macro frames, when that is the same line.
fn statement_at(
    sets: (
        &'static isa::InstructionSet,
        Option<&'static isa::InstructionSet>,
    ),
    placed: &Span,
    reported: &Span,
    label: Option<String>,
    op: Option<Operation>,
) -> Statement {
    let mut made = statement(placed, label, op);
    // A line for a CPU with more than the 6502's instructions names them.
    if sets.1.is_some() {
        made.instruction_set = Some(sets.0);
        made.extension_set = sets.1;
    }
    if reported.file == placed.file && reported.line == placed.line {
        made.operand_span = Some(reported.clone());
    }
    made
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
