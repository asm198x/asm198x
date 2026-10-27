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
//! This is step 6c of `docs/plans/2026-10-27-ca65-walk/plan.md`: flat
//! programs with scopes, in one segment. Segments and visibility come next.
//! The old reader in `dialects/ca65` stays the default until then.
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

use std::collections::{HashMap, HashSet};

use lang198x_ca65::Profile;
use lang198x_ca65::ast::{Expr as SourceExpr, NameRef};
use lang198x_ca65::expand::{Expander, Host};
use lang198x_ca65::lower::{self, Known, Scope, Size, Value};
use lang198x_ca65::syntax::{self, DiagnosticKind};

use crate::ast::{map_sym_expr, map_syms};
use crate::dialect::Dialect;
use crate::engine::{AsmError, Expr, Operation, Statement, Warning, next_pc};
use crate::span::{FileId, Span};

const PROFILE: Profile = Profile::CA65_2_19_6502;

/// The ca65 dialect through Lang198x's front end, for the 6502.
pub(crate) struct Ca65Walk;

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
        Walk::default().run(source)
    }

    fn equ_label_colon(&self) -> bool {
        false
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

/// What the walk knows as it reads the program.
#[derive(Default)]
struct Walk {
    /// What is known of each name defined so far, by its full name.
    names: HashMap<String, Value>,
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
    /// Names `.ifconst` asked about, with where. ca65 counts the question
    /// as a use, so each must be defined by the end.
    asked: Vec<(String, Span)>,
    /// Names bound at the end of the program.
    pending: Vec<Pending>,
    /// Where the line being read is, for what the host is asked about it.
    here: Option<Span>,
    /// The address, while `.org` makes it known.
    org: Option<i64>,
    /// How many bytes the program has made.
    made: i64,
    /// The full name of the last label that was not a cheap local, which a
    /// cheap local belongs to.
    cheap: String,
    /// How many unnamed labels have been defined.
    unnamed: usize,
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

impl Walk {
    fn run(mut self, source: &str) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
        let mut expander = Expander::new(source, PROFILE)
            .map_err(|_| AsmError::new(0, "the program is too long to parse"))?;
        let mut statements = Vec::new();
        loop {
            let line = expander.next_line(&mut self);
            check(&expander)?;
            let Some(line) = line else { break };
            let at = place(&expander, line.span());
            self.here = Some(at.clone());
            let lowered = lower::line(&line, PROFILE, &self).map_err(|diagnostic| {
                let span = syntax::Span::new(line.text(), diagnostic.range());
                let message = match diagnostic.kind() {
                    DiagnosticKind::Unsupported => {
                        format!("{} (Lang198x's ca65 front end)", diagnostic.message())
                    }
                    _ => diagnostic.message().to_owned(),
                };
                AsmError::at(place(&expander, span), message)
            })?;
            let made = self.made;
            let mut labels = Vec::new();
            for item in lowered.items {
                // What a label is known as comes from the line as written,
                // before its names are bound.
                let value = item.op.as_ref().map(|op| self.value_of(op));
                let label = item.label.map(|name| self.define_label(&name));
                let op = item.op.map(|op| self.names_in(op));
                let sized = !matches!(op, Some(Operation::Equ(_) | Operation::Set(_)));
                let op = self.apply(label.as_deref(), value, op, &at)?;
                if let (Some(label), true) = (&label, sized) {
                    labels.push(label.clone());
                }
                statements.push(statement(&at, label, op));
            }
            let line_size = self.made - made;
            for label in labels {
                self.sizes.insert(label, line_size);
            }
            if let Some(change) = lowered.scope {
                self.change_scope(change, &at)?;
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
        Ok((statements, Vec::new()))
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

    /// Records what a statement tells the walk: its label's value, and the
    /// address. `.org` becomes the engine's `PseudoPc`, since ca65 moves
    /// only what addresses read as, not where bytes go.
    fn apply(
        &mut self,
        label: Option<&str>,
        value: Option<Value>,
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
            self.names.insert(label.to_owned(), value);
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
                return Ok(Some(Operation::PseudoPc(Some(Expr::Num(value)))));
            }
            other => other,
        };
        if let Some(op) = &op
            && !matches!(op, Operation::Equ(_) | Operation::Set(_))
        {
            let pc = self.org.unwrap_or(self.made);
            let next = next_pc(op, pc, &isa::mos6502::SET, None, 1, at.line as usize)
                .map_err(|error| refuse(error.message))?;
            self.made += next - pc;
            if self.org.is_some() {
                self.org = Some(next);
            }
        }
        Ok(op)
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

/// The name the `index`th unnamed label defines. ca65 gives them no names a
/// person can write, and neither does this.
fn unnamed(index: usize) -> String {
    format!(":{index}")
}

impl Known for Walk {
    /// A plain name this scope has not defined is known by its address size
    /// only, from the innermost scope further out that has.
    fn name(&self, name: &str) -> Value {
        match self.lookup(name) {
            Lookup::Full(full) => self.names.get(&full).copied().unwrap_or(Value::Unknown),
            Lookup::Plain(name) => match self.defined(&name) {
                Some(full) if full == qualify(self.path(), &name) => self.names[&full],
                Some(full) => match self.names[&full] {
                    Value::Constant(value) => Value::Address(Size::of(value)),
                    other => other,
                },
                None => Value::Unknown,
            },
        }
    }

    fn pc(&self) -> Value {
        match self.org {
            Some(pc) => Value::Constant(pc),
            None => Value::Address(Size::Absolute),
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
}

impl Host for Walk {
    /// A condition or count, which ca65 needs now: a constant expression.
    fn evaluate(&mut self, expr: &SourceExpr) -> Result<i64, String> {
        let lowered = lower::expression(expr, &*self).map_err(|d| d.message().to_owned())?;
        match lower::value(&lowered, &*self) {
            Value::Constant(value) => Ok(value),
            _ => Err("Constant expression expected".to_owned()),
        }
    }

    fn is_defined(&mut self, name: &NameRef) -> bool {
        match self.lookup(&name.text()) {
            Lookup::Full(full) => self.names.contains_key(&full),
            Lookup::Plain(name) => self.defined(&name).is_some(),
        }
    }

    fn is_referenced(&mut self, name: &NameRef) -> bool {
        match self.lookup(&name.text()) {
            Lookup::Full(full) => self.referenced.contains(&full),
            Lookup::Plain(name) => {
                outwards(self.path()).any(|path| self.referenced.contains(&qualify(path, &name)))
            }
        }
    }

    /// `.ifconst`, which ca65 counts as a use of each name it reads.
    fn is_constant(&mut self, expr: &SourceExpr) -> bool {
        if let Ok(lowered) = lower::expression(expr, &*self) {
            let here = self
                .here
                .clone()
                .unwrap_or_else(|| Span::in_file(FileId(0), 0, 0));
            let mut asked = Vec::new();
            map_sym_expr(lowered, &mut |name| {
                asked.push((self.reference(&name), here.clone()));
                Expr::Sym(name)
            });
            self.asked.extend(asked);
        }
        self.evaluate(expr).is_ok()
    }
}

/// Stops at the expander's first problem.
fn check(expander: &Expander) -> Result<(), AsmError> {
    match expander.diagnostics().first() {
        Some((text, diagnostic)) => {
            let span = syntax::Span::new(*text, diagnostic.range());
            Err(AsmError::at(place(expander, span), diagnostic.message()))
        }
        None => Ok(()),
    }
}

/// Where `span` was written. This step reads one file.
fn place(expander: &Expander, span: syntax::Span) -> Span {
    match expander.locate(span) {
        Some(at) => Span::in_file(FileId(0), at.line, at.column),
        None => Span::in_file(FileId(0), 0, 0),
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
