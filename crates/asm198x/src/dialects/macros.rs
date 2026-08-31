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

use crate::ast::Node;
use crate::engine::AsmError;
use crate::span::FileId;
use crate::span::{ExpansionFrame, Span};

/// How deep expansion may nest before we call it runaway.
///
/// Both references measured — sjasmplus 1.21.0 and PasmoNext v0.1.3 — **segfault**
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
    ///
    /// Only the default [`collect`](Self::collect) consults this, so a dialect
    /// whose bodies are not line-terminated overrides that instead and leaves
    /// this alone.
    fn is_end(&self, line: &str) -> bool {
        let _ = line;
        false
    }

    /// What that closing keyword is called, so an unterminated definition is
    /// reported in the spelling the author is looking for. As
    /// [`is_end`](Self::is_end), the default collector's business only.
    fn end_keyword(&self) -> &'static str {
        ""
    }

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

    /// How an invocation's arguments are spelled inside the body.
    ///
    /// A dialect that names its parameters in the header uses that list, which
    /// is the default. lwasm and vasm name nothing: a body refers to `\1`,
    /// `\2`, so the spellings depend on how many arguments arrived rather than
    /// on the definition, and a macro's arity is whatever its call site says.
    fn argument_names(&self, declared: &[String], count: usize) -> Vec<String> {
        let _ = count;
        declared.to_vec()
    }

    /// A token the body may use to make a name unique per expansion, replaced
    /// by the expansion's number wherever it appears.
    ///
    /// vasm's `\@` — `spin\@` gives a distinct label each time the macro is
    /// used. It is a substitution, not a scoping rule, which is why it is a
    /// token here and not part of [`locals`](Self::locals): it can appear in
    /// the middle of a name, and in more than one name per body.
    fn expansion_token(&self) -> Option<&'static str> {
        None
    }

    /// A word the body may use for **how many arguments this invocation was
    /// given** — ca65's `.paramcount`, which counts the call site and not the
    /// declared parameters, so a macro with two parameters called with one
    /// answers 1.
    ///
    /// A plain textual swap, like [`expansion_token`](Self::expansion_token),
    /// and for the same reason: the word is not a symbol.
    fn argument_count_word(&self) -> Option<&'static str> {
        None
    }

    /// A call the dialect answers with **whether a macro of that name is
    /// defined** — ca65's `.definedmacro(name)`.
    ///
    /// Answered against the macros defined above the line that asks, which for
    /// a line inside a body is the line the macro was *invoked* on: that is
    /// where ca65 evaluates it, and it is the origin every expanded line
    /// already carries.
    fn defined_macro_word(&self) -> Option<&'static str> {
        None
    }

    /// Whether `c` can appear inside a symbol, for word-boundary-aware
    /// substitution.
    ///
    /// The default is the set every dialect shares. lwasm adds `?` and `@`,
    /// which it allows as *suffixes* marking a symbol local to the expansion —
    /// so without this, renaming `spin?` would rename the `spin` in front of a
    /// character the substitution could not see.
    fn is_symbol_char(&self, c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_' || c == b'.'
    }

    /// What a local is called in expansion number `n`.
    ///
    /// The default appends a suffix, which is invisible to every parser here.
    /// lwasm needs to override it: its locals are marked by a trailing `?` or
    /// `@` that lwasm's own parser strips and ours does not, so the marker has
    /// to go rather than end up buried mid-name.
    fn rename_local(&self, name: &str, n: usize) -> String {
        format!("{name}__{n}")
    }

    /// Lift the definition starting at `lines[start]`, if one starts there.
    ///
    /// `None` means the line opens no definition. `Some(Err(..))` means it does
    /// and the definition is malformed, with the message to report.
    ///
    /// The default collects a **line-terminated** body — the header line, then
    /// every line up to the one [`is_end`](Self::is_end) recognises — which is
    /// what four of the five keyword dialects need. acme overrides it because
    /// its bodies are brace-delimited at character level: braces nest inside a
    /// body, and both braces share lines with code.
    fn collect(&self, lines: &[&str], start: usize) -> Option<Result<Definition, String>> {
        let (name, params) = self.header(lines[start])?;
        let mut body = Vec::new();
        for (offset, line) in lines.iter().enumerate().skip(start + 1) {
            if self.is_end(line) {
                return Some(Ok(Definition {
                    name,
                    def: MacroDef {
                        params,
                        body,
                        defined_at: None,
                    },
                    last_line: offset,
                }));
            }
            body.push((*line).to_string());
        }
        Some(Err(format!(
            "`macro {name}` has no matching `{}`",
            self.end_keyword()
        )))
    }

    /// Which definition of `name` an invocation passing `argc` arguments means.
    ///
    /// Every dialect but acme has one definition per name and takes it whatever
    /// the count, leaving [`fit_arguments`](Self::fit_arguments) to reconcile —
    /// so the default takes the last one defined, as an overwriting table did.
    /// acme dispatches on the count itself: `ldav .v` and `ldav .v, .w` are two
    /// macros, and a call matching neither is `Macro not defined`, not a bad
    /// argument list.
    fn select<'a>(&self, defs: &'a [MacroDef], argc: usize) -> Option<&'a MacroDef> {
        let _ = argc;
        defs.last()
    }

    /// The macro an invocation's first word names, if it names one.
    ///
    /// acme spells a call `+ldav`, and a bare `ldav` is not one; everyone else
    /// writes the name alone, which is the default.
    fn invocation_name<'a>(&self, head: &'a str) -> Option<&'a str> {
        Some(head)
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
#[derive(Clone)]
pub(crate) struct MacroDef {
    /// Parameter names, in order. Empty for a dialect whose parameters are
    /// positional, where [`MacroSyntax::argument_names`] supplies them instead.
    pub(crate) params: Vec<String>,
    /// Body lines, verbatim, before substitution.
    pub(crate) body: Vec<String>,
    /// The definition header, when expansion is shared across source files.
    pub(crate) defined_at: Option<Span>,
}

/// A live macro namespace. Most dialects expand one source string and use the
/// short-lived default; ACME threads one namespace through its live include
/// walk because `!source` is textual for macro visibility.
#[derive(Default)]
pub(crate) struct MacroState {
    macros: std::collections::HashMap<String, Vec<MacroDef>>,
    expansions: usize,
}

/// A definition the pre-pass lifted out of the source.
pub(crate) struct Definition {
    pub(crate) name: String,
    pub(crate) def: MacroDef,
    /// Index of the last line the definition occupies, so the pre-pass knows
    /// where the file resumes. A brace-delimited dialect can end a definition
    /// part-way along a line; whatever follows the closing brace is dropped,
    /// which is what every reference measured does.
    pub(crate) last_line: usize,
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
pub(crate) fn substitute<S: MacroSyntax>(
    syntax: &S,
    line: &str,
    names: &[String],
    values: &[String],
) -> String {
    if names.is_empty() {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let word = |c: u8| syntax.is_symbol_char(c);
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
/// Answer every `word(name)` on one line with 1 or 0, against the macros
/// defined above source line `at`.
///
/// A definition *below* the line does not count — ca65 answers 0 there — which
/// is why this reads the source line the expansion recorded rather than the
/// position in the output.
fn fold_defined_macro(
    word: &str,
    line: &str,
    defined_at: &std::collections::HashMap<String, usize>,
    at: usize,
) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find(word) {
        let after = &rest[start + word.len()..];
        let Some(open) = after.find('(').filter(|i| after[..*i].trim().is_empty()) else {
            out.push_str(&rest[..start + word.len()]);
            rest = after;
            continue;
        };
        let Some(close) = after[open..].find(')').map(|i| open + i) else {
            break;
        };
        let name = after[open + 1..close].trim();
        let known = defined_at.get(name).is_some_and(|line| *line < at);
        out.push_str(&rest[..start]);
        out.push(if known { '1' } else { '0' });
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn expand<S: MacroSyntax>(syntax: &S, source: &str) -> Result<Expanded, AsmError> {
    expand_in(syntax, source, FileId(0), &mut MacroState::default())
}

/// Expand one file against a namespace shared with earlier files in an include
/// walk. Definitions collected here remain visible to subsequent calls.
pub(crate) fn expand_in<S: MacroSyntax>(
    syntax: &S,
    source: &str,
    file: FileId,
    state: &mut MacroState,
) -> Result<Expanded, AsmError> {
    expand_at(syntax, source, file, 1, state)
}

/// As [`expand_in`], with the source fragment's first line in its real file.
/// ACME uses this when a live include walk reaches a definition or invocation
/// node that was deliberately left unexpanded by the file parser.
pub(crate) fn expand_at<S: MacroSyntax>(
    syntax: &S,
    source: &str,
    file: FileId,
    first_line: usize,
    state: &mut MacroState,
) -> Result<Expanded, AsmError> {
    expand_at_inner(syntax, source, file, first_line, state, true)
}

/// Expand one invocation layer while retaining nested calls in the result.
/// A live evaluator uses this so a call inside a repetition is expanded anew
/// on every iteration, receiving a fresh per-invocation local scope.
pub(crate) fn expand_one_at<S: MacroSyntax>(
    syntax: &S,
    source: &str,
    file: FileId,
    first_line: usize,
    state: &mut MacroState,
) -> Result<Expanded, AsmError> {
    expand_at_inner(syntax, source, file, first_line, state, false)
}

fn expand_at_inner<S: MacroSyntax>(
    syntax: &S,
    source: &str,
    file: FileId,
    first_line: usize,
    state: &mut MacroState,
    recursive: bool,
) -> Result<Expanded, AsmError> {
    let lines: Vec<&str> = source.lines().collect();
    // Definitions are grouped by name rather than replaced, because one dialect
    // — acme — lets a name carry several, told apart by how many arguments they
    // take. Where a dialect has only ever one, the group holds one.
    // The source line each name became defined on, for `.definedmacro`: a
    // definition below the line that asks does not count.
    let mut defined_at: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    // Pass 1 — take the definitions out, keep everything else with its origin.
    let mut body: Vec<(LineOrigin, String)> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let line_no = first_line + i;
        if let Some(collected) = syntax.collect(&lines, i) {
            let Definition {
                name,
                def,
                last_line,
            } = collected.map_err(|msg| AsmError::new(line_no, msg))?;
            defined_at.entry(name.clone()).or_insert(last_line + 1);
            let mut def = def;
            def.defined_at = Some(Span::in_file(file, line_no as u32, 1));
            state.macros.entry(name).or_default().push(def);
            i = last_line + 1;
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
    for depth in 0..=MAX_EXPANSION_DEPTH {
        let mut next: Vec<(LineOrigin, String)> = Vec::with_capacity(body.len());
        let mut expanded_any = false;
        for (origin, text) in &body {
            let Some((label, name, args)) =
                invocation(syntax, text, &|w: &str| state.macros.contains_key(w))
            else {
                next.push((origin.clone(), text.clone()));
                continue;
            };
            let Some(def) = syntax.select(&state.macros[&name], args.len()) else {
                // Only reachable where arity picks the definition: the name is
                // known, but no definition takes this many arguments.
                return Err(AsmError::new(
                    origin.line,
                    format!(
                        "no definition of macro `{name}` takes {} argument(s)",
                        args.len()
                    ),
                ));
            };
            let given = args.len();
            let args = syntax
                .fit_arguments(&name, &def.params, args)
                .map_err(|msg| AsmError::new(origin.line, msg))?;
            let params = syntax.argument_names(&def.params, args.len());
            if depth == MAX_EXPANSION_DEPTH {
                return Err(AsmError::new(
                    origin.line,
                    format!(
                        "macro `{name}` is still expanding after {MAX_EXPANSION_DEPTH} levels — is it recursive?"
                    ),
                ));
            }
            expanded_any = true;
            state.expansions += 1;
            let locals = syntax.locals(&def.body);
            let renamed: Vec<String> = locals
                .iter()
                .map(|local| syntax.rename_local(local, state.expansions))
                .collect();
            // The new frame goes in front, so the innermost expansion is named
            // first — the order the `included from` notes already use.
            let mut frames = Vec::with_capacity(origin.frames.len() + 1);
            frames.push(ExpansionFrame {
                macro_name: name.clone(),
                defined_at: def.defined_at.clone().map(Box::new),
                invoked_at: Box::new(Span::in_file(file, origin.line as u32, 0)),
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
                let mut text = substitute(syntax, body_line, &params, &args);
                if let Some(word) = syntax.argument_count_word() {
                    // The call site's count, not the fitted one: a macro with
                    // two parameters called with one answers 1.
                    text = text.replace(word, &given.to_string());
                }
                if let Some(token) = syntax.expansion_token() {
                    // A plain textual swap: the token is not a symbol, so the
                    // word-boundary rules above would never see it.
                    text = text.replace(token, &state.expansions.to_string());
                }
                next.push((
                    LineOrigin {
                        line: origin.line,
                        frames: frames.clone(),
                    },
                    substitute(syntax, &text, &locals, &renamed),
                ));
            }
        }
        body = next;
        if !expanded_any || !recursive {
            break;
        }
    }

    let mut text = String::with_capacity(source.len());
    let mut origins = Vec::with_capacity(body.len());
    for (origin, line) in body {
        let line = match syntax.defined_macro_word() {
            Some(word) => fold_defined_macro(word, &line, &defined_at, origin.line),
            None => line,
        };
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
fn invocation<S: MacroSyntax>(
    syntax: &S,
    line: &str,
    known: &dyn Fn(&str) -> bool,
) -> Option<(Option<String>, String, Vec<String>)> {
    let named = |word: &str| {
        syntax
            .invocation_name(word)
            .filter(|name| known(name))
            .map(str::to_string)
    };
    let stripped = without_comment(line);
    let trimmed = stripped.trim();
    let (head, tail) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));
    if let Some(name) = named(head) {
        return Some((None, name, split_args(tail)));
    }
    // Only a word at column 0 can be a label; anywhere else it is a mnemonic,
    // and every reference measured takes the colon as optional.
    if stripped.starts_with(char::is_whitespace) {
        return None;
    }
    let tail = tail.trim();
    let (word, args) = tail.split_once(char::is_whitespace).unwrap_or((tail, ""));
    named(word).map(|name| (Some(head.to_string()), name, split_args(args)))
}

/// What a line does to a macro definition, as far as a **walk** is concerned.
///
/// A formatter's walk needs to know only enough to copy a definition through
/// untouched — never to read one. It asks the question here so that it and the
/// expander cannot drift: the same grammar decides both, and a dialect that
/// grew a second spelling grows it once.
pub(crate) enum MacroLine {
    /// Opens a definition, and names it.
    Opens(String),
    /// Closes the definition that is open.
    Closes,
    /// Invokes a macro already defined.
    Invokes,
    /// Does none of those.
    None,
}

/// Which of those `line` is, given the names defined so far.
pub(crate) fn macro_line<S: MacroSyntax>(
    syntax: &S,
    line: &str,
    known: &dyn Fn(&str) -> bool,
) -> MacroLine {
    if let Some((name, _)) = syntax.header(line) {
        MacroLine::Opens(name)
    } else if syntax.is_end(line) {
        MacroLine::Closes
    } else if invocation(syntax, line, known).is_some() {
        MacroLine::Invokes
    } else {
        MacroLine::None
    }
}

// ---------------------------------------------------------------------------
// Wiring the expander into a parse
//
// Expansion is a source pre-pass, so a parse that runs it is reading text the
// author never wrote: line numbers shift, and whole lines exist that are in no
// file. Everything below puts that back — and decides, explicitly, whether the
// pre-pass runs at all.
//
// It lives here rather than in one dialect family because both need it, and
// because the mistake it prevents is not obvious enough to leave to each
// adopter. See `decisions/macro-expansion-framework.md`.
// ---------------------------------------------------------------------------

/// Whether a parse expands the dialect's macros.
///
/// Assembly expands, because that is what a macro is for. The **formatter must
/// not**: `asm198x fmt` lays source out, and a formatter that replaced a
/// definition with its expansions and deleted the definition would be rewriting
/// the program instead — silently, over the author's file. So the two paths ask
/// for different parses of the same text, and this is where they part.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expand {
    /// Expand macros: the assembly paths.
    Yes,
    /// Leave definitions and invocations as written: the formatter's parse.
    No,
}

/// A dialect's source rewrite, held for as long as the parse borrows it.
///
/// `None` means the dialect rewrites nothing and the source is parsed as
/// written — the case every dialect but the macro-capable ones is in.
pub(crate) type Expansion = Option<(String, Vec<LineOrigin>)>;

/// Run the dialect's rewrite, unless this parse is the formatter's.
///
/// Taking the rewrite as a closure keeps this free of any one dialect family's
/// syntax trait, so the z80 and flat walks share it rather than each growing
/// their own copy of the `Expand::No` check — the check being the whole point.
pub(crate) fn expansion<F>(mode: Expand, source: &str, expand: F) -> Result<Expansion, AsmError>
where
    F: FnOnce(&str) -> Result<Expansion, AsmError>,
{
    match mode {
        Expand::Yes => expand(source),
        Expand::No => Ok(None),
    }
}

/// The text to parse: the rewrite if there was one, else the source itself.
pub(crate) fn expanded_text<'a>(expansion: &'a Expansion, source: &'a str) -> &'a str {
    expansion.as_ref().map_or(source, |(text, _)| text.as_str())
}

/// Where each rewritten line came from, if the source was rewritten.
pub(crate) fn line_origins(expansion: &Expansion) -> Option<&[LineOrigin]> {
    expansion.as_ref().map(|(_, origins)| origins.as_slice())
}

/// Put every span in `nodes` back on the line the author wrote.
pub(crate) fn place_nodes(nodes: &mut [Node], origins: Option<&[LineOrigin]>) {
    let Some(origins) = origins else { return };
    for node in nodes {
        place(&mut node.span, origins);
        if let Some(span) = node.operand_span.as_mut() {
            place(span, origins);
        }
    }
}

/// Put a span back where the author would look: the line they wrote, and the
/// expansions the text came through.
pub(crate) fn place(span: &mut Span, origins: &[LineOrigin]) {
    let Some(origin) = origins.get((span.line as usize).saturating_sub(1)) else {
        return;
    };
    span.line = origin.line as u32;
    span.expansion_frames.clone_from(&origin.frames);
}

/// Rewrite an error raised against rewritten source, so it names a real line
/// and carries the expansions it came through.
pub(crate) fn remap_lines(mut e: AsmError, origins: Option<&[LineOrigin]>) -> AsmError {
    let Some(origins) = origins else { return e };
    let origin = origins.get(e.line.saturating_sub(1)).cloned();
    if let Some(origin) = &origin {
        e.line = origin.line;
    }
    match e.span.as_mut() {
        Some(span) => place(span, origins),
        // An error raised while *parsing* expanded text carries no span, so
        // fixing its line was all this used to do — and the frames went with
        // it. A dialect whose walk reads a macro body eagerly raises every one
        // of its errors here, which is why four dialects could expand a macro
        // and still not say a diagnostic came from one.
        //
        // The span is minted line-granular (`col: 0`) because the parse knew no
        // column either; it exists to carry the frames.
        None => {
            if let Some(origin) = origin
                && !origin.frames.is_empty()
            {
                let mut span = Span::at(origin.line as u32, 0);
                span.expansion_frames.clone_from(&origin.frames);
                e.span = Some(span);
            }
        }
    }
    e
}

#[cfg(test)]
mod frame_tests {
    /// A diagnostic raised while **parsing** expanded text carries its frames.
    ///
    /// This is the half that was missing. A dialect whose walk reads a macro
    /// body eagerly raises every one of its errors at parse time, with no span
    /// — so remapping fixed the line and dropped the frames, and four dialects
    /// could expand a macro and still not say a diagnostic came from one.
    #[test]
    fn a_parse_error_in_an_expansion_keeps_its_frames() {
        let err = crate::assemble_lwasm("bad macro\n frobnicate\n endm\n bad\n")
            .expect_err("frobnicate is not an instruction");
        let span = err
            .span
            .as_ref()
            .expect("a span was minted to carry the frames");
        assert_eq!(span.col, 0, "line-granular: the parse knew no column");
        assert_eq!(span.expansion_frames.len(), 1, "{span:?}");
        assert_eq!(span.expansion_frames[0].macro_name, "bad");
        // The line the author wrote, not the line inside the expansion.
        assert_eq!(err.line, 4);
    }

    /// Every dialect that expands a macro now says so — the two paths a
    /// diagnostic can take out of an expansion both carry the frames.
    #[test]
    fn every_expanding_dialect_reports_its_expansion() {
        /// One dialect's assemble entry point.
        type Assemble = fn(&str) -> Result<crate::AssemblyResult, crate::AsmError>;

        let cases: &[(&str, Assemble, &str)] = &[
            (
                "sjasmplus",
                crate::assemble_sjasmplus,
                " MACRO bad\n frobnicate\n ENDM\n bad\n",
            ),
            (
                "pasmo",
                crate::assemble_pasmo,
                " MACRO bad\n frobnicate\n ENDM\n bad\n",
            ),
            (
                "lwasm",
                crate::assemble_lwasm,
                "bad macro\n frobnicate\n endm\n bad\n",
            ),
            (
                "ca65-816",
                crate::assemble_ca65_816,
                ".macro bad\n frobnicate\n.endmacro\n bad\n",
            ),
        ];
        for (name, assemble, src) in cases {
            let err = assemble(src).expect_err(name);
            let frames = err
                .span
                .as_ref()
                .map(|s| s.expansion_frames.as_slice())
                .unwrap_or_default();
            assert_eq!(frames.len(), 1, "{name}: {err:?}");
            assert_eq!(frames[0].macro_name, "bad", "{name}");
        }

        // vasm returns bare bytes rather than an `AssemblyResult`, and reaches
        // this from the other direction: its errors come from the multi-pass
        // layout, so its native statement carries the frames rather than a node
        // attaching them as it lowers.
        let err = crate::assemble_vasm("bad macro\n frobnicate\n endm\n bad\n").expect_err("vasm");
        let frames = err
            .span
            .as_ref()
            .map(|s| s.expansion_frames.as_slice())
            .unwrap_or_default();
        assert_eq!(frames.len(), 1, "vasm: {err:?}");
        assert_eq!(frames[0].macro_name, "bad");
    }

    /// An error outside any expansion gains nothing: this fills in only what a
    /// macro actually explains.
    #[test]
    fn an_error_outside_an_expansion_gains_nothing() {
        let err = crate::assemble_lwasm(" frobnicate\n").expect_err("not an instruction");
        assert!(
            err.span
                .as_ref()
                .is_none_or(|s| s.expansion_frames.is_empty()),
            "no expansion, no frames: {err:?}"
        );
    }
}
