//! The text layer: string symbols and string functions, resolved before the
//! parse.
//!
//! `decisions/string-and-text-layer.md` is the binding record. In short: an
//! expression evaluates to an `i64` and always will, so the references' string
//! features are a **source pre-pass** in the shape
//! [`macros`](super::macros) already uses — the mechanics live here once, and
//! each dialect supplies its grammar through [`TextSyntax`].
//!
//! The pass walks lines in source order, so a name is defined before it is
//! used, exactly as the references require. It emits **one output line per
//! input line** — a definition becomes a blank line rather than disappearing —
//! so every span still points at the line the author wrote, and no origin map
//! is needed.
//!
//! Everything folds to *text*. `STRCAT("ab","cd")` becomes `"abcd"` and
//! `STRLEN("abc")` becomes `3`; the ordinary parse then reads a string literal
//! and a number as it always would. That is what keeps the layer out of the
//! expression language.

use std::collections::BTreeMap;

use crate::engine::AsmError;

/// One argument to a string function, after the pass has folded it.
#[derive(Debug, Clone)]
pub(crate) enum Arg {
    /// A string literal's contents, without its quotes.
    Text(String),
    /// Anything else, verbatim — a number, a symbol, an expression.
    Bare(String),
}

impl Arg {
    /// The text of a string argument, or a refusal naming what was wanted.
    pub(crate) fn text(&self, name: &str, line: usize) -> Result<&str, AsmError> {
        match self {
            Arg::Text(t) => Ok(t),
            Arg::Bare(b) => Err(AsmError::new(
                line,
                format!("`{name}` takes a string here, and `{b}` is not one"),
            )),
        }
    }

    /// The argument's text, whichever kind it is — what a number is read from.
    fn raw(&self) -> &str {
        match self {
            Arg::Bare(b) => b.trim(),
            Arg::Text(t) => t,
        }
    }
}

/// What a folded call may consult: the numeric constants defined **above** it,
/// read through the dialect's own expression grammar.
///
/// The pass walks in source order and folds the environment as it goes, so a
/// constant is in scope exactly where the reference has it in scope.
pub(crate) struct Scope<'a> {
    numbers: &'a BTreeMap<String, i64>,
    evaluate: &'a Evaluate<'a>,
}

/// Reading a number in a dialect's own expression grammar, against the
/// constants above the line.
type Evaluate<'a> = dyn Fn(&str, &BTreeMap<String, i64>, usize) -> Option<i64> + 'a;

impl Scope<'_> {
    /// A whole-number argument, which the pass works out where it stands: an
    /// index, a length, or a value to format.
    ///
    /// # Errors
    ///
    /// Anything the pass cannot reduce to a number here — including the one
    /// case `decisions/string-and-text-layer.md` names, a label's address,
    /// which is not assigned until long after this pass has run.
    pub(crate) fn number(&self, arg: &Arg, name: &str, line: usize) -> Result<i64, AsmError> {
        let text = arg.raw();
        (self.evaluate)(text, self.numbers, line).ok_or_else(|| {
            AsmError::new(
                line,
                format!(
                    "`{name}` needs a number it can work out where it stands, and `{text}` is \
                     not one: this pass runs before the layout, so a label's address cannot be \
                     reached from here"
                ),
            )
        })
    }
}

/// A decimal, `$hex` or `%binary` literal — what an index argument may be.
fn parse_int(text: &str) -> Option<i64> {
    let (negative, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, text),
    };
    let value = if let Some(hex) = text.strip_prefix('$') {
        i64::from_str_radix(hex, 16).ok()?
    } else if let Some(bin) = text.strip_prefix('%') {
        i64::from_str_radix(bin, 2).ok()?
    } else {
        text.parse().ok()?
    };
    Some(if negative { -value } else { value })
}

/// What a folded function produced.
pub(crate) enum Folded {
    /// Text, which is spliced back in **quoted** so the parse reads a literal.
    Text(String),
    /// A number, spliced back in as digits.
    Number(i64),
}

/// A dialect's string grammar.
pub(crate) trait TextSyntax {
    /// Recognise a string-symbol definition, returning the name and the text
    /// of its value (the value is folded by the pass before it is stored).
    fn definition(&self, line: &str) -> Option<(String, String)>;

    /// Recognise a **numeric** constant definition and work its value out
    /// against the constants above it. The line itself is kept as it stands —
    /// unlike a string symbol, a constant is a statement the ordinary parse
    /// still reads — and its value joins the environment the lines below it
    /// fold against.
    fn constant(&self, line: &str, numbers: &BTreeMap<String, i64>) -> Option<(String, i64)> {
        let _ = (line, numbers);
        None
    }

    /// Read a number in this dialect's expression grammar, against the
    /// constants collected above the line. The default reads a literal, which
    /// is all a dialect with no constants environment needs.
    fn evaluate(&self, text: &str, numbers: &BTreeMap<String, i64>, line: usize) -> Option<i64> {
        let _ = (numbers, line);
        parse_int(text)
    }

    /// Fold one call. `None` means the name is not a string function here, and
    /// the call is left alone for the ordinary parse to deal with.
    fn function(
        &self,
        name: &str,
        args: &[Arg],
        scope: &Scope,
        line: usize,
    ) -> Result<Option<Folded>, AsmError>;

    /// Whether `{name}` splices a string symbol into the middle of a token.
    fn interpolates(&self) -> bool {
        false
    }
}

/// Run the pass over a whole source.
///
/// # Errors
///
/// Any refusal a folded function raises — a wrong argument type, an index the
/// pass cannot work out, or a function given the wrong number of arguments.
pub(crate) fn expand<S: TextSyntax>(syntax: &S, source: &str) -> Result<String, AsmError> {
    let mut symbols: BTreeMap<String, String> = BTreeMap::new();
    let mut numbers: BTreeMap<String, i64> = BTreeMap::new();
    let mut out = String::with_capacity(source.len());
    let evaluate = |t: &str, n: &BTreeMap<String, i64>, l: usize| syntax.evaluate(t, n, l);
    for (index, raw) in source.lines().enumerate() {
        let line = index + 1;
        if let Some((name, value)) = syntax.definition(raw) {
            // The value is folded now, against the symbols above it, and the
            // definition line is kept as a blank so every later span still
            // names the line its author wrote.
            let scope = Scope {
                numbers: &numbers,
                evaluate: &evaluate,
            };
            let value = fold_line(syntax, &scope, &substitute(&value, &symbols, syntax), line)?;
            symbols.insert(name, unquote(value.trim()));
            out.push('\n');
            continue;
        }
        let substituted = substitute(raw, &symbols, syntax);
        let folded = {
            let scope = Scope {
                numbers: &numbers,
                evaluate: &evaluate,
            };
            fold_line(syntax, &scope, &substituted, line)?
        };
        // A constant is read off the folded line, so `N EQU STRLEN("abc")`
        // joins the environment as the number it became.
        if let Some((name, value)) = syntax.constant(&folded, &numbers) {
            numbers.insert(name, value);
        }
        out.push_str(&folded);
        out.push('\n');
    }
    Ok(out)
}

/// A string literal's contents with its escapes resolved, or the text
/// unchanged if it is not a literal.
///
/// `\"` and `\\` are the two every reference measured spells the same way, and
/// they are the two that matter here: a stored string may hold a quote, and it
/// is written back out quoted.
fn unquote(text: &str) -> String {
    match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        Some(inner) => unescape(inner),
        None => text.to_string(),
    }
}

/// Resolve `\"` and `\\`; anything else keeps its backslash.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The inverse, for splicing folded text back into the source as a literal.
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Replace every string symbol in a line: a bare name at a word boundary, and
/// — where the dialect has it — a `{name}` interpolation, which reaches into
/// the middle of a token and into string literals.
fn substitute<S: TextSyntax>(line: &str, symbols: &BTreeMap<String, String>, syntax: &S) -> String {
    if symbols.is_empty() {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let word =
        |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'#' || c == b'@';
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // `{name}` is spliced even inside a string, which is what it is for.
        if syntax.interpolates()
            && c == b'{'
            && let Some(end) = line[i + 1..].find('}')
        {
            let name = &line[i + 1..i + 1 + end];
            if let Some(value) = symbols.get(name) {
                out.push_str(value);
                i += end + 2;
                continue;
            }
        }
        if let Some(q) = quote {
            out.push(c as char);
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            quote = Some(c);
            out.push(c as char);
            i += 1;
            continue;
        }
        if c == b';' {
            out.push_str(&line[i..]);
            break;
        }
        if word(c) && (i == 0 || !word(bytes[i - 1])) {
            let mut j = i;
            while j < bytes.len() && word(bytes[j]) {
                j += 1;
            }
            let token = &line[i..j];
            match symbols.get(token) {
                Some(value) => out.push_str(value),
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

/// Fold every string-function call in one line, innermost first.
fn fold_line<S: TextSyntax>(
    syntax: &S,
    scope: &Scope,
    line: &str,
    at: usize,
) -> Result<String, AsmError> {
    let mut current = line.to_string();
    // A fold can expose another (`STRLEN(STRCAT(...))`), so the line is walked
    // again until it stops changing. The bound is the line's own length, which
    // no sequence of folds can exceed in count.
    for _ in 0..=line.len() {
        match fold_once(syntax, scope, &current, at)? {
            Some(next) => current = next,
            None => return Ok(current),
        }
    }
    Err(AsmError::new(
        at,
        "a string function on this line never stops folding",
    ))
}

/// Fold the **innermost, leftmost** call, or `None` when there is none left.
fn fold_once<S: TextSyntax>(
    syntax: &S,
    scope: &Scope,
    line: &str,
    at: usize,
) -> Result<Option<String>, AsmError> {
    let bytes = line.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'.';
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                quote = Some(c);
                i += 1;
            }
            b';' => break,
            _ if word(c) && (i == 0 || !word(bytes[i - 1])) => {
                let mut j = i;
                while j < bytes.len() && word(bytes[j]) {
                    j += 1;
                }
                let name = &line[i..j];
                if bytes.get(j) == Some(&b'(')
                    && let Some(close) = matching_paren(line, j)
                {
                    let inner = &line[j + 1..close];
                    // Innermost first: a call inside the arguments is folded
                    // before this one, so every argument is already a literal.
                    if has_call(syntax, scope, inner) {
                        i = j + 1;
                        continue;
                    }
                    let args = split_args(inner);
                    if let Some(folded) = syntax.function(name, &args, scope, at)? {
                        let text = match folded {
                            Folded::Text(t) => format!("\"{}\"", escape(&t)),
                            Folded::Number(n) => n.to_string(),
                        };
                        return Ok(Some(format!("{}{text}{}", &line[..i], &line[close + 1..])));
                    }
                }
                i = j;
            }
            _ => i += 1,
        }
    }
    Ok(None)
}

/// Whether the text holds a call this dialect would fold — used to find the
/// innermost one rather than folding an outer call over unfolded arguments.
fn has_call<S: TextSyntax>(syntax: &S, scope: &Scope, text: &str) -> bool {
    let bytes = text.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'.';
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            quote = Some(c);
            i += 1;
            continue;
        }
        if word(c) && (i == 0 || !word(bytes[i - 1])) {
            let mut j = i;
            while j < bytes.len() && word(bytes[j]) {
                j += 1;
            }
            if bytes.get(j) == Some(&b'(')
                && syntax
                    .function(&text[i..j], &[], scope, 0)
                    .is_ok_and(|f| f.is_some())
            {
                return true;
            }
            // A name the dialect knows but which refused an empty argument
            // list is still a call.
            if bytes.get(j) == Some(&b'(') && syntax.function(&text[i..j], &[], scope, 0).is_err() {
                return true;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    false
}

/// The index of the `)` closing the `(` at `open`.
fn matching_paren(line: &str, open: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    for (i, &c) in bytes.iter().enumerate().skip(open) {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'"' => quote = Some(c),
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split an argument list on commas outside strings and nested parentheses.
fn split_args(text: &str) -> Vec<Arg> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut start = 0usize;
    let bytes = text.as_bytes();
    for (i, &c) in bytes.iter().enumerate() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'"' => quote = Some(c),
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                out.push(one_arg(&text[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(one_arg(&text[start..]));
    out
}

fn one_arg(text: &str) -> Arg {
    let trimmed = text.trim();
    match trimmed.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        Some(inner) => Arg::Text(unescape(inner)),
        _ => Arg::Bare(trimmed.to_string()),
    }
}
