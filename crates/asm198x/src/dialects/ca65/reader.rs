//! Owned ca65 source tokens and ordered semantic consumption.
//!
//! Classification retains authored spelling and opaque string handles without
//! evaluating names or skipped directives. The semantic input expands token
//! constructors at consumption, keeps numeric calls inert during capture, and
//! borrows the shared numeric grammar for values and constructor arguments.

use std::rc::Rc;

use super::string_tokens::StringTokens;
use crate::dialects::ca65_flat::Target;

/// What a token in a ca65 token list *is*, which is what `.match` compares.
///
/// Probed against ca65 V2.18, and the surprises are all here: `a`, `x` and `y`
/// are register tokens and nothing else is (`s` is an ordinary identifier), a
/// character literal is not a number, and each dot-keyword and each punctuation
/// mark is its own kind — `.match({.byte},{.word})` is 0 where
/// `.match({abc},{abd})` is 1.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum TokenKind {
    Number,
    Char,
    Str,
    Ident,
    /// `a`, `x` or `y`, lower-cased.
    Register(char),
    /// `.byte`, `.word`, … lower-cased, since each is its own kind.
    DotKeyword(String),
    /// `+`, `<<`, `::`, … verbatim, since each is its own kind.
    Punct(String),
}

/// One token, with the slice of source it came from so a sub-list can be
/// spliced back as the author wrote it rather than re-rendered.
#[derive(Clone)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    source: Rc<str>,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl Token {
    pub(crate) fn text(&self) -> &str {
        &self.source[self.start..self.end]
    }
}

/// ca65's two-character operators, longest-match-first.
const OPERATORS: &[&str] = &["::", "<<", ">>", "<=", ">=", "<>", "&&", "||"];

/// Split a token list into tokens. Whitespace separates and is not a token, so
/// `{+ +}` and `{++}` are the same two tokens.
pub(crate) fn tokens(text: &str, target: Target) -> Vec<Token> {
    let source: Rc<str> = text.into();
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let kind = match c {
            1 if StringTokens::is_handle(&text[i..]) => {
                i += StringTokens::handle_len(&text[i..]).unwrap_or(text.len() - i);
                TokenKind::Str
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                TokenKind::Str
            }
            b'\'' => {
                // The value may itself be an apostrophe: `'''` is one
                // character token, not an empty token followed by a quote.
                if bytes.get(i + 1).is_some_and(u8::is_ascii) && bytes.get(i + 2) == Some(&b'\'') {
                    i += 3;
                } else {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'\'' {
                        i += 1;
                    }
                    i = (i + 1).min(bytes.len());
                }
                TokenKind::Char
            }
            // `$ff` and `%1010` are numbers; a lone `$` is the location counter
            // and a lone `%` is the modulo operator.
            b'$' | b'%' if bytes.get(i + 1).is_some_and(|d| d.is_ascii_alphanumeric()) => {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                TokenKind::Number
            }
            _ if c.is_ascii_digit() => {
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                TokenKind::Number
            }
            // A dot-keyword, and not an identifier that happens to start with a
            // dot: ca65 has no such identifier.
            b'.' if bytes.get(i + 1).is_some_and(|d| d.is_ascii_alphabetic()) => {
                i += 1;
                while i < bytes.len() && word(bytes[i]) {
                    i += 1;
                }
                TokenKind::DotKeyword(text[start..i].to_ascii_lowercase())
            }
            _ if c.is_ascii_alphabetic() || c == b'_' || c == b'@' => {
                i += 1;
                while i < bytes.len() && word(bytes[i]) {
                    i += 1;
                }
                let name = text[start..i].to_ascii_lowercase();
                if target.holds_register(&name) {
                    TokenKind::Register(name.chars().next().unwrap_or('a'))
                } else {
                    TokenKind::Ident
                }
            }
            // `:+` and `:---` are one anonymous-label reference, not a colon
            // followed by operators.
            b':' if matches!(bytes.get(i + 1), Some(b'+' | b'-')) => {
                let sign = bytes[i + 1];
                i += 1;
                while bytes.get(i) == Some(&sign) {
                    i += 1;
                }
                TokenKind::Punct(text[start..i].to_string())
            }
            _ => {
                let rest = &text[i..];
                let op = OPERATORS.iter().find(|o| rest.starts_with(**o));
                i += op.map_or_else(
                    || text[i..].chars().next().map_or(1, char::len_utf8),
                    |o| o.len(),
                );
                TokenKind::Punct(text[start..i].to_string())
            }
        };
        out.push(Token {
            kind,
            source: Rc::clone(&source),
            start,
            end: i,
        });
    }
    out
}

use crate::dialects::{ca65_flat, mos6502, text};
use crate::engine::AsmError;
use crate::engine::Expr;

/// Generated frames suspend their caller. Tokens are moved between frames,
/// never rendered and classified again: a generated `A` remains an identifier.
struct Input<'a> {
    frames: Vec<std::vec::IntoIter<Token>>,
    current: Option<Token>,
    live: &'a ca65_flat::LiveText<'a>,
    line: usize,
    depth: usize,
}

impl<'a> Input<'a> {
    fn new(source: &str, live: &'a ca65_flat::LiveText<'a>, line: usize) -> Self {
        Self {
            frames: vec![tokens(source, live.token_target.unwrap_or(live.target)).into_iter()],
            current: None,
            live,
            line,
            depth: 0,
        }
    }

    fn raw(&mut self) -> Option<Token> {
        while let Some(frame) = self.frames.last_mut() {
            if let Some(token) = frame.next() {
                return Some(token);
            }
            self.frames.pop();
        }
        None
    }

    fn peek(&mut self) -> Result<Option<&Token>, AsmError> {
        loop {
            if self.current.is_none() {
                self.current = self.raw();
            }
            let constructor = self.current.as_ref().and_then(|token| match &token.kind {
                TokenKind::DotKeyword(name)
                    if matches!(
                        name.as_str(),
                        ".left" | ".right" | ".mid" | ".ident" | ".concat" | ".sprintf" | ".string"
                    ) =>
                {
                    Some(name.clone())
                }
                _ => None,
            });
            let Some(name) = constructor else {
                return Ok(self.current.as_ref());
            };
            self.current = None;
            if self.depth >= 100 {
                return Err(AsmError::new(
                    self.line,
                    "source constructors nested too deeply",
                ));
            }
            self.depth += 1;
            let generated = self.constructor(&name);
            self.depth -= 1;
            self.frames.push(generated?.into_iter());
        }
    }

    fn next(&mut self) -> Result<Option<Token>, AsmError> {
        self.peek()?;
        Ok(self.current.take())
    }

    fn at(&mut self, punctuation: &str) -> Result<bool, AsmError> {
        Ok(self
            .peek()?
            .is_some_and(|token| matches!(&token.kind, TokenKind::Punct(p) if p == punctuation)))
    }

    fn require(&mut self, punctuation: &str) -> Result<(), AsmError> {
        if !self.at(punctuation)? {
            return Err(AsmError::new(
                self.line,
                format!("expected `{punctuation}`"),
            ));
        }
        self.current = None;
        Ok(())
    }

    /// Native GetTokListTerm: only the first brace chooses the delimiter.
    /// Braces inside the list are ordinary tokens, including generated ones.
    fn capture(&mut self, unbraced_end: &str) -> Result<Vec<Token>, AsmError> {
        let braced = self.at("{")?;
        if braced {
            self.current = None;
        }
        let end = if braced { "}" } else { unbraced_end };
        let mut list = Vec::new();
        while !self.at(end)? {
            let token = self.next()?.ok_or_else(|| {
                AsmError::new(self.line, format!("expected `{end}` in token list"))
            })?;
            list.push(token);
        }
        if braced {
            self.current = None;
        }
        Ok(list)
    }

    fn number(&mut self) -> Result<i64, AsmError> {
        self.read_number(ca65_flat::NumericRead::Constant)
    }

    fn read_number(&mut self, read: ca65_flat::NumericRead) -> Result<i64, AsmError> {
        let context = self.live.context;
        let line = self.line;
        context.number_tokens(
            &mut Numeric {
                input: self,
                current: None,
                after_value: false,
            },
            read,
            line,
        )
    }

    fn query(&mut self, name: &str) -> Result<i64, AsmError> {
        let left = self.capture(if matches!(name, ".match" | ".xmatch") {
            ","
        } else {
            ")"
        })?;
        match name {
            ".tcount" => Ok(left.len() as i64),
            ".blank" => Ok(i64::from(left.is_empty())),
            ".match" | ".xmatch" => {
                self.require(",")?;
                let right = self.capture(")")?;
                let mut same = left.len() == right.len();
                for (a, b) in left.iter().zip(&right) {
                    if a.kind != b.kind
                        || (name == ".xmatch"
                            && token_value(a, self.live, self.line)?
                                != token_value(b, self.live, self.line)?)
                    {
                        same = false;
                        break;
                    }
                }
                Ok(i64::from(same))
            }
            _ => Err(AsmError::new(self.line, "unknown captured token query")),
        }
    }

    fn string_argument(&mut self) -> Result<text::Arg, AsmError> {
        let token = self
            .next()?
            .filter(|t| t.kind == TokenKind::Str)
            .ok_or_else(|| AsmError::new(self.line, "string constant expected"))?;
        Ok(if StringTokens::is_handle(token.text()) {
            text::Arg::Bare(token.text().to_string())
        } else {
            text::Arg::Text {
                cooked: String::new(),
                spelling: token.text().to_string(),
            }
        })
    }

    fn generated(&self, source: String, kind: TokenKind) -> Token {
        let end = source.len();
        Token {
            source: source.into(),
            kind,
            start: 0,
            end,
        }
    }

    fn constructor(&mut self, name: &str) -> Result<Vec<Token>, AsmError> {
        self.require("(")?;
        if matches!(name, ".left" | ".right" | ".mid") {
            let first = self.number()?.max(0) as usize;
            self.require(",")?;
            let (start, count) = if name == ".mid" {
                let count = self.number()?.max(0) as usize;
                self.require(",")?;
                (first, count)
            } else {
                (0, first)
            };
            let list = self.capture(")")?;
            self.require(")")?;
            let start = if name == ".right" {
                list.len().saturating_sub(count)
            } else {
                start
            };
            // Native saves the caller's current token before pushing the
            // selected list. Obtaining it can run an adjacent constructor.
            let following = self.next()?;
            return Ok(list
                .into_iter()
                .skip(start)
                .take(count)
                .chain(following)
                .collect());
        }
        if name == ".string" {
            let named = self
                .peek()?
                .is_some_and(|t| t.kind == TokenKind::Ident || t.text() == "::");
            let value = if named {
                let mut spelling = String::new();
                loop {
                    let token = self
                        .next()?
                        .ok_or_else(|| AsmError::new(self.line, "identifier expected"))?;
                    let namespace = token.text() == "::";
                    if (!namespace && token.kind != TokenKind::Ident)
                        || (namespace && !spelling.is_empty() && spelling.ends_with("::"))
                    {
                        return Err(AsmError::new(self.line, "identifier expected"));
                    }
                    spelling.push_str(token.text());
                    if namespace {
                        continue;
                    }
                    if !self.at("::")? {
                        break;
                    }
                }
                spelling.into_bytes()
            } else {
                if self
                    .peek()?
                    .is_some_and(|t| matches!(t.kind, TokenKind::Register(_) | TokenKind::Str))
                {
                    return Err(AsmError::new(self.line, "numeric expression expected"));
                }
                self.number()?.to_string().into_bytes()
            };
            self.require(")")?;
            let handle = self
                .live
                .strings
                .intern(value, ".string(...)".into(), self.line)?;
            return Ok(vec![self.generated(handle, TokenKind::Str)]);
        }
        let mut args = vec![self.string_argument()?];
        while self.at(",")? {
            self.current = None;
            args.push(
                if name == ".sprintf" && !self.peek()?.is_some_and(|t| t.kind == TokenKind::Str) {
                    text::Arg::Bare(self.number()?.to_string())
                } else {
                    self.string_argument()?
                },
            );
        }
        self.require(")")?;
        let folded = ca65_flat::call_read_ca65(name, &args, self.line, self.live)?;
        match folded {
            Some(text::Folded::Bare(source)) => Ok(vec![self.generated(
                source,
                if name == ".ident" {
                    TokenKind::Ident
                } else {
                    TokenKind::Str
                },
            )]),
            _ => Err(AsmError::new(
                self.line,
                format!("`{name}` did not produce a source token"),
            )),
        }
    }
}

/// Numeric view of the same stream. This borrows the operator grammar and
/// consumes source names and typed queries at their atom, before later input.
struct Numeric<'a, 'b> {
    input: &'a mut Input<'b>,
    current: Option<mos6502::Tok>,
    after_value: bool,
}

impl mos6502::ExprTokens for Numeric<'_, '_> {
    fn peek(&mut self) -> Result<Option<&mos6502::Tok>, AsmError> {
        use mos6502::Tok;
        if self.current.is_none() {
            let line = self.input.line;
            let after_value = self.after_value;
            self.current = self
                .input
                .peek()?
                .map(|token| match &token.kind {
                    TokenKind::Ident => Ok(Tok::SourceName(token.text().into())),
                    TokenKind::Register(_) => {
                        Err(AsmError::new(line, "register is not a numeric value"))
                    }
                    TokenKind::DotKeyword(name)
                        if matches!(
                            name.as_str(),
                            ".const"
                                | ".ref"
                                | ".referenced"
                                | ".def"
                                | ".defined"
                                | ".sizeof"
                                | ".strlen"
                                | ".strat"
                                | ".tcount"
                                | ".blank"
                                | ".match"
                                | ".xmatch"
                                | ".ismnem"
                                | ".ismnemonic"
                        ) =>
                    {
                        Ok(Tok::SourceCall {
                            name: name.clone(),
                            args: String::new(),
                        })
                    }
                    TokenKind::Punct(p) if p == "::" || super::anon_ref(p).is_some() => {
                        Ok(Tok::SourceName(p.clone()))
                    }
                    _ => {
                        let mut tokens = mos6502::tokenize_fragment(
                            token.text(),
                            line,
                            super::parse_number,
                            super::expression_options(),
                            None,
                            after_value,
                        )?;
                        if tokens.len() != 1 {
                            return Err(AsmError::new(line, "expected one numeric token"));
                        }
                        Ok(tokens.pop().expect("one token checked"))
                    }
                })
                .transpose()?;
        }
        Ok(self.current.as_ref())
    }

    fn advance(&mut self) {
        use mos6502::Tok;
        self.after_value = matches!(
            self.current,
            Some(
                Tok::Num(_)
                    | Tok::SourceName(_)
                    | Tok::SourceCall { .. }
                    | Tok::Pc
                    | Tok::RParen
                    | Tok::Str(_)
            )
        );
        self.current = None;
        self.input.current = None;
    }

    fn source_name(
        &mut self,
        name: &str,
        line: usize,
        source: Option<&dyn mos6502::ExprSource>,
    ) -> Result<Expr, AsmError> {
        let mut name = name.to_string();
        if name == "::" {
            name.push_str(&self.name_component()?);
        }
        // Finishing a native scoped name obtains its next current token
        // before the reference is recorded. An adjacent constructor can
        // provide `::`; a physical operator stops this lookahead.
        while self.input.at("::")? {
            self.input.current = None;
            name.push_str("::");
            name.push_str(&self.name_component()?);
        }
        source
            .ok_or_else(|| AsmError::new(line, "source name requires a context"))?
            .symbol(&name, line)
    }

    fn source_call(
        &mut self,
        name: &str,
        _: &str,
        line: usize,
        source: Option<&dyn mos6502::ExprSource>,
    ) -> Result<Expr, AsmError> {
        self.input.require("(")?;
        let value = match name {
            ".const" => Expr::Num(self.input.read_number(ca65_flat::NumericRead::Constancy)?),
            ".tcount" | ".blank" | ".match" | ".xmatch" => Expr::Num(self.input.query(name)?),
            ".strlen" | ".strat" => {
                let arg = self.input.string_argument()?;
                let bytes = self.input.live.strings.bytes(&arg, name, line)?;
                let value = if name == ".strlen" {
                    bytes.len() as i64
                } else {
                    self.input.require(",")?;
                    let index = self.input.read_number(ca65_flat::NumericRead::Literal)?;
                    i64::from(
                        *usize::try_from(index)
                            .ok()
                            .and_then(|i| bytes.get(i))
                            .ok_or_else(|| AsmError::new(line, "string index out of range"))?,
                    )
                };
                Expr::Num(value)
            }
            _ => {
                let mut argument = if self.input.at("::")? {
                    self.input.current = None;
                    format!("::{}", self.name_component()?)
                } else {
                    self.name_component()?
                };
                while self.input.at("::")? {
                    self.input.current = None;
                    argument.push_str("::");
                    argument.push_str(&self.name_component()?);
                }
                if matches!(name, ".ismnem" | ".ismnemonic") {
                    match ca65_flat::call_read_ca65(
                        name,
                        &[text::Arg::Bare(argument)],
                        line,
                        self.input.live,
                    )? {
                        Some(text::Folded::Number(value)) => Expr::Num(value),
                        _ => {
                            return Err(AsmError::new(
                                line,
                                "mnemonic query did not produce a number",
                            ));
                        }
                    }
                } else {
                    source
                        .ok_or_else(|| AsmError::new(line, "source query requires a context"))?
                        .function(name, &argument, line)?
                }
            }
        };
        self.input.require(")")?;
        Ok(value)
    }
}

impl Numeric<'_, '_> {
    fn name_component(&mut self) -> Result<String, AsmError> {
        let line = self.input.line;
        self.input
            .next()?
            .filter(|t| t.kind == TokenKind::Ident)
            .map(|t| t.text().to_string())
            .ok_or_else(|| AsmError::new(line, "identifier expected"))
    }
}

/// Read a captured query through the owned stream. Numeric calls inside a
/// capture remain tokens; only token-producing constructors run there.
pub(crate) fn captured_query(
    name: &str,
    arguments: &str,
    line: usize,
    live: ca65_flat::LiveText<'_>,
) -> Result<i64, AsmError> {
    // The final function delimiter is still part of the stream grammar.
    let mut input = Input::new(&format!("{arguments})"), &live, line);
    let value = input.query(name)?;
    input.require(")")?;
    if input.peek()?.is_some() {
        return Err(AsmError::new(line, "unexpected tokens after token list"));
    }
    Ok(value)
}

fn token_value(
    token: &Token,
    live: &ca65_flat::LiveText<'_>,
    line: usize,
) -> Result<Vec<u8>, AsmError> {
    if StringTokens::is_handle(token.text()) {
        live.strings.resolve(token.text(), line)
    } else {
        Ok(ca65_flat::token_value(token).into_bytes())
    }
}

#[derive(Clone, Copy)]
pub(super) enum DataGrammar {
    Bytes,
    Values,
    Asciiz,
}

/// Parse the directive's complete operand stream, so a selected comma stays
/// visible to this grammar rather than disappearing in an expression parse.
pub(super) fn data(
    source: &str,
    line: usize,
    grammar: DataGrammar,
    live: ca65_flat::LiveText<'_>,
) -> Result<Vec<Expr>, AsmError> {
    let mut input = Input::new(source, &live, line);
    let mut values = Vec::new();
    loop {
        if !matches!(grammar, DataGrammar::Values)
            && input.peek()?.is_some_and(|t| t.kind == TokenKind::Str)
        {
            let token = input.next()?.expect("string token checked");
            let bytes = live
                .strings
                .data(token.text(), line)?
                .ok_or_else(|| AsmError::new(line, "string constant expected"))?;
            values.extend(bytes.into_iter().map(|b| Expr::Num(i64::from(b))));
        } else if matches!(grammar, DataGrammar::Asciiz) {
            return Err(AsmError::new(line, "`.asciiz` needs string operands"));
        } else {
            values.push(live.context.expression_tokens(
                &mut Numeric {
                    input: &mut input,
                    current: None,
                    after_value: false,
                },
                line,
            )?);
        }
        if input.peek()?.is_none() {
            break;
        }
        input.require(",")?;
    }
    if matches!(grammar, DataGrammar::Asciiz) {
        values.push(Expr::Num(0));
    }
    Ok(values)
}

/// Allocation operands share the data stream, but consume source-time numbers
/// rather than deferred byte values. A fill is read even when no bytes are
/// needed, because reading its expression can reference a source name.
pub(super) fn allocation(
    directive: &str,
    source: &str,
    line: usize,
    live: ca65_flat::LiveText<'_>,
) -> Result<super::Kind, AsmError> {
    if !matches!(directive, "org" | "res" | "align") {
        return Err(AsmError::new(line, "unknown allocation directive"));
    }
    let mut input = Input::new(source, &live, line);
    let kind = if directive == "org" {
        super::Kind::Org(live.context.expression_tokens(
            &mut Numeric {
                input: &mut input,
                current: None,
                after_value: false,
            },
            line,
        )?)
    } else {
        let bound = input.number()?;
        let count = if directive == "res" {
            Some(
                usize::try_from(bound)
                    .map_err(|_| AsmError::new(line, "`.res` count must be non-negative"))?,
            )
        } else {
            if bound < 1 {
                return Err(AsmError::new(line, "`.align` boundary must be positive"));
            }
            None
        };
        let fill =
            if input.peek()?.is_none() {
                None
            } else {
                input.require(",")?;
                Some(u8::try_from(input.number()?).map_err(|_| {
                    AsmError::new(line, format!("`.{directive}` fill must be a byte"))
                })?)
            };
        match count {
            Some(count) => super::Kind::Res(count, fill),
            None => super::Kind::Align(bound, fill),
        }
    };
    if input.peek()?.is_some() {
        return Err(AsmError::new(
            line,
            format!("unexpected tokens after `.{directive}` operands"),
        ));
    }
    Ok(kind)
}

/// Assertions keep their deferred condition, while their action and message
/// are source-time fields on the same input (including generated separators).
pub(super) fn assertion(
    source: &str,
    line: usize,
    live: ca65_flat::LiveText<'_>,
) -> Result<super::Kind, AsmError> {
    let mut input = Input::new(source, &live, line);
    let condition = live.context.expression_tokens(
        &mut Numeric {
            input: &mut input,
            current: None,
            after_value: false,
        },
        line,
    )?;
    input.require(",")?;
    let action = input
        .next()?
        .filter(|token| token.kind == TokenKind::Ident)
        .ok_or_else(|| AsmError::new(line, "assertion action identifier expected"))?;
    let fatal = match action.text().to_ascii_lowercase().as_str() {
        "error" | "lderror" => true,
        "warning" | "ldwarning" => false,
        _ => return Err(AsmError::new(line, "unknown assertion action")),
    };
    let message = if input.peek()?.is_none() {
        "Assertion failed".into()
    } else {
        input.require(",")?;
        input.text()?
    };
    if input.peek()?.is_some() {
        return Err(AsmError::new(
            line,
            "unexpected tokens after assertion message",
        ));
    }
    Ok(super::Kind::Assert(condition, fatal, message))
}

/// Message tails are token skips, not expressions. Native token constructors
/// still run while warning/error tails are skipped; ordinary names stay inert.
pub(super) fn message(
    source: &str,
    strict: bool,
    severity: crate::engine::DiagSeverity,
    line: usize,
    live: ca65_flat::LiveText<'_>,
) -> Result<super::Kind, AsmError> {
    let mut input = Input::new(source, &live, line);
    if strict
        && !input
            .peek()?
            .is_some_and(|token| token.kind == TokenKind::Str)
    {
        return Err(AsmError::new(line, "`.out` takes one string"));
    }
    let text = input.text()?;
    if strict {
        if input.peek()?.is_some() {
            return Err(AsmError::new(line, "`.out` takes one string"));
        }
    } else {
        while input.next()?.is_some() {}
    }
    Ok(super::Kind::Message(severity, text))
}

impl Input<'_> {
    fn text(&mut self) -> Result<String, AsmError> {
        let token = self
            .next()?
            .filter(|token| token.kind == TokenKind::Str)
            .ok_or_else(|| AsmError::new(self.line, "string constant expected"))?;
        let (text, tail) = self.live.strings.text_prefix(token.text(), self.line)?;
        if !tail.is_empty() {
            return Err(AsmError::new(
                self.line,
                "unexpected tokens in string value",
            ));
        }
        Ok(text)
    }
}

/// A repeat header is one count and an optional identifier token, rather than
/// source split on the first authored comma. Constructors retain their kinds.
pub(super) fn repetition(
    source: &str,
    line: usize,
    live: ca65_flat::LiveText<'_>,
) -> Result<(i64, Option<String>), AsmError> {
    let mut input = Input::new(source, &live, line);
    let count = input.number()?;
    if count < 0 {
        return Err(AsmError::new(line, "repetition count may not be negative"));
    }
    let counter = if input.peek()?.is_none() {
        None
    } else {
        input.require(",")?;
        let token = input
            .next()?
            .filter(|token| token.kind == TokenKind::Ident && super::is_ident(token.text()))
            .ok_or_else(|| AsmError::new(line, "`.repeat` needs an identifier after the comma"))?;
        Some(token.text().to_string())
    };
    if input.peek()?.is_some() {
        return Err(AsmError::new(line, "unexpected tokens after repeat header"));
    }
    Ok((count, counter))
}

/// Read a complete numeric condition/value through the same owned stream.
/// A generated separator remains visible and is invalid in this grammar.
pub(super) fn value(
    source: &str,
    line: usize,
    live: ca65_flat::LiveText<'_>,
) -> Result<Expr, AsmError> {
    let mut input = Input::new(source, &live, line);
    let value = live.context.expression_tokens(
        &mut Numeric {
            input: &mut input,
            current: None,
            after_value: false,
        },
        line,
    )?;
    if input.peek()?.is_some() {
        return Err(AsmError::new(line, "unexpected tokens after expression"));
    }
    Ok(value)
}
