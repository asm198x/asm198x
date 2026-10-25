//! PasmoNext's scopes: its `localstack` of `LocalLevel`s, and the names local
//! to each. Every rule here was measured on PasmoNext 0.1.3.
//!
//! A name made local is assembled under a name of its own, which
//! `genlocalname` makes from a counter: eight hex digits, which no source name
//! can be. The counter starts again each pass, so each pass gives each local
//! the same name.
//!
//! Under `--alocal`, a name starting `_` or `@` is local to an automatic scope.
//! Merely reading one opens that scope, so the scopes are kept behind `&self`:
//! the front end reads names through [`lower::Known`](lang198x_z80_asm::lower::Known),
//! which takes `&self`.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::span::Span;

/// The scopes open in one pass.
pub(super) struct Scopes {
    /// Whether names starting `_` or `@` are automatic locals (`--alocal`).
    automatic: bool,
    inner: RefCell<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Innermost last.
    levels: Vec<Level>,
    /// How many local names this pass has made: `localcount`.
    made: u32,
}

struct Level {
    kind: Kind,
    /// Each name made local here, and the name it is assembled under.
    names: HashMap<String, String>,
}

#[derive(PartialEq)]
enum Kind {
    /// A macro call, `REPT` or `IRP`: `MacroLevel`.
    Block,
    /// `PROC`, written at this place: `ProcLevel`.
    Proc(Span),
    /// The scope automatic locals share: `AutoLevel`.
    Automatic,
}

/// PasmoNext's `InvalidInAutolocal`.
const INVALID_IN_AUTOMATIC: &str = "Invalid use of name in autolocal mode";

impl Scopes {
    pub(super) fn new(automatic: bool) -> Self {
        Self {
            automatic,
            inner: RefCell::new(Inner::default()),
        }
    }

    /// A new pass: no scope open, and the counter back at 0.
    pub(super) fn reset(&self) {
        *self.inner.borrow_mut() = Inner::default();
    }

    /// `isautolocalname`.
    fn is_automatic(&self, name: &str) -> bool {
        self.automatic && (name.starts_with('_') || name.starts_with('@'))
    }

    /// The name `name` is assembled under, where the source reads it. Under
    /// `--alocal` an automatic local is made local by being read
    /// (`checkautolocal`).
    pub(super) fn resolve(&self, name: String) -> String {
        if self.is_automatic(&name) {
            self.automatic_local(&name);
        }
        self.inner
            .borrow()
            .levels
            .iter()
            .rev()
            .find_map(|level| level.names.get(&name).cloned())
            .unwrap_or(name)
    }

    /// `name` is being defined, as a label or by `EQU` or `DEFL`
    /// (`setequorlabel`, `setdefl`). Under `--alocal`, an automatic local
    /// joins the automatic scope, and any other name ends it.
    pub(super) fn defining(&self, name: &str) {
        if self.is_automatic(name) {
            self.automatic_local(name);
        } else {
            self.end_automatic();
        }
    }

    /// `enterautolocal`, then `LocalLevel::add`.
    fn automatic_local(&self, name: &str) {
        let mut inner = self.inner.borrow_mut();
        if inner
            .levels
            .last()
            .is_none_or(|level| level.kind != Kind::Automatic)
        {
            inner.levels.push(Level {
                kind: Kind::Automatic,
                names: HashMap::new(),
            });
        }
        inner.add(name);
    }

    /// `finishautolocal`: the automatic scope, if it is the innermost, ends.
    fn end_automatic(&self) {
        let mut inner = self.inner.borrow_mut();
        if inner
            .levels
            .last()
            .is_some_and(|level| level.kind == Kind::Automatic)
        {
            inner.levels.pop();
        }
    }

    /// `LOCAL names` (`parseLOCAL`): from the next line, each is local to the
    /// innermost scope.
    pub(super) fn local(&self, names: &[String]) -> Result<(), String> {
        self.end_automatic();
        if self.inner.borrow().levels.is_empty() {
            return Err("Trying to use a non existent local level".into());
        }
        for name in names {
            if self.is_automatic(name) {
                return Err(INVALID_IN_AUTOMATIC.into());
            }
            self.inner.borrow_mut().add(name);
        }
        Ok(())
    }

    /// `PUBLIC names` (`parsePUBLIC`) changes no scope, but an automatic
    /// local cannot be public.
    pub(super) fn public(&self, names: &[String]) -> Result<(), String> {
        if names.iter().any(|name| self.is_automatic(name)) {
            return Err(INVALID_IN_AUTOMATIC.into());
        }
        Ok(())
    }

    /// `PROC`, written at `at` (`parsePROC`).
    pub(super) fn proc(&self, at: Span) {
        self.end_automatic();
        self.inner.borrow_mut().levels.push(Level {
            kind: Kind::Proc(at),
            names: HashMap::new(),
        });
    }

    /// `ENDP` (`parseENDP`): the innermost scope must be a `PROC`.
    pub(super) fn endp(&self) -> Result<(), String> {
        self.end_automatic();
        let mut inner = self.inner.borrow_mut();
        if !matches!(
            inner.levels.last(),
            Some(Level {
                kind: Kind::Proc(_),
                ..
            })
        ) {
            return Err("Unbalanced ENDP".into());
        }
        inner.levels.pop();
        Ok(())
    }

    /// A macro named `name` is being defined (`parseMACRO`).
    pub(super) fn macro_defined(&self, name: &str) -> Result<(), String> {
        self.end_automatic();
        if self.is_automatic(name) {
            return Err(INVALID_IN_AUTOMATIC.into());
        }
        Ok(())
    }

    /// A `REPT` counter, local to its repeat (`parseREPT`). Returns the name
    /// it is assembled under.
    pub(super) fn counter(&self, name: &str) -> Result<String, String> {
        if self.is_automatic(name) {
            return Err(INVALID_IN_AUTOMATIC.into());
        }
        self.inner.borrow_mut().add(name);
        Ok(self.resolve(name.to_owned()))
    }

    /// A macro call, `REPT` or `IRP` starts: `MacroLevel`.
    pub(super) fn enter_block(&self) {
        self.inner.borrow_mut().levels.push(Level {
            kind: Kind::Block,
            names: HashMap::new(),
        });
    }

    /// The block ends, and with it any `PROC` or automatic scope left open
    /// inside it (`~MacroFrameBase`).
    pub(super) fn leave_block(&self) {
        let mut inner = self.inner.borrow_mut();
        while let Some(level) = inner.levels.pop() {
            if level.kind == Kind::Block {
                break;
            }
        }
    }

    /// The end of a pass. A `PROC` still open is an error where it was
    /// written.
    pub(super) fn finish(&self) -> Result<(), (Option<Span>, &'static str)> {
        self.end_automatic();
        match self.inner.borrow_mut().levels.pop() {
            None => Ok(()),
            Some(Level {
                kind: Kind::Proc(at),
                ..
            }) => Err((Some(at), "Unbalanced PROC")),
            Some(_) => Err((None, "Unexpected local block encountered")),
        }
    }
}

impl Inner {
    /// `LocalLevel::add`: `name` is local to the innermost scope, under a
    /// name of its own. Naming it again in the same scope changes nothing.
    /// The caller has made sure there is a scope.
    fn add(&mut self, name: &str) {
        let made = self.made;
        let Some(level) = self.levels.last_mut() else {
            return;
        };
        if !level.names.contains_key(name) {
            level.names.insert(name.to_owned(), format!("{made:08X}"));
            self.made += 1;
        }
    }
}
