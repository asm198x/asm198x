//! Native source-token classification for the ca65 family.
//!
//! Shared by token-list functions and the canonical source reader. Token slices
//! retain authored spelling and opaque string handles; classification does not
//! evaluate names, expand source functions or validate skipped directives.

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
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
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
