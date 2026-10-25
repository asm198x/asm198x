//! PasmoNext 0.1.3 through Lang198x's Z80 front end.
//!
//! `lang198x-z80-asm` parses PasmoNext source, runs its macros, repeats and
//! conditionals, and lowers each line to statements. This walk does what that
//! crate leaves to an assembler: PasmoNext's passes, its symbols and the
//! address, kept with [`next_pc`] so sizes have one source. The engine then
//! assembles the last pass's statements. The rules were measured on PasmoNext
//! and are recorded in `decisions/pasmonext-walk.md`.

use std::cell::RefCell;
use std::collections::HashMap;

use lang198x_z80_asm::ast::{Expr as SourceExpr, Statement as SourceStatement};
use lang198x_z80_asm::expand::{ActiveLine, Block, Expander, Host, Program};
use lang198x_z80_asm::lower::{
    self, Cpu, Definition, Known, Message, Piece as MessagePiece, Scope,
};
use lang198x_z80_asm::syntax::{self, DiagnosticKind, TextId};
use lang198x_z80_asm::{Profile, predefine};

use crate::ast::{map_sym_expr, map_syms};
use crate::engine::{AsmError, Expr, Operation, Piece, Statement, Warning, next_pc};
use crate::source::{LoadError, SourceLoader, SourceMap};
use crate::span::{ExpansionFrame, FileId, Span};

mod scopes;

use scopes::Scopes;

/// The name the root file has in the front end's own table.
const ROOT: &str = "<source>";

/// PasmoNext 0.1.3's options that change what source means.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// `--nocase`: names match whatever their case.
    pub names_in_any_case: bool,
    /// `--alocal`: names starting `_` or `@` are automatic locals.
    pub automatic_locals: bool,
    /// Each `--equ` (or `-E`) text, such as `size=10` or `debug`, in order.
    pub predefined: Vec<String>,
}

impl Options {
    fn profile(&self) -> Profile {
        if self.names_in_any_case {
            Profile::PASMONEXT_0_1_3.with_names_in_any_case()
        } else {
            Profile::PASMONEXT_0_1_3
        }
    }
}

/// Statements for a single source, with no includes.
pub(crate) fn statements(
    source: &str,
    z80n: bool,
    options: &Options,
) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
    let profile = options.profile();
    let program = Program::load(ROOT, source, profile, |_| None).map_err(too_long)?;
    Walk::new(z80n, vec![FileId(0)], options)?.run(&program, &mut |_| None)
}

/// Statements for a program whose root is `FileId(0)` in `map`, reading
/// `INCLUDE` and `INCBIN` files through `loader`.
pub(crate) fn statements_multi(
    map: &mut SourceMap,
    loader: &dyn SourceLoader,
    z80n: bool,
    options: &Options,
) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
    let loader = &FromRunFolder(loader);
    let root = map.contents(FileId(0)).unwrap_or_default().to_owned();
    let mut files = vec![FileId(0)];
    let program = Program::load(ROOT, &root, options.profile(), |name| {
        let id = map.load(loader, name, FileId(0), 0).ok()?;
        files.push(id);
        map.contents(id).map(str::to_owned)
    })
    .map_err(too_long)?;
    let mut binaries = |name: &str| loader.load_binary(name, None).ok();
    Walk::new(z80n, files, options)?.run(&program, &mut binaries)
}

/// PasmoNext opens a file by the name written, from the folder it runs in,
/// not from the folder of the file that includes it. A loader is asked as if
/// for the root, so it resolves from its base: `include "caller.inc"` in
/// `mistakes/broken.asm`, run from the folder above, finds `caller.inc` there.
struct FromRunFolder<'a>(&'a dyn SourceLoader);

impl SourceLoader for FromRunFolder<'_> {
    fn load_text(&self, request: &str, _: Option<&str>) -> Result<(String, String), LoadError> {
        self.0.load_text(request, None)
    }

    fn load_text_at(
        &self,
        request: &str,
        _: Option<&str>,
        line: u32,
    ) -> Result<(String, String), LoadError> {
        self.0.load_text_at(request, None, line)
    }

    fn load_binary(&self, request: &str, _: Option<&str>) -> Result<Vec<u8>, LoadError> {
        self.0.load_binary(request, None)
    }

    fn resolve_text(&self, request: &str, _: Option<&str>) -> Option<String> {
        self.0.resolve_text(request, None)
    }
}

fn too_long(_: syntax::ProvenanceError) -> AsmError {
    AsmError::new(0, "the program is too long to read")
}

/// How a symbol was defined, as PasmoNext records it (`Defined` in its
/// `asm.cpp`). A name with no entry is undefined.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Def {
    /// By `--equ`, before pass 1.
    Predefined,
    Pass1,
    Pass2,
    Defl,
}

#[derive(Clone, Copy, Debug)]
struct Symbol {
    value: i64,
    def: Def,
}

struct Walk {
    profile: Profile,
    cpu: Cpu,
    set: &'static isa::InstructionSet,
    ext: Option<&'static isa::InstructionSet>,
    /// Each front-end file's id here, in the order the front end read them.
    files: Vec<FileId>,
    pass: u8,
    symbols: HashMap<String, Symbol>,
    /// The address, which PasmoNext keeps in 16 bits.
    pc: i64,
    /// A problem found where the expander could not be told at once.
    pending: Option<String>,
    /// `REPT` counters set since the last line, for the statements.
    counters: Vec<(String, i64)>,
    /// The scopes open: PasmoNext's `localstack`.
    scopes: Scopes,
    /// The names `--equ` defines, with their values, in order.
    predefined: Vec<(String, i64)>,
    /// The open `STRUCT`'s name and the offset its next field takes.
    structure: Option<(String, i64)>,
    /// The counter `RB` and `RW` reserve from, which `RSSET` sets.
    reserve: i64,
}

/// What stops a pass: a located error, or a program that needs PasmoNext's
/// third pass.
type Stop = AsmError;

impl Walk {
    /// A walk with PasmoNext's `options`. Names given by `--equ` are defined
    /// now, before pass 1, as `addpredef` defines them.
    fn new(z80n: bool, files: Vec<FileId>, options: &Options) -> Result<Self, AsmError> {
        let profile = options.profile();
        let predefined = options
            .predefined
            .iter()
            .map(|text| predefine(text, profile).map_err(|message| AsmError::new(0, message)))
            .collect::<Result<Vec<_>, _>>()?;
        let symbols = predefined
            .iter()
            .map(|(name, value)| {
                let symbol = Symbol {
                    value: *value,
                    def: Def::Predefined,
                };
                (name.clone(), symbol)
            })
            .collect();
        Ok(Self {
            profile,
            cpu: if z80n { Cpu::Z80N } else { Cpu::Z80 },
            set: &isa::z80::SET,
            ext: z80n.then_some(&isa::z80::NEXT),
            files,
            pass: 1,
            symbols,
            pc: 0,
            pending: None,
            counters: Vec::new(),
            scopes: Scopes::new(options.automatic_locals),
            predefined,
            structure: None,
            reserve: 0,
        })
    }

    /// PasmoNext's `processfile`: pass 1, then pass 2, whose statements are
    /// the program.
    fn run(
        mut self,
        program: &Program,
        binaries: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    ) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
        self.pass(program, binaries, None)?;
        self.pass = 2;
        let mut out = (Vec::new(), Vec::new());
        self.pass(program, binaries, Some(&mut out))?;
        Ok(out)
    }

    /// PasmoNext's `dopass`. `out` collects the statements on the last pass.
    fn pass(
        &mut self,
        program: &Program,
        binaries: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
        mut out: Option<&mut (Vec<Statement>, Vec<Warning>)>,
    ) -> Result<(), Stop> {
        self.pc = 0;
        self.scopes.reset();
        // PasmoNext never resets its reserve counter, so a pass starts where
        // the last left off and `RB` without `RSSET` ends in a phase error.
        // That is one of its bugs: each pass starts the counter at 0.
        self.reserve = 0;
        self.structure = None;
        self.symbols.retain(|_, symbol| symbol.def != Def::Defl);
        let mut expander = Expander::new(program.clone(), self.profile)
            .map_err(|_| AsmError::new(0, "the program is too long to parse"))?;
        if let Some((file, diagnostic)) = program.diagnostics().first() {
            let span = syntax::Span::new(TextId::File(*file), diagnostic.range());
            return Err(self.error_at(&expander, span, diagnostic.message()));
        }
        // The engine is told the names `--equ` defines before any line.
        if let Some((statements, _)) = out.as_deref_mut() {
            let before = Span::in_file(FileId(0), 0, 0);
            for (name, value) in &self.predefined {
                let op = Some(Operation::Equ(Expr::Num(*value)));
                statements.push(statement(&before, Some(name.clone()), op));
            }
        }
        loop {
            let line = expander.next_line(self);
            self.check(&expander)?;
            let Some(line) = line else {
                return self.finish();
            };
            // PasmoNext defines a line's label before it reads the rest,
            // except on `ORG`, `EQU` and `DEFL` lines (`parselabel`). Under
            // `--alocal`, that order decides which scope a name read on the
            // line belongs to.
            let label_first = label_first(&line, self.profile);
            if let Some(name) = &label_first {
                self.scopes.defining(name);
            }
            let lowered = lower::line(&line, &self.profile, self.cpu, self, binaries).map_err(
                |diagnostic| {
                    let span = syntax::Span::new(line.text(), diagnostic.range());
                    let message = match diagnostic.kind() {
                        DiagnosticKind::Unsupported => {
                            format!("{} (Lang198x's PasmoNext front end)", diagnostic.message())
                        }
                        _ => diagnostic.message().to_owned(),
                    };
                    self.error_at(&expander, span, &message)
                },
            )?;
            let at = self.place(&expander, &line);
            let messages: Vec<String> = lowered.warnings().collect();
            let counters = std::mem::take(&mut self.counters);
            if let Some((statements, _)) = out.as_deref_mut() {
                for (name, value) in counters {
                    statements.push(statement(
                        &at,
                        Some(name),
                        Some(Operation::Set(Expr::Num(value))),
                    ));
                }
            }
            for mut item in lowered.items {
                item.op = item
                    .op
                    .map(|op| map_syms(op, &mut |name| Expr::Sym(self.scopes.resolve(name))));
                if let Some(name) = &item.label
                    && label_first.is_none()
                {
                    self.scopes.defining(name);
                }
                item.label = item.label.map(|name| self.scopes.resolve(name));
                let value = self
                    .apply(item.label.as_deref(), item.op.as_ref())
                    .map_err(|message| AsmError::at(at.clone(), message))?;
                if let Some((statements, _)) = out.as_deref_mut() {
                    // The engine reads a `DEFL` value where it stands, so one
                    // defined from a name further on would fail there. The
                    // walk has PasmoNext's value for both kinds of symbol, so
                    // the engine is given it.
                    let op = match (item.op, value) {
                        (Some(Operation::Equ(_)), Some(value)) => {
                            Some(Operation::Equ(Expr::Num(value)))
                        }
                        (Some(Operation::Set(_)), Some(value)) => {
                            Some(Operation::Set(Expr::Num(value)))
                        }
                        (op, _) => op,
                    };
                    statements.push(statement(&at, item.label, op));
                }
            }
            if let Some(scope) = lowered.scope {
                self.scope(scope, &at)
                    .map_err(|message| AsmError::at(at.clone(), message))?;
            }
            if let Some(definition) = lowered.definition {
                let defined = self
                    .definition(definition)
                    .map_err(|message| AsmError::at(at.clone(), message))?;
                if let Some((statements, _)) = out.as_deref_mut() {
                    for (name, value) in defined {
                        let op = Some(Operation::Equ(Expr::Num(value)));
                        statements.push(statement(&at, Some(name), op));
                    }
                }
            }
            // PasmoNext prints a `MESSAGE` on the last pass only, and gives a
            // `.WARNING` on every pass, so twice; the walk gives each once.
            if let (Some(message), Some((_, warnings))) = (lowered.message, out.as_deref_mut()) {
                let mut warning = match message {
                    Message::Warning(text) => Warning::new(at.line as usize, text),
                    Message::Print(pieces) => {
                        let text = self
                            .message(pieces)
                            .map_err(|message| AsmError::at(at.clone(), message))?;
                        Warning::note(at.line as usize, text)
                    }
                    other => {
                        return Err(AsmError::at(
                            at.clone(),
                            format!(
                                "{other:?} is not assembled yet (Lang198x's PasmoNext front end)"
                            ),
                        ));
                    }
                };
                warning.file = at.file;
                warnings.push(warning);
            }
            if let Some((_, warnings)) = out.as_deref_mut() {
                for message in messages {
                    let mut warning = Warning::new(at.line as usize, message);
                    warning.file = at.file;
                    warnings.push(warning);
                }
            }
        }
    }

    /// The end of a pass: a `PROC` still open is an error where it was
    /// written.
    fn finish(&mut self) -> Result<(), Stop> {
        self.scopes.finish().map_err(|(at, message)| match at {
            Some(at) => AsmError::at(at, message),
            None => AsmError::new(0, message),
        })
    }

    /// A line's change to the scopes, after its statements.
    fn scope(&mut self, scope: Scope, at: &Span) -> Result<(), String> {
        match scope {
            Scope::Local(names) => self.scopes.local(&names),
            Scope::Proc => {
                self.scopes.proc(at.clone());
                Ok(())
            }
            Scope::Endp => self.scopes.endp(),
            // The names go in the public symbol file, which this path does
            // not write yet; what they mean does not change.
            Scope::Public(names) => self.scopes.public(&names),
            other => Err(format!(
                "{other:?} is not assembled yet (Lang198x's PasmoNext front end)"
            )),
        }
    }

    /// A structure, `RB`, `RW` or `RSSET` line (`parseSTRUCT`, `parseRB`,
    /// `parseRW`, `parseRSSET`). Returns the names it defines, each as
    /// `setequorlabel` defines it, with their values.
    fn definition(&mut self, definition: Definition) -> Result<Vec<(String, i64)>, String> {
        let (name, value) = match definition {
            Definition::Struct(name) => {
                self.scopes.macro_defined(&name)?;
                self.structure = Some((name, 0));
                return Ok(Vec::new());
            }
            Definition::Field { name, size } => {
                let (structure, offset) =
                    self.structure.as_mut().ok_or("a field outside of STRUCT")?;
                let field = (format!("{structure}.{name}"), *offset);
                *offset += size;
                field
            }
            Definition::EndStruct => self.structure.take().ok_or("ENDS outside of STRUCT")?,
            Definition::Reserve { name, size } => {
                let value = self.reserve;
                self.reserve += size;
                (name, value)
            }
            Definition::ReserveFrom(value) => {
                self.reserve = value;
                return Ok(Vec::new());
            }
            // Where a snapshot's stack starts; this path writes no snapshot.
            Definition::Stack(_) => return Ok(Vec::new()),
            other => {
                return Err(format!(
                    "{other:?} is not assembled yet (Lang198x's PasmoNext front end)"
                ));
            }
        };
        self.scopes.defining(&name);
        let name = self.scopes.resolve(name);
        let value = value & 0xFFFF;
        self.define(&name, value)?;
        Ok(vec![(name, value)])
    }

    /// A `MESSAGE` line's text (`parseMESSAGE`): each value in four hex
    /// digits, or in decimal after `%d`, and every name defined.
    fn message(&self, pieces: Vec<MessagePiece>) -> Result<String, String> {
        let mut text = String::new();
        for piece in pieces {
            match piece {
                MessagePiece::Text(words) => text.push_str(&words),
                MessagePiece::Value { value, decimal } => {
                    let value =
                        map_sym_expr(value, &mut |name| Expr::Sym(self.scopes.resolve(name)));
                    let value = self.read(&value)? & 0xFFFF;
                    if decimal {
                        text.push_str(&value.to_string());
                    } else {
                        text.push_str(&format!("{value:04X}"));
                    }
                }
                other => {
                    return Err(format!(
                        "{other:?} is not printed yet (Lang198x's PasmoNext front end)"
                    ));
                }
            }
        }
        Ok(text)
    }

    /// Stops at the expander's first problem, or one found while it ran.
    fn check(&mut self, expander: &Expander) -> Result<(), Stop> {
        if let Some((text, diagnostic)) = expander.diagnostics().first() {
            let span = syntax::Span::new(*text, diagnostic.range());
            return Err(self.error_at(expander, span, diagnostic.message()));
        }
        if let Some(message) = self.pending.take() {
            return Err(AsmError::new(0, message));
        }
        Ok(())
    }

    /// Where `line` was written, with the calls that ran it.
    fn place(&self, expander: &Expander, line: &ActiveLine) -> Span {
        let mut span = self.span_of(expander, line.span());
        span.expansion_frames = expander
            .calls(line)
            .into_iter()
            .map(|call| {
                let at = Span::in_file(self.file(call.at.file), call.at.line, call.at.column);
                ExpansionFrame::new(call.name, None, at)
            })
            .collect();
        span
    }

    fn span_of(&self, expander: &Expander, span: syntax::Span) -> Span {
        match expander.locate(span) {
            Some(at) => Span::in_file(self.file(at.file), at.line, at.column),
            None => Span::in_file(FileId(0), 0, 0),
        }
    }

    fn file(&self, id: syntax::FileId) -> FileId {
        self.files
            .get(id.get() as usize)
            .copied()
            .unwrap_or(FileId(0))
    }

    fn error_at(&self, expander: &Expander, span: syntax::Span, message: &str) -> AsmError {
        AsmError::at(self.span_of(expander, span), message)
    }

    // What a statement does to the symbols and the address.

    /// Applies a statement. Returns the value an `EQU` or `DEFL` gave its
    /// name.
    fn apply(
        &mut self,
        label: Option<&str>,
        op: Option<&Operation>,
    ) -> Result<Option<i64>, String> {
        match op {
            Some(Operation::Equ(value)) => {
                let value = self.read(value)?;
                self.define(label.unwrap_or_default(), value)?;
                Ok(Some(value))
            }
            Some(Operation::Set(value)) => {
                let value = self.read(value)?;
                self.define_defl(label.unwrap_or_default(), value)?;
                Ok(Some(value))
            }
            Some(Operation::Org(address)) => {
                self.pc = self.read(address)? & 0xFFFF;
                Ok(None)
            }
            _ => {
                if let Some(label) = label {
                    self.define(label, self.pc)?;
                }
                let Some(op) = op else {
                    return Ok(None);
                };
                for expr in expressions(op) {
                    self.read(expr)?;
                }
                let next = next_pc(op, self.pc, self.set, self.ext, 1, 0)
                    .map_err(|error| error.message)?;
                self.pc = next & 0xFFFF;
                Ok(None)
            }
        }
    }

    /// The value PasmoNext reads where it does not need one now: a name not
    /// defined is 0 in pass 1 and an error after.
    fn read(&self, expr: &Expr) -> Result<i64, String> {
        let missing = RefCell::new(None);
        let resolve = |name: &str| match self.symbols.get(name) {
            Some(symbol) => Some(symbol.value),
            None if self.pass == 1 => Some(0),
            None => {
                missing.borrow_mut().get_or_insert_with(|| name.to_owned());
                None
            }
        };
        expr.eval_with(&resolve, Some(self.pc), 0)
            .map_err(|error| match missing.into_inner() {
                Some(name) => format!("Symbol '{name}' is undefined"),
                None => error.message,
            })
    }

    /// PasmoNext's `setequorlabel`.
    fn define(&mut self, name: &str, value: i64) -> Result<(), String> {
        let old = self.symbols.get(name).copied();
        match old.map(|symbol| symbol.def) {
            Some(Def::Predefined) => return Err("Can't redefine, is predefined".into()),
            Some(Def::Defl) => return Err("Invalid definition, previously defined as DEFL".into()),
            Some(Def::Pass1) if self.pass == 1 => return Err(redefined()),
            Some(Def::Pass2) if self.pass == 2 => return Err(redefined()),
            None if self.pass > 1 => {
                return Err(format!("The symbol '{name}' must be defined in pass 1"));
            }
            _ => {}
        }
        let def = if self.pass == 1 {
            Def::Pass1
        } else {
            Def::Pass2
        };
        // `setvar`: a value that moves in the last pass needs another.
        if def == Def::Pass2
            && let Some(old) = old
            && old.value != value
        {
            return Err(format!(
                "PasmoNext 0.1.3 runs a third pass for this program, since '{name}' changed from \
                 {:04X}H to {value:04X}H in the second; three-pass mode is not lowered yet \
                 (Lang198x's PasmoNext front end)",
                old.value
            ));
        }
        self.symbols.insert(name.to_owned(), Symbol { value, def });
        Ok(())
    }

    /// PasmoNext's `setdefl`.
    fn define_defl(&mut self, name: &str, value: i64) -> Result<(), String> {
        if matches!(
            self.symbols.get(name).map(|symbol| symbol.def),
            Some(Def::Predefined | Def::Pass1 | Def::Pass2)
        ) {
            return Err(redefined());
        }
        self.symbols.insert(
            name.to_owned(),
            Symbol {
                value,
                def: Def::Defl,
            },
        );
        Ok(())
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

fn redefined() -> String {
    "Invalid definition, previously defined as EQU or label".into()
}

/// The expressions an operation the front end makes reads.
fn expressions(op: &Operation) -> Vec<&Expr> {
    match op {
        Operation::Bytes(values) | Operation::Words(values) => values.iter().collect(),
        Operation::Instruction { operands, .. } => operands.iter().collect(),
        Operation::ComputedInstruction(pieces) | Operation::Encoded(pieces) => pieces
            .iter()
            .filter_map(|piece| match piece {
                Piece::Val { expr, .. } | Piece::Packed { expr, .. } => Some(expr),
                _ => None,
            })
            .collect(),
        Operation::Entry(expr) | Operation::Fill { count: expr, .. } => vec![expr],
        _ => Vec::new(),
    }
}

impl Known for Walk {
    /// `isdefined`: undefined, or defined only in an earlier pass, is false.
    fn defined(&self, name: &str) -> bool {
        match self
            .symbols
            .get(&self.scopes.resolve(name.to_owned()))
            .map(|symbol| symbol.def)
        {
            None => false,
            Some(Def::Pass1) => self.pass == 1,
            Some(_) => true,
        }
    }

    /// `getvalue` where a value is required: any definition will do.
    fn value(&self, name: &str) -> Option<i64> {
        self.symbols
            .get(&self.scopes.resolve(name.to_owned()))
            .map(|symbol| symbol.value)
    }

    fn pc(&self) -> Option<i64> {
        Some(self.pc)
    }
}

impl Host for Walk {
    /// An `IF` condition or `REPT` count, which PasmoNext needs now.
    fn evaluate(&mut self, expr: &SourceExpr) -> Result<i64, String> {
        let ir = lower::expression(expr, &self.profile, self)
            .map_err(|diagnostic| diagnostic.message().to_owned())?;
        let missing = RefCell::new(None);
        let resolve = |name: &str| {
            let value = Known::value(self, name);
            if value.is_none() {
                missing.borrow_mut().get_or_insert_with(|| name.to_owned());
            }
            value
        };
        ir.eval_with(&resolve, Some(self.pc), 0)
            .map_err(|error| match missing.into_inner() {
                Some(name) => format!("Symbol '{name}' is undefined"),
                None => error.message,
            })
    }

    /// A `REPT` counter, which PasmoNext makes local to its repeat and sets
    /// with `setdefl`.
    fn set_counter(&mut self, name: &str, value: i64) {
        let value = value & 0xFFFF;
        let set = self
            .scopes
            .counter(name)
            .and_then(|name| self.define_defl(&name, value).map(|()| name));
        match set {
            Ok(name) => self.counters.push((name, value)),
            Err(message) => {
                self.pending.get_or_insert(message);
            }
        }
    }

    /// A macro call, `REPT` or `IRP` is a scope: PasmoNext's `MacroLevel`.
    fn enter(&mut self, _: Block) {
        self.scopes.enter_block();
    }

    /// The block's scope ends, and with it any `PROC` left open inside it.
    fn leave(&mut self) {
        self.scopes.leave_block();
    }

    /// Under `--alocal`, defining a macro ends the automatic scope.
    fn macro_defined(&mut self, name: &str) {
        if let Err(message) = self.scopes.macro_defined(name) {
            self.pending.get_or_insert(message);
        }
    }
}

/// The name a line's label defines, if PasmoNext defines it before reading
/// the rest of the line: on every line but `ORG`, `EQU` and `DEFL`, which
/// read their value first.
fn label_first(line: &ActiveLine, profile: Profile) -> Option<String> {
    let syntax = line.line();
    let label = syntax.label()?.name()?;
    let value_first = match syntax.statement() {
        Some(SourceStatement::Directive(directive)) => directive.keyword().is_some_and(|k| {
            ["ORG", "EQU", "DEFL"]
                .iter()
                .any(|word| k.text().eq_ignore_ascii_case(word))
        }),
        _ => false,
    };
    (!value_first).then(|| profile.name(label.text()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(pass: u8) -> Walk {
        let mut walk = Walk::new(false, vec![FileId(0)], &Options::default()).expect("no options");
        walk.pass = pass;
        walk
    }

    const LABEL: &str = "Invalid definition, previously defined as EQU or label";
    const DEFL: &str = "Invalid definition, previously defined as DEFL";

    #[test]
    fn a_name_is_defined_once_per_pass_as_setequorlabel_allows() {
        let mut first = walk(1);
        first.define("x", 1).expect("first");
        assert_eq!(first.define("x", 1), Err(LABEL.to_owned()));
        assert_eq!(first.define_defl("x", 2), Err(LABEL.to_owned()));
        // Pass 2 redefines what pass 1 defined, once.
        first.pass = 2;
        first.define("x", 1).expect("again in pass 2");
        assert_eq!(first.define("x", 1), Err(LABEL.to_owned()));
        assert_eq!(first.define_defl("x", 2), Err(LABEL.to_owned()));
    }

    #[test]
    fn a_defl_name_may_be_set_again_but_never_become_a_label() {
        let mut walk = walk(1);
        walk.define_defl("n", 1).expect("first");
        walk.define_defl("n", 2).expect("again");
        assert_eq!(walk.define("n", 3), Err(DEFL.to_owned()));
    }

    #[test]
    fn a_name_given_by_equ_is_defined_before_pass_one_and_never_again() {
        // equ_redefined_by_source and equ_redefined_by_defl, measured on
        // PasmoNext 0.1.3.
        let options = Options {
            predefined: vec!["aa=1".into(), "flag".into()],
            ..Options::default()
        };
        let mut walk = Walk::new(false, vec![FileId(0)], &options).expect("predefines");
        assert_eq!(walk.value("aa"), Some(1));
        assert_eq!(walk.value("flag"), Some(0xFFFF));
        assert!(walk.defined("aa"));
        assert_eq!(
            walk.define("aa", 2),
            Err("Can't redefine, is predefined".to_owned())
        );
        assert_eq!(walk.define_defl("aa", 2), Err(LABEL.to_owned()));
        let refused = Options {
            predefined: vec!["aa=1+1".into()],
            ..Options::default()
        };
        assert!(Walk::new(false, vec![FileId(0)], &refused).is_err());
    }

    #[test]
    fn a_name_pass_one_never_defined_cannot_appear_in_pass_two() {
        let mut walk = walk(2);
        assert_eq!(
            walk.define("late", 0),
            Err("The symbol 'late' must be defined in pass 1".to_owned())
        );
    }

    #[test]
    fn a_value_that_moves_in_pass_two_needs_a_third() {
        let mut walk = walk(1);
        walk.define("x", 0).expect("pass 1");
        walk.pass = 2;
        let moved = walk.define("x", 5).expect_err("moved");
        assert!(moved.contains("third pass"), "{moved}");
    }

    #[test]
    fn defined_means_this_pass_and_reading_an_unknown_name_is_zero_only_in_pass_one() {
        let mut walk = walk(1);
        walk.define("x", 7).expect("pass 1");
        assert!(walk.defined("x"));
        assert_eq!(walk.read(&Expr::Sym("later".into())), Ok(0));
        walk.pass = 2;
        assert!(!walk.defined("x"));
        assert_eq!(walk.value("x"), Some(7));
        assert_eq!(
            walk.read(&Expr::Sym("later".into())),
            Err("Symbol 'later' is undefined".to_owned())
        );
    }
}
