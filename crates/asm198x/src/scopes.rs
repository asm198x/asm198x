//! The shared scoped symbol environment (#484) —
//! `decisions/scoped-symbol-environment.md`.
//!
//! Scope *events* stay dialect-spelled — a dialect declares where a scope
//! opens, closes, or re-anchors — and this module owns what those events mean
//! for lookup. The semantics here are the Z80 family's, probe-pinned against
//! sjasmplus 1.21.0 (probes m4, m8, m13, m15, m25, m29, m31); each dialect
//! that migrates onto the environment brings its own declared policy, never
//! another dialect's rule generalised to it.
//!
//! The environment resolves names to qualified spellings; values stay in the
//! engine's flat symbol table under those spellings, so the expression
//! evaluator is untouched — resolution happens before evaluation.

use std::collections::{BTreeMap, BTreeSet};

use crate::engine::Operation;
use crate::span::FileId;

/// One open named scope (a sjasmplus `MODULE` today; the caller validates the
/// name before opening — a bad spelling is the dialect's diagnostic).
pub(crate) struct OpenScope {
    pub name: String,
    /// Where it was opened, so leaving it open can be reported against the
    /// line that did.
    pub line: usize,
    pub file: FileId,
}

/// The scoped symbol environment: the re-anchorable current global that
/// locals qualify under, the stack of open named scopes whose dotted join
/// prefixes definitions and references inside, and the qualified→bare
/// fallbacks a stream-end repair chooses between.
#[derive(Default)]
pub(crate) struct ScopeEnv {
    /// The most recent global (non-local) label, kept *unprefixed*: the scope
    /// prefix wraps the result, so a local under `glob` inside `foo` is
    /// `foo.glob.loc` (probe m25).
    anchor: Option<String>,
    /// Open named scopes, outermost first.
    scopes: Vec<OpenScope>,
    /// Scope-qualified reference → the bare name it falls back to. The
    /// reference tries the qualified name first and the *global* name second,
    /// with no walk-up through intermediate levels (probes m8/m13/m31); which
    /// one is right depends on what ends up defined, including by a
    /// definition the walk has not reached yet, so the choice is repaired by
    /// [`ScopeEnv::alias_repair`] once the whole stream is known.
    aliases: BTreeMap<String, String>,
    /// Compound bindings by qualified name (#484 rule 3) — structures today.
    structs: BTreeMap<String, StructDef>,
}

impl ScopeEnv {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The current global as a bare string, `""` when none — the spelling
    /// ca65's cheap-local key mechanics use, where a cheap local before any
    /// global keys under the empty anchor.
    pub(crate) fn anchor_str(&self) -> &str {
        self.anchor.as_deref().unwrap_or("")
    }

    /// Re-anchor: the event a plain (non-local) label raises. The caller
    /// hands the name in whatever qualified spelling its scope rules
    /// produced; the environment owns only what the anchor means.
    pub(crate) fn set_anchor(&mut self, name: String) {
        self.anchor = Some(name);
    }

    /// Swap the anchor for a scope's own — ca65's `.proc`/`.scope` open
    /// saves the outer anchor and anchors the scope's path; its closer hands
    /// the saved value back. `""` is the no-anchor spelling on both sides.
    pub(crate) fn replace_anchor(&mut self, new: String) -> String {
        let old = self.anchor.take().unwrap_or_default();
        self.anchor = (!new.is_empty()).then_some(new);
        old
    }

    /// The dotted prefix the open scopes impose, `""` when none are open (so
    /// a dialect without them pays only an empty `format!`).
    pub(crate) fn prefix(&self) -> String {
        if self.scopes.is_empty() {
            String::new()
        } else {
            let names: Vec<&str> = self.scopes.iter().map(|m| m.name.as_str()).collect();
            format!("{}.", names.join("."))
        }
    }

    /// Open a named scope. Nothing is emitted: a scope is a naming rule, not
    /// an operation.
    pub(crate) fn open(&mut self, name: &str, line: usize, file: FileId) {
        self.scopes.push(OpenScope {
            name: name.to_string(),
            line,
            file,
        });
    }

    /// Close the innermost scope; `false` when none is open — the dialect
    /// spells the diagnostic.
    pub(crate) fn close(&mut self) -> bool {
        self.scopes.pop().is_some()
    }

    /// The innermost scope left open at the end of the walk, if any, named by
    /// its full dotted path — one advisory naming that, not one per level.
    pub(crate) fn unclosed(&self) -> Option<(String, usize, FileId)> {
        let last = self.scopes.last()?;
        let names: Vec<&str> = self.scopes.iter().map(|m| m.name.as_str()).collect();
        Some((names.join("."), last.line, last.file))
    }

    /// Qualify a label's *defined* name with the live environment: under a
    /// `locals` policy, a leading-`.` local qualifies under the current
    /// global and a plain name re-anchors; under a `scoped` policy, `@name`
    /// opts out of both scopes and defines the bare global name (probes
    /// m4/m15 — it does not become the current global either), and anything
    /// else takes the open scopes' prefix.
    pub(crate) fn define(&mut self, name: String, locals: bool, scoped: bool) -> String {
        if scoped && let Some(bare) = name.strip_prefix('@') {
            return bare.to_string();
        }
        let qualified = if locals && name.starts_with('.') {
            match &self.anchor {
                Some(g) => format!("{g}{name}"),
                None => name,
            }
        } else {
            if locals {
                self.anchor = Some(name.clone());
            }
            name
        };
        format!("{}{qualified}", self.prefix())
    }

    /// Qualify every name an operation *references*: locals under the current
    /// global first, then the scope prefix wrapping the result — matching the
    /// definition side. Each scope-qualified reference records its bare
    /// fallback for [`ScopeEnv::alias_repair`].
    pub(crate) fn qualify_op(&mut self, op: Operation, locals: bool, scoped: bool) -> Operation {
        let op = if locals && let Some(g) = self.anchor.clone() {
            crate::ast::qualify_locals(op, &g)
        } else {
            op
        };
        if !scoped {
            return op;
        }
        let prefix = self.prefix();
        let aliases = &mut self.aliases;
        crate::ast::map_syms(op, &mut |s| {
            crate::engine::Expr::Sym(scope_ref(s, &prefix, aliases))
        })
    }

    /// The repairs the stream's end settles: a reference keeps its qualified
    /// spelling unless that name is undefined *and* the bare one is defined.
    /// Keeping it when neither exists is what makes the error name the same
    /// candidate the reference names.
    pub(crate) fn alias_repair<'a>(
        &'a self,
        defined: &BTreeSet<String>,
    ) -> BTreeMap<&'a str, &'a str> {
        self.aliases
            .iter()
            .filter(|(q, bare)| !defined.contains(*q) && defined.contains(*bare))
            .map(|(q, bare)| (q.as_str(), bare.as_str()))
            .collect()
    }

    /// Whether any qualified→bare fallbacks were recorded at all.
    pub(crate) fn has_aliases(&self) -> bool {
        !self.aliases.is_empty()
    }

    /// Bind a structure under its qualified name.
    pub(crate) fn bind_struct(&mut self, name: String, def: StructDef) {
        self.structs.insert(name, def);
    }

    /// The structure bound to `name`, if one is.
    pub(crate) fn struct_def(&self, name: &str) -> Option<&StructDef> {
        self.structs.get(name)
    }
}

/// A compound binding (#484 rule 3): a structure binds a total size plus
/// named member offsets, resolved to plain values at reference time so the
/// expression evaluator is untouched — the dialect exports each name as an
/// ordinary constant, and this record is what it exports from.
pub(crate) struct StructDef {
    /// `db Name` answers this — the total, an initial offset included.
    pub size: i64,
    /// Every named offset the definition binds, in layout order: members,
    /// and an embedded member's flattened paths (`hb`, `hb.x0`, `hb.y0`).
    pub names: Vec<(String, i64)>,
    /// Emission order for an instantiation.
    pub leaves: Vec<StructLeaf>,
}

/// One run of bytes an instantiation lays down: a member, or the reserve a
/// `DS` member makes.
pub(crate) struct StructLeaf {
    /// The member's dotted path under the instance, if it is named.
    pub path: Option<String>,
    /// What it emits when the instantiation gives no value: the member's
    /// default, little-endian where multi-byte.
    pub bytes: Vec<u8>,
    /// Whether a `{ … }` initialiser list value lands here. A `DS`/`BLOCK`
    /// member reserves without taking a slot (probed, #548).
    pub slot: bool,
}

/// Rewrite one reference under the open scopes: `@name` escapes to the bare
/// global name, anything else is qualified and its bare fallback recorded for
/// [`ScopeEnv::alias_repair`] to choose between.
fn scope_ref(name: String, prefix: &str, aliases: &mut BTreeMap<String, String>) -> String {
    if let Some(bare) = name.strip_prefix('@') {
        return bare.to_string();
    }
    if prefix.is_empty() {
        return name;
    }
    let qualified = format!("{prefix}{name}");
    aliases.insert(qualified.clone(), name);
    qualified
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Z80-family policy, exactly as probed: locals qualify under the
    /// anchor, plain names re-anchor, scopes prefix both sides.
    #[test]
    fn defines_qualify_the_way_sjasmplus_does() {
        let mut env = ScopeEnv::new();
        assert_eq!(env.define("glob".into(), true, true), "glob");
        assert_eq!(env.define(".loc".into(), true, true), "glob.loc");
        env.open("foo", 1, FileId(0));
        // Probe m25: the scope prefix wraps the local rule's result.
        assert_eq!(env.define("g2".into(), true, true), "foo.g2");
        assert_eq!(env.define(".l2".into(), true, true), "foo.g2.l2");
        // Probes m4/m15: `@name` escapes to the bare global and does not
        // re-anchor.
        assert_eq!(env.define("@bare".into(), true, true), "bare");
        assert_eq!(env.define(".l3".into(), true, true), "foo.g2.l3");
        assert!(env.close());
        assert!(!env.close(), "nothing left to close");
    }

    /// A dialect that declares no scoping sees nothing change.
    #[test]
    fn no_policy_means_no_change() {
        let mut env = ScopeEnv::new();
        assert_eq!(env.define("name".into(), false, false), "name");
        assert_eq!(env.define(".dotted".into(), false, false), ".dotted");
        assert_eq!(env.define("@at".into(), false, false), "@at");
    }

    /// A local defined before any global stays bare — there is nothing to
    /// qualify it under.
    #[test]
    fn a_local_before_any_global_stays_bare() {
        let mut env = ScopeEnv::new();
        assert_eq!(env.define(".orphan".into(), true, false), ".orphan");
    }

    /// The repair rule, probes m8/m13/m31: a qualified reference falls back
    /// to its bare name only when the qualified one is undefined and the
    /// bare one is defined — with no walk-up through intermediate levels.
    #[test]
    fn alias_repair_prefers_the_qualified_name() {
        let mut env = ScopeEnv::new();
        env.open("m", 1, FileId(0));
        let op = Operation::Bytes(vec![
            crate::engine::Expr::Sym("hit".into()),
            crate::engine::Expr::Sym("miss".into()),
        ]);
        let out = env.qualify_op(op, true, true);
        let Operation::Bytes(es) = &out else {
            panic!("shape preserved");
        };
        assert_eq!(es.len(), 2);
        // `m.hit` ends up defined, `m.miss` does not — only `miss` repairs.
        let defined: BTreeSet<String> = ["m.hit".to_string(), "miss".to_string()].into();
        let fix = env.alias_repair(&defined);
        assert_eq!(fix.get("m.miss"), Some(&"miss"));
        assert_eq!(
            fix.get("m.hit"),
            None,
            "the qualified name wins when defined"
        );
    }
}
