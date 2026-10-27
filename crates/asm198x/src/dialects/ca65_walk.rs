//! ca65 V2.19 through Lang198x's ca65 front end.
//!
//! `lang198x-ca65` reads ca65 source, runs its preprocessing, and lowers
//! each line to statements. This walk does what that crate leaves to an
//! assembler, the way PasmoNext's walk does for the `pasmo` dialect: it
//! keeps what ca65 knows of each name as it reads the program once, from
//! the top, so lowering can choose an operand's size as ca65 does, and it
//! names cheap locals and unnamed labels. The engine then assembles the
//! statements in two passes.
//!
//! This is step 6b of `docs/plans/2026-10-27-ca65-walk/plan.md`: flat
//! programs, in one segment. Scopes, segments and visibility come in 6c and
//! 6d. The old reader in `dialects/ca65` stays the default until then.

use std::collections::{HashMap, HashSet};

use lang198x_ca65::Profile;
use lang198x_ca65::ast::{Expr as SourceExpr, NameRef};
use lang198x_ca65::expand::{Expander, Host};
use lang198x_ca65::lower::{self, Known, Size, Value};
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

/// What the walk knows as it reads the program.
#[derive(Default)]
struct Walk {
    /// What is known of each name defined so far, by its full name.
    names: HashMap<String, Value>,
    /// Names read so far, for `.ifref`.
    referenced: HashSet<String>,
    /// Names `.ifconst` asked about, with where. ca65 counts the question
    /// as a use, so each must be defined by the end.
    asked: Vec<(String, Span)>,
    /// Where the line being read is, for what the host is asked about it.
    here: Option<Span>,
    /// The address, while `.org` makes it known.
    org: Option<i64>,
    /// The last name a label defined that was not a cheap local, which a
    /// cheap local belongs to.
    scope: String,
    /// How many unnamed labels have been defined.
    unnamed: usize,
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
            for item in lowered.items {
                let op = item.op.map(|op| self.names_in(op));
                let label = item.label.map(|name| self.define_label(&name));
                let op = self.apply(label.as_deref(), op, &at)?;
                statements.push(statement(&at, label, op));
            }
        }
        if let Some((name, at)) = self
            .asked
            .iter()
            .find(|(name, _)| !self.names.contains_key(name))
        {
            return Err(AsmError::at(
                at.clone(),
                format!("Symbol `{name}` is undefined"),
            ));
        }
        Ok((statements, Vec::new()))
    }

    /// The full name `name`, as written, stands for here: a cheap local
    /// belongs to the last label, and an unnamed label's reference counts
    /// the unnamed labels before or after it.
    fn resolve(&self, name: &str) -> String {
        if let Some(rest) = name.strip_prefix(':') {
            let back = rest.chars().filter(|&c| c == '-').count();
            let forward = rest.chars().filter(|&c| c == '+').count();
            let index = if back > 0 {
                self.unnamed.checked_sub(back)
            } else {
                Some(self.unnamed + forward.saturating_sub(1))
            };
            return match index {
                Some(index) => unnamed(index),
                // No unnamed label that far back: a name nothing defines,
                // so the engine reports it.
                None => name.to_owned(),
            };
        }
        if name.starts_with('@') {
            return format!("{}{name}", self.scope);
        }
        name.strip_prefix("::").unwrap_or(name).to_owned()
    }

    /// `op` with each name it reads resolved, and recorded as read.
    fn names_in(&mut self, op: Operation) -> Operation {
        map_syms(op, &mut |name| {
            let full = self.resolve(&name);
            self.referenced.insert(full.clone());
            Expr::Sym(full)
        })
    }

    /// The full name a label defines, which moves the cheap-local scope or
    /// counts an unnamed label.
    fn define_label(&mut self, name: &str) -> String {
        if name == ":" {
            self.unnamed += 1;
            return unnamed(self.unnamed - 1);
        }
        let full = self.resolve(name);
        if !name.starts_with('@') {
            self.scope.clone_from(&full);
        }
        full
    }

    /// Records what a statement tells the walk: its label's value, and the
    /// address. `.org` becomes the engine's `PseudoPc`, since ca65 moves
    /// only what addresses read as, not where bytes go.
    fn apply(
        &mut self,
        label: Option<&str>,
        op: Option<Operation>,
        at: &Span,
    ) -> Result<Option<Operation>, AsmError> {
        let refuse = |message: String| AsmError::at(at.clone(), message);
        if let Some(label) = label {
            let value = match &op {
                Some(Operation::Equ(value) | Operation::Set(value)) => match self.constant(value) {
                    Some(value) => Value::Constant(value),
                    None => Value::Address(Size::Absolute),
                },
                _ => match self.org {
                    Some(pc) => Value::Constant(pc),
                    None => Value::Address(Size::Absolute),
                },
            };
            let redefinable = matches!(op, Some(Operation::Set(_)));
            if !redefinable && self.names.contains_key(label) {
                return Err(refuse(format!("Symbol `{label}` is already defined")));
            }
            self.names.insert(label.to_owned(), value);
        }
        let op = match op {
            Some(Operation::Org(address)) => {
                let value = self
                    .constant(&address)
                    .ok_or_else(|| refuse("Constant expression expected".to_owned()))?;
                // ca65 keeps addresses in 24 bits.
                if !(0..=0xFF_FFFF).contains(&value) {
                    return Err(refuse("Range error".to_owned()));
                }
                self.org = Some(value);
                return Ok(Some(Operation::PseudoPc(Some(Expr::Num(value)))));
            }
            other => other,
        };
        if let (Some(pc), Some(op)) = (self.org, &op)
            && !matches!(op, Operation::Equ(_) | Operation::Set(_))
        {
            let next = next_pc(op, pc, &isa::mos6502::SET, None, 1, at.line as usize)
                .map_err(|error| refuse(error.message))?;
            self.org = Some(next);
        }
        Ok(op)
    }

    /// The value of `expr` if every name in it is a constant now.
    fn constant(&self, expr: &Expr) -> Option<i64> {
        let value = |name: &str| match self.names.get(name) {
            Some(Value::Constant(value)) => Some(*value),
            _ => None,
        };
        expr.eval_with(&value, self.org, 0).ok()
    }
}

/// The name the `index`th unnamed label defines. ca65 gives them no names a
/// person can write, and neither does this.
fn unnamed(index: usize) -> String {
    format!(":{index}")
}

impl Known for Walk {
    fn name(&self, name: &str) -> Value {
        self.names
            .get(&self.resolve(name))
            .copied()
            .unwrap_or(Value::Unknown)
    }

    fn pc(&self) -> Value {
        match self.org {
            Some(pc) => Value::Constant(pc),
            None => Value::Address(Size::Absolute),
        }
    }
}

impl Host for Walk {
    /// A condition or count, which ca65 needs now: a constant expression.
    fn evaluate(&mut self, expr: &SourceExpr) -> Result<i64, String> {
        let lowered = lower::expression(expr).map_err(|d| d.message().to_owned())?;
        let lowered = map_sym_expr(lowered, &mut |name| Expr::Sym(self.resolve(&name)));
        self.constant(&lowered)
            .ok_or_else(|| "Constant expression expected".to_owned())
    }

    fn is_defined(&mut self, name: &NameRef) -> bool {
        self.names.contains_key(&self.resolve(&name.text()))
    }

    fn is_referenced(&mut self, name: &NameRef) -> bool {
        self.referenced.contains(&self.resolve(&name.text()))
    }

    /// `.ifconst`, which ca65 counts as a use of each name it reads.
    fn is_constant(&mut self, expr: &SourceExpr) -> bool {
        if let Ok(lowered) = lower::expression(expr) {
            let here = self
                .here
                .clone()
                .unwrap_or_else(|| Span::in_file(FileId(0), 0, 0));
            let mut asked = Vec::new();
            map_sym_expr(lowered, &mut |name| {
                asked.push((self.resolve(&name), here.clone()));
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
