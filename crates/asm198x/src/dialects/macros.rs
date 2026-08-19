//! Macro expansion, shared across dialects that have macros (#93).
//!
//! Expansion is a source pre-pass: definitions are collected and removed,
//! invocations are replaced by their substituted bodies, and the result goes
//! through the dialect's ordinary parse.
//!
//! # What is shared, and what is not
//!
//! The *mechanics* are identical everywhere they were measured — substitution
//! is textual, respects word boundaries, does not reach inside string literals,
//! is case-sensitive on parameter names, and happens before expression
//! evaluation. Those live here.
//!
//! The *grammar* is not shared, and trying to would be the mistake. Every
//! reference spells this differently, and the differences are not cosmetic:
//!
//! | | sjasmplus | pasmo |
//! |---|---|---|
//! | header | `MACRO name p1 p2` | `MACRO name, p1, p2` **or** `name MACRO p1, p2` |
//! | per-expansion locals | any `.dotted` label | only those a `LOCAL` line declares |
//! | a `.dotted` label | scoped | **collides** — it is an ordinary label |
//!
//! So a dialect supplies its grammar through [`MacroSyntax`] and inherits the
//! rest. `v1-scope.md` scopes macros as *"adopted against real dialect
//! requirements rather than as a universal macro language"*, and this is the
//! seam that keeps that honest.

use crate::engine::AsmError;
use crate::span::{ExpansionFrame, Span};

/// How deep expansion may nest before we call it runaway.
///
/// Both references measured — sjasmplus 1.21.0 and pasmo 0.5.5 — **segfault**
/// on a self-recursive macro (exit 139). We decline to reproduce that: a crash
/// is not a verdict about anyone's source, and an assembler that dies is worse
/// than one that explains itself. Byte-identical output is the goal;
/// byte-identical crashing is not.
const MAX_EXPANSION_DEPTH: usize = 64;

/// One dialect's macro grammar.
pub(crate) trait MacroSyntax {
    /// Parse a definition header, returning the macro's name and parameters.
    /// `None` if the line does not open a macro.
    fn header(&self, line: &str) -> Option<(String, Vec<String>)>;

    /// Whether the line closes a definition.
    fn is_end(&self, line: &str) -> bool;

    /// What that closing keyword is called, so an unterminated definition is
    /// reported in the spelling the author is looking for.
    fn end_keyword(&self) -> &'static str;

    /// The names in `body` that are local to each expansion, and so must be
    /// renamed per invocation.
    fn locals(&self, body: &[String]) -> Vec<String>;

    /// Whether the line is a local *declaration* rather than code — pasmo's
    /// `LOCAL loop`. Such lines are dropped from the expansion; a dialect that
    /// infers locals from their spelling has none.
    fn is_local_decl(&self, line: &str) -> bool {
        let _ = line;
        false
    }

    /// Reconcile an invocation's arguments with the macro's parameters,
    /// returning the list to substitute or the message to reject it with.
    ///
    /// There is deliberately no default. The two references measured so far do
    /// opposite things — sjasmplus rejects any mismatch by name, pasmo drops
    /// extras and substitutes *empty* for a missing one, so the diagnostic
    /// arrives from whatever the empty operand broke. Either posture is
    /// defensible; guessing which one a new dialect takes would produce wrong
    /// bytes in silence, so every dialect must say.
    fn fit_arguments(
        &self,
        name: &str,
        params: &[String],
        args: Vec<String>,
    ) -> Result<Vec<String>, String>;
}

/// Where one line of rewritten source came from.
///
/// The line number alone is enough to point a diagnostic at something the
/// author wrote. The frames are what let it explain *why* — `in expansion of
/// macro \`m\`` — which for generated code is most of the answer, because the
/// failing text is often nowhere in the file.
#[derive(Clone, Debug)]
pub(crate) struct LineOrigin {
    /// The 1-based source line this output line reports as.
    pub(crate) line: usize,
    /// The expansions this line came through, innermost first — the same order
    /// the `included from` notes use.
    pub(crate) frames: Vec<ExpansionFrame>,
}

/// Expanded source, plus where each output line came from.
pub(crate) struct Expanded {
    pub(crate) text: String,
    pub(crate) origins: Vec<LineOrigin>,
}

/// A macro as collected by the pre-pass.
struct MacroDef {
    params: Vec<String>,
    body: Vec<String>,
}

/// Strip a trailing comment, respecting string literals so a `;` inside quotes
/// is not mistaken for one.
pub(crate) fn without_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == b'"' || c == b'\'' {
                    quote = Some(c);
                } else if c == b';' || (c == b'/' && bytes.get(i + 1) == Some(&b'/')) {
                    return &line[..i];
                }
            }
        }
        i += 1;
    }
    line
}

/// Replace whole-word occurrences of each name with its replacement, leaving
/// string literals untouched.
///
/// Both properties were measured, and a naive `replace` gets both wrong: a
/// parameter `v` would corrupt the symbol `val`, and `db "v"` would emit the
/// argument instead of the letter.
pub(crate) fn substitute(line: &str, names: &[String], values: &[String]) -> String {
    if names.is_empty() {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'.';
    let mut out = String::with_capacity(line.len());
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            out.push(c as char);
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = Some(c);
            out.push(c as char);
            i += 1;
            continue;
        }
        if word(c) && (i == 0 || !word(bytes[i - 1])) {
            let mut j = i;
            while j < bytes.len() && word(bytes[j]) {
                j += 1;
            }
            let token = &line[i..j];
            match names.iter().position(|n| n == token) {
                Some(k) => out.push_str(values.get(k).map_or("", String::as_str)),
                None => out.push_str(token),
            }
            i = j;
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

/// Split an argument list on commas outside strings.
pub(crate) fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match quote {
            Some(q) => {
                current.push(c);
                if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                current.push(c);
            }
            None if c == ',' => {
                args.push(current.trim().to_string());
                current = String::new();
            }
            None => current.push(c),
        }
    }
    let last = current.trim();
    if !last.is_empty() || !args.is_empty() {
        args.push(last.to_string());
    }
    args
}

/// Collect macro definitions and expand their invocations.
///
/// Two passes, because a macro may invoke one defined **later** in the file —
/// the references resolve a name when they expand, not when they read.
pub(crate) fn expand<S: MacroSyntax>(syntax: &S, source: &str) -> Result<Expanded, AsmError> {
    let lines: Vec<&str> = source.lines().collect();
    let mut macros: std::collections::HashMap<String, MacroDef> = std::collections::HashMap::new();

    // Pass 1 — take the definitions out, keep everything else with its origin.
    let mut body: Vec<(LineOrigin, String)> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let line_no = i + 1;
        if let Some((name, params)) = syntax.header(raw) {
            let mut collected = Vec::new();
            let mut j = i + 1;
            let mut closed = false;
            while j < lines.len() {
                if syntax.is_end(lines[j]) {
                    closed = true;
                    break;
                }
                collected.push(lines[j].to_string());
                j += 1;
            }
            if !closed {
                return Err(AsmError::new(
                    line_no,
                    format!("`macro {name}` has no matching `{}`", syntax.end_keyword()),
                ));
            }
            macros.insert(
                name,
                MacroDef {
                    params,
                    body: collected,
                },
            );
            i = j + 1;
            continue;
        }
        body.push((
            LineOrigin {
                line: line_no,
                frames: Vec::new(),
            },
            raw.to_string(),
        ));
        i += 1;
    }

    // Pass 2 — expand until nothing is left to expand, since a body may invoke
    // another macro.
    let mut expansions = 0usize;
    for depth in 0..=MAX_EXPANSION_DEPTH {
        let mut next: Vec<(LineOrigin, String)> = Vec::with_capacity(body.len());
        let mut expanded_any = false;
        for (origin, text) in &body {
            let Some((label, name, args)) = invocation(text, &macros) else {
                next.push((origin.clone(), text.clone()));
                continue;
            };
            let def = &macros[&name];
            let args = syntax
                .fit_arguments(&name, &def.params, args)
                .map_err(|msg| AsmError::new(origin.line, msg))?;
            if depth == MAX_EXPANSION_DEPTH {
                return Err(AsmError::new(
                    origin.line,
                    format!(
                        "macro `{name}` is still expanding after {MAX_EXPANSION_DEPTH} levels — is it recursive?"
                    ),
                ));
            }
            expanded_any = true;
            expansions += 1;
            let locals = syntax.locals(&def.body);
            let renamed: Vec<String> = locals
                .iter()
                .map(|local| format!("{local}__{expansions}"))
                .collect();
            // The new frame goes in front, so the innermost expansion is named
            // first — the order the `included from` notes already use.
            let mut frames = Vec::with_capacity(origin.frames.len() + 1);
            frames.push(ExpansionFrame {
                macro_name: name.clone(),
                invoked_at: Box::new(Span::at(origin.line as u32, 0)),
            });
            frames.extend(origin.frames.iter().cloned());
            // A label in front of the invocation is the author's own text, so
            // it keeps their origin and gains no frame. Emitting it on its own
            // line binds it to the expansion's first address, which is what the
            // references do.
            if let Some(label) = label {
                next.push((origin.clone(), label));
            }
            for body_line in &def.body {
                if syntax.is_local_decl(body_line) {
                    continue;
                }
                let with_args = substitute(body_line, &def.params, &args);
                next.push((
                    LineOrigin {
                        line: origin.line,
                        frames: frames.clone(),
                    },
                    substitute(&with_args, &locals, &renamed),
                ));
            }
        }
        body = next;
        if !expanded_any {
            break;
        }
    }

    let mut text = String::with_capacity(source.len());
    let mut origins = Vec::with_capacity(body.len());
    for (origin, line) in body {
        text.push_str(&line);
        text.push('\n');
        origins.push(origin);
    }
    Ok(Expanded { text, origins })
}

/// What a line invokes, if anything: the label in front of it, the macro's
/// name, and its arguments. Names match case-sensitively, as every reference
/// measured does.
///
/// The label matters. `lbl: m1 9` is accepted by every reference measured, and
/// binds `lbl` to the address the expansion starts at — the same rule a label
/// on an `include` line follows. Missing it does not mis-assemble; it rejects
/// the line outright as an unknown instruction, because the label is read as
/// the mnemonic.
fn invocation(
    line: &str,
    macros: &std::collections::HashMap<String, MacroDef>,
) -> Option<(Option<String>, String, Vec<String>)> {
    let stripped = without_comment(line);
    let trimmed = stripped.trim();
    let (head, tail) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));
    if macros.contains_key(head) {
        return Some((None, head.to_string(), split_args(tail)));
    }
    // Only a word at column 0 can be a label; anywhere else it is a mnemonic,
    // and every reference measured takes the colon as optional.
    if stripped.starts_with(char::is_whitespace) {
        return None;
    }
    let tail = tail.trim();
    let (name, args) = tail.split_once(char::is_whitespace).unwrap_or((tail, ""));
    macros
        .contains_key(name)
        .then(|| (Some(head.to_string()), name.to_string(), split_args(args)))
}
