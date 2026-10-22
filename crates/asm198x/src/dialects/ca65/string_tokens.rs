//! Byte values retained by ca65's source pre-pass, before numeric layout.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::dialects::text;
use crate::engine::AsmError;

const PREFIX: &str = "\u{1}ca65-string:";
const END: char = '\u{1}';
static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
struct Value {
    bytes: Box<[u8]>,
    source: String,
    line: usize,
}

/// Values belong to one reader. Flat file expansions transfer their values to
/// that reader before parsing; globally distinct keys prevent cross-file aliasing.
#[derive(Clone, Default)]
pub(crate) struct StringTokens {
    values: RefCell<BTreeMap<usize, Value>>,
}

impl StringTokens {
    pub(crate) fn retain(&self, other: Self) {
        self.values
            .borrow_mut()
            .append(&mut other.values.into_inner());
    }

    pub(crate) fn intern(
        &self,
        bytes: Vec<u8>,
        source: String,
        line: usize,
    ) -> Result<String, AsmError> {
        let key = NEXT_TOKEN
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| AsmError::new(line, "too many computed string tokens"))?;
        self.values.borrow_mut().insert(
            key,
            Value {
                bytes: bytes.into_boxed_slice(),
                source,
                line,
            },
        );
        Ok(format!("{PREFIX}{key}{END}"))
    }

    /// Only physical source uses this boundary. Generated streams retain their
    /// handles; quoted occurrences and comments remain authored data.
    pub(crate) fn reject_authored(source: &str, line: usize) -> Result<(), AsmError> {
        if handle_spans(source).is_empty() {
            Ok(())
        } else {
            Err(AsmError::new(
                line,
                "reserved internal string marker in source",
            ))
        }
    }

    pub(crate) fn is_handle(source: &str) -> bool {
        source.starts_with(PREFIX)
    }

    pub(crate) fn handle_len(source: &str) -> Option<usize> {
        let rest = source.strip_prefix(PREFIX)?;
        Some(PREFIX.len() + rest.find(END)? + END.len_utf8())
    }

    fn key(source: &str, line: usize) -> Result<usize, AsmError> {
        source
            .strip_prefix(PREFIX)
            .and_then(|s| s.strip_suffix(END))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| AsmError::new(line, "invalid internal string token"))
    }

    pub(crate) fn resolve(&self, source: &str, line: usize) -> Result<Vec<u8>, AsmError> {
        let key = Self::key(source, line)?;
        self.values
            .borrow()
            .get(&key)
            .map(|v| v.bytes.to_vec())
            .ok_or_else(|| AsmError::new(line, "unknown internal string token"))
    }

    pub(crate) fn is_string(arg: &text::Arg) -> bool {
        matches!(arg, text::Arg::Text { .. })
            || matches!(arg, text::Arg::Bare(source) if Self::is_handle(source.trim()))
    }

    /// ca65 does not decode C-style escapes in physical quoted literals.
    pub(crate) fn bytes(
        &self,
        arg: &text::Arg,
        name: &str,
        line: usize,
    ) -> Result<Vec<u8>, AsmError> {
        if let Some(source) = arg.literal_spelling() {
            return Self::literal_bytes(source, line)?
                .ok_or_else(|| AsmError::new(line, format!("`{name}` takes a string here")));
        }
        if let text::Arg::Bare(source) = arg
            && Self::is_handle(source.trim())
        {
            return self.resolve(source.trim(), line);
        }
        Err(AsmError::new(line, format!("`{name}` takes a string here")))
    }

    pub(crate) fn data(&self, source: &str, line: usize) -> Result<Option<Vec<u8>>, AsmError> {
        let source = source.trim();
        if Self::is_handle(source) {
            return self.resolve(source, line).map(Some);
        }
        Self::literal_bytes(source, line)
    }

    fn literal_bytes(source: &str, line: usize) -> Result<Option<Vec<u8>>, AsmError> {
        let Some(rest) = source.strip_prefix('"') else {
            return Ok(None);
        };
        let Some(end) = rest.find('"') else {
            return Err(AsmError::new(line, "unterminated string literal"));
        };
        if end + 1 != rest.len() {
            return Err(AsmError::new(
                line,
                "unexpected tokens after string literal",
            ));
        }
        Ok(Some(rest.as_bytes()[..end].to_vec()))
    }

    pub(crate) fn asciiz<'a>(
        &self,
        items: impl IntoIterator<Item = &'a str>,
        line: usize,
    ) -> Result<Vec<u8>, AsmError> {
        let mut out = Vec::new();
        let mut has_item = false;
        for item in items {
            let bytes = self
                .data(item, line)?
                .ok_or_else(|| AsmError::new(line, "`.asciiz` requires string operands"))?;
            has_item = true;
            out.extend(bytes);
        }
        if !has_item {
            return Err(AsmError::new(line, "`.asciiz` requires string operands"));
        }
        out.push(0);
        Ok(out)
    }

    pub(crate) fn error(&self, mut error: AsmError) -> AsmError {
        error.message = self.display(&error.message);
        error
    }

    fn text_value(&self, handle: &str, line: usize) -> Result<String, AsmError> {
        let key = Self::key(handle, line)?;
        let values = self.values.borrow();
        let value = values
            .get(&key)
            .ok_or_else(|| AsmError::new(line, "unknown internal string token"))?;
        std::str::from_utf8(&value.bytes)
            .map(str::to_owned)
            .map_err(|_| {
                AsmError::new(
                    value.line,
                    "binary string cannot be used as UTF-8 text here",
                )
            })
    }

    /// Read one native text value without turning computed bytes into source.
    /// Each consumer decides whether the remaining source is allowed.
    pub(crate) fn text_prefix<'a>(
        &self,
        source: &'a str,
        line: usize,
    ) -> Result<(String, &'a str), AsmError> {
        let source = source.trim_start();
        if Self::is_handle(source) {
            let end = Self::handle_len(source)
                .ok_or_else(|| AsmError::new(line, "invalid internal string token"))?;
            return Ok((self.text_value(&source[..end], line)?, &source[end..]));
        }
        let body = source
            .strip_prefix('"')
            .ok_or_else(|| AsmError::new(line, "string constant expected"))?;
        let end = body
            .find('"')
            .ok_or_else(|| AsmError::new(line, "unterminated string"))?;
        Ok((body[..end].to_string(), &body[end + 1..]))
    }

    /// Text-only grammar positions retain their UTF-8 contract. Byte directives
    /// instead resolve values directly, without serializing the byte payload.
    pub(crate) fn text_source<'a>(
        &self,
        source: &'a str,
        line: usize,
    ) -> Result<Cow<'a, str>, AsmError> {
        if !source.contains(PREFIX) {
            return Ok(Cow::Borrowed(source));
        }
        self.rewrite(source, |handle| {
            let text = self.text_value(handle, line)?;
            Ok(format!("\"{text}\""))
        })
        .map(Cow::Owned)
    }

    /// Recover authored construction spelling for diagnostics and source fields.
    /// Quoted marker-looking bytes are literal data and never looked up.
    pub(crate) fn display(&self, source: &str) -> String {
        self.rewrite(source, |handle| {
            let value = Self::key(handle, 0)
                .ok()
                .and_then(|key| self.values.borrow().get(&key).map(|v| v.source.clone()));
            Ok(value.unwrap_or_else(|| "<invalid string token>".into()))
        })
        .unwrap_or_else(|_| "<invalid string token>".into())
    }

    fn rewrite(
        &self,
        source: &str,
        mut replace: impl FnMut(&str) -> Result<String, AsmError>,
    ) -> Result<String, AsmError> {
        let mut out = String::with_capacity(source.len());
        let mut copied = 0;
        for span in handle_spans(source) {
            out.push_str(&source[copied..span.start]);
            out.push_str(&replace(&source[span.clone()])?);
            copied = span.end;
        }
        out.push_str(&source[copied..]);
        Ok(out)
    }
}

/// Native quoted literals have no backslash escape. A character token consumes
/// one byte and its optional closing apostrophe, including an apostrophe token.
fn handle_spans(source: &str) -> Vec<std::ops::Range<usize>> {
    if !source.contains(PREFIX) {
        return Vec::new();
    }
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() && !matches!(bytes[i], b'"' | b'\n') {
                    i += 1;
                }
                i += usize::from(bytes.get(i) == Some(&b'"'));
            }
            b'\'' => {
                i = (i + 2).min(bytes.len());
                i += usize::from(bytes.get(i) == Some(&b'\''));
            }
            b';' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            1 if StringTokens::is_handle(&source[i..]) => {
                let len = StringTokens::handle_len(&source[i..]).unwrap_or(bytes.len() - i);
                spans.push(i..i + len);
                i += len;
            }
            _ => i += 1,
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_preserve_all_bytes_and_survive_file_transfer() {
        let file = StringTokens::default();
        let bytes: Vec<u8> = (0..=255).collect();
        let token = file
            .intern(bytes.clone(), ".sprintf(...)".into(), 7)
            .expect("token");
        assert_eq!(file.resolve(&token, 1).expect("value"), bytes);
        let assembly = StringTokens::default();
        assert!(assembly.resolve(&token, 1).is_err());
        assembly.retain(file);
        assert_eq!(assembly.resolve(&token, 1).expect("retained value"), bytes);
        assert_eq!(assembly.display(&token), ".sprintf(...)");
        let key = StringTokens::key(&token, 1).expect("key");
        assert_eq!(assembly.values.borrow()[&key].line, 7);
    }

    #[test]
    fn malformed_unknown_and_cross_reader_handles_fail_closed() {
        let first = StringTokens::default();
        let second = StringTokens::default();
        let a = first.intern(vec![34], "first".into(), 1).expect("first");
        let b = second.intern(vec![92], "second".into(), 2).expect("second");
        assert_ne!(a, b);
        assert!(first.resolve(&b, 1).is_err());
        assert!(second.resolve(&a, 1).is_err());
        assert!(first.text_prefix(&b, 1).is_err());
        assert!(second.text_prefix(&a, 1).is_err());
        assert!(first.text_prefix(PREFIX, 1).is_err());
        assert!(first.text_prefix(&format!("{PREFIX}x{END}"), 1).is_err());
        for source in [
            PREFIX.to_string(),
            format!("{PREFIX}x{END}"),
            format!("{a}tail"),
        ] {
            assert!(first.resolve(&source, 1).is_err());
        }
    }

    #[test]
    fn physical_source_cannot_inject_a_known_handle() {
        let store = StringTokens::default();
        let token = store
            .intern(vec![65], "constructor".into(), 1)
            .expect("token");
        assert!(StringTokens::reject_authored(&format!(".byte {token}"), 1).is_err());
        assert!(StringTokens::reject_authored(&format!(".byte ''' , {token}"), 1).is_err());
        assert!(StringTokens::reject_authored(&format!(".byte \"{token}\""), 1).is_ok());
        assert!(StringTokens::reject_authored(&format!("; {token}\n.byte 1"), 1).is_ok());
    }

    #[test]
    fn quoted_handle_spelling_remains_literal_data() {
        let store = StringTokens::default();
        let token = store
            .intern(vec![255], "constructor".into(), 1)
            .expect("token");
        let quoted = format!("\"{token}\"");
        let arg = text::Arg::Text {
            cooked: token.clone(),
            spelling: quoted.clone(),
        };
        assert_eq!(
            store.bytes(&arg, "test", 1).expect("literal"),
            token.as_bytes()
        );
        assert_eq!(store.display(&quoted), quoted);
        let (text, tail) = store.text_prefix(&quoted, 1).expect("literal text");
        assert_eq!(text.as_bytes(), token.as_bytes());
        assert!(tail.is_empty());
    }

    #[test]
    fn binary_values_remain_data_but_fail_the_utf8_text_contract() {
        let store = StringTokens::default();
        let token = store
            .intern(vec![255], "constructor".into(), 7)
            .expect("token");
        assert_eq!(store.data(&token, 1).expect("binary data"), Some(vec![255]));
        assert!(store.text_prefix(&token, 1).is_err());
        assert!(store.text_source(&token, 1).is_err());
    }
}
