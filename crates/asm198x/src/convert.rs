//! Dialect conversion (#502): read source in one dialect of a CPU, emit it in
//! another, and prove the pair byte-identical before handing anything back.
//!
//! The route is parse-and-re-emit, never disassemble-and-reassemble: the
//! source-preserving AST keeps labels, structure, and operand spelling, so
//! the output reads like source rather than like a hex dump with mnemonics.
//! Both ends are real reference dialects — nothing here invents syntax — and
//! the **self-verification gate is the feature's spine**: output exists only
//! when assembling the input under the source dialect and the output under
//! the target dialect yield byte-identical images. A conversion that cannot
//! prove itself is a reported error, never silent plausible output.
//!
//! v1 is the Z80 pair the plan named (pasmo → sjasmplus), instruction- and
//! directive-level. The two dialects share most of their surface, so the
//! structural rewrites are few and honest — the repetition closer is the
//! emblematic one: pasmo closes `REPT` with `ENDM`, the same word that closes
//! a macro, and sjasmplus refuses that; the AST carries every block's closer
//! as written, so the rewrite is a field edit, not a text guess.

use crate::ast::{Item, Node};
use crate::engine::AsmError;

/// A successful, verified conversion.
#[derive(Debug)]
pub struct Conversion {
    /// The output source, assembling byte-identical under the target dialect.
    pub output: String,
}

/// Convert `source` between two dialects of one CPU, self-verified.
///
/// # Errors
/// An unknown dialect pair; input that does not assemble under the source
/// dialect; output the target dialect refuses; or a byte divergence between
/// the two assemblies — reported with the first differing offset, with no
/// output handed back.
pub fn convert(from: &str, to: &str, source: &str) -> Result<Conversion, AsmError> {
    match (from, to) {
        ("pasmo", "sjasmplus") => pasmo_to_sjasmplus(source, false),
        ("pasmonext", "sjasmplus") => pasmo_to_sjasmplus(source, true),
        _ => Err(AsmError::new(
            0,
            format!(
                "no converter from `{from}` to `{to}` yet — v1 converts pasmo/pasmonext \
                 source to sjasmplus (#502 tracks the rest)"
            ),
        )),
    }
}

fn pasmo_to_sjasmplus(source: &str, z80n: bool) -> Result<Conversion, AsmError> {
    use crate::dialect::Dialect as _;

    // Parse under the source dialect: structure, labels, and verbatim
    // operation text, macros and repetitions unexpanded.
    let mut program = crate::dialects::Pasmo { z80n }
        .parse_ast(source)
        .map_err(|e| context(e, "the input does not parse under pasmo"))?
        .expect("pasmo builds an AST");
    rewrite_nodes(&mut program.nodes);

    // Emit through the shared layout rules — the same emitter the sjasmplus
    // formatter uses, so the output is canonically laid out.
    let output = crate::ast::emit(&program, true);

    // The gate: both sides assemble, and the images agree byte for byte.
    let ours = if z80n {
        crate::assemble_pasmonext(source)
    } else {
        crate::assemble_pasmo(source)
    }
    .map_err(|e| context(e, "the input does not assemble under pasmo"))?;
    let theirs = if z80n {
        crate::assemble_sjasmplus_next(&output)
    } else {
        crate::assemble_sjasmplus(&output)
    }
    .map_err(|e| {
        context(
            e,
            "the converted output does not assemble under sjasmplus — a construct \
             this converter does not translate yet; no output was written",
        )
    })?;
    if let Some(at) = first_divergence(&ours.bytes, &theirs.bytes) {
        return Err(AsmError::new(
            0,
            format!(
                "conversion does not verify: the images diverge at byte {at} \
                 (pasmo {} bytes, sjasmplus {} bytes); no output was written",
                ours.bytes.len(),
                theirs.bytes.len()
            ),
        ));
    }
    if ours.origin != theirs.origin {
        return Err(AsmError::new(
            0,
            format!(
                "conversion does not verify: origins differ ({:?} vs {:?}); \
                 no output was written",
                ours.origin, theirs.origin
            ),
        ));
    }
    Ok(Conversion { output })
}

/// The pasmo→sjasmplus structural rewrites, applied through every block body.
fn rewrite_nodes(nodes: &mut [Node]) {
    for node in nodes {
        match &mut node.item {
            // pasmo closes a repetition with `ENDM` — the macro closer — and
            // sjasmplus refuses that spelling on a REPT. The closer travels
            // in the tree exactly as written, so this is the one field edit,
            // in the author's own case.
            Some(Item::Repeat { close, body, .. }) => {
                if close.eq_ignore_ascii_case("endm") {
                    *close = match_case(close, "endr");
                }
                rewrite_nodes(body);
            }
            Some(Item::Conditional {
                then_body,
                else_body,
                ..
            }) => {
                rewrite_nodes(then_body);
                if let Some(body) = else_body {
                    rewrite_nodes(body);
                }
            }
            Some(Item::Loop { body, .. }) => rewrite_nodes(body),
            _ => {}
        }
    }
}

/// Spell `word` in the case pattern of `like` — all-caps stays all-caps.
fn match_case(like: &str, word: &str) -> String {
    if like.chars().all(|c| !c.is_ascii_lowercase()) {
        word.to_ascii_uppercase()
    } else {
        word.to_ascii_lowercase()
    }
}

fn first_divergence(a: &[u8], b: &[u8]) -> Option<usize> {
    if a == b {
        return None;
    }
    Some(
        a.iter()
            .zip(b.iter())
            .position(|(x, y)| x != y)
            .unwrap_or_else(|| a.len().min(b.len())),
    )
}

fn context(mut e: AsmError, what: &str) -> AsmError {
    e.message = format!("{what}: {}", e.message);
    e
}
