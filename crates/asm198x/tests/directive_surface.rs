//! R4: the declaration and the parser agree, for every dialect.
//!
//! The per-dialect surfaces are only worth generating documentation from if
//! they describe what the assembler actually accepts. Two halves prove that,
//! and both are driven by [`directives::surfaces`] rather than a list written
//! here — so a dialect cannot be added, or a spelling dropped, without this
//! suite noticing.
//!
//! It runs through the public API on purpose. The generator that will consume
//! the surface is another crate, so a seam that works only from inside this one
//! is not the seam it needs.

use asm198x::directives::Category;
use asm198x::{AsmError, AssemblyResult, dialect_table, directives};

type Assemble = fn(&str) -> Result<AssemblyResult, AsmError>;

/// How to reach each declared dialect from outside the crate.
///
/// Hand-written, and kept honest by `every_dialect_surface_has_an_assembler`:
/// a surface with no entry here fails rather than going untested.
const ASSEMBLERS: &[(&str, Assemble)] = &[
    ("acme", asm198x::assemble_acme),
    ("ca65", asm198x::assemble_ca65),
    ("65816", asm198x::assemble_ca65_816),
    ("huc6280", asm198x::assemble_ca65_huc6280),
    ("vasm", asm198x::assemble_vasm),
    ("lwasm", asm198x::assemble_lwasm),
    ("rgbasm", asm198x::assemble_rgbasm),
    ("pasmo", asm198x::assemble_pasmo),
    ("sjasmplus", asm198x::assemble_sjasmplus),
    ("8080", asm198x::assemble_i8080),
    ("6800", asm198x::assemble_m6800),
    ("1802", asm198x::assemble_1802),
    ("8048", asm198x::assemble_8048),
    ("scmp", asm198x::assemble_scmp),
    ("f8", asm198x::assemble_f8),
    ("2650", asm198x::assemble_2650),
    ("tms7000", asm198x::assemble_tms7000),
    ("pdp11", asm198x::assemble_pdp11),
    ("tms9900", asm198x::assemble_tms9900),
    ("cp1610", asm198x::assemble_cp1610),
    ("z8000", asm198x::assemble_z8000),
];

fn assembler(dialect: &str) -> Assemble {
    ASSEMBLERS
        .iter()
        .find(|(name, _)| *name == dialect)
        .map(|(_, f)| *f)
        .unwrap_or_else(|| panic!("`{dialect}` has a declared surface but no assembler here"))
}

/// Whether an error refuses the word by name, as something the dialect has no
/// such thing as.
///
/// That is the one refusal a declared spelling must never draw. Every other
/// failure — a missing argument, an unterminated block, an origin the dialect
/// insists on — means the word was recognised and its body ran, which is all
/// the declaration claims.
///
/// Two wordings, because there are two ways to fall through to instruction
/// resolution and be turned away by it. Most dialects look the mnemonic up and
/// answer `unknown instruction`; the Z80 family and rgbasm build the operand
/// combinations first and answer `has no form for operands`, which names the
/// word without ever saying it is not a word. Both are counted, so a dialect
/// with the weaker diagnostic is not the one place a lost dispatch arm could
/// hide.
fn refused_by_name(err: &AsmError, word: &str) -> bool {
    let message = err.to_string().to_lowercase();
    let word = word.to_lowercase();
    message.contains(&word) && (message.contains("unknown") || message.contains("has no form"))
}

/// Every spelling a dialect declares is a word that dialect knows.
#[test]
fn every_declared_spelling_is_recognised() {
    for surface in directives::surfaces() {
        let assemble = assembler(surface.dialect);
        for directive in &surface.directives {
            for spelling in &directive.spellings() {
                let source = format!(" {spelling} 1\n");
                if let Err(err) = assemble(&source) {
                    assert!(
                        !refused_by_name(&err, spelling),
                        "{}: `{spelling}` is declared and the parser does not know it: {err}",
                        surface.dialect
                    );
                }
            }
        }
    }
}

/// A word no dialect declares is refused as an unknown instruction, everywhere.
///
/// This is the other half of R3: the declaration is the only route in, so
/// something outside it falls through to instruction resolution and is turned
/// away. `include` would be the obvious probe and is the wrong one — most of
/// these dialects implement it.
///
/// The assertion used to be that it fails, not how, because the refusal was
/// worded three ways and one of them named nothing: `` `x` has no form for
/// operands `1` `` implied the word existed, and the ca65 family's `no suitable
/// addressing mode for this operand` pointed at an operand that was never the
/// problem. Five dialects check the mnemonic before touching operands now, so
/// the wording is one thing and this asserts it.
///
/// **acme names the operand rather than the probe, and that is correct.** An
/// indented bare word is a *label* in acme, so `frobnicate 1` reads as a label
/// and a mnemonic `1` — which is what real acme does too (it accepts it with
/// "Label name not in leftmost column", probed 2026-08-22). The unknown
/// instruction genuinely is `1`.
#[test]
fn an_undeclared_spelling_is_refused_as_an_unknown_instruction() {
    for surface in directives::surfaces() {
        let assemble = assembler(surface.dialect);
        assert!(
            directives::lookup(&surface.directives, "frobnicate").is_none(),
            "{}: the probe must not be a declared spelling",
            surface.dialect
        );
        let err = assemble(" frobnicate 1\n").expect_err("not a word anywhere");
        assert!(
            err.to_string().contains("unknown instruction"),
            "{}: refused, but not as an unknown instruction: {err}",
            surface.dialect
        );
    }
}

/// Every declared surface can be reached from outside the crate.
#[test]
fn every_dialect_surface_has_an_assembler() {
    for surface in directives::surfaces() {
        assembler(surface.dialect);
    }
    for (name, _) in ASSEMBLERS {
        assert!(
            directives::surfaces().iter().any(|s| s.dialect == *name),
            "`{name}` is listed here with no declared surface"
        );
    }
}

/// Dialects that share another's surface rather than declaring their own.
///
/// A variant selects a different **target** — the Next's Z80N, the ROM-less
/// MCS-48 parts, the segmented Z8000 — which is not a syntax difference, so it
/// has no vocabulary of its own. Naming them is what keeps their absence from
/// `surfaces()` a fact rather than a hole.
const SHARES_A_SURFACE: &[(&str, &str)] =
    &[("pasmonext", "pasmo"), ("8035", "8048"), ("z8001", "z8000")];

/// Every dialect `--dialect` offers either declares a surface or is named as
/// sharing one.
///
/// The list of names comes from the same table the command line resolves
/// against, so adding a dialect fails here until its vocabulary is declared —
/// which is the difference between a surface that is absent and one that is
/// silently empty.
#[test]
fn every_selectable_dialect_is_accounted_for() {
    let declared: Vec<&str> = directives::surfaces().iter().map(|s| s.dialect).collect();
    for entry in dialect_table::DIALECTS {
        if let Some((_, shared)) = SHARES_A_SURFACE.iter().find(|(v, _)| *v == entry.name) {
            assert!(
                declared.contains(shared),
                "`{}` shares `{shared}`, which is not declared",
                entry.name
            );
            continue;
        }
        assert!(
            declared.contains(&entry.name),
            "`{}` is selectable and declares no directive surface",
            entry.name
        );
    }
}

/// Every `KnownUnsupported` spelling is refused as one, and says so.
///
/// The category spent two plans with no members and a test asserting the count
/// was zero. It has one now — pasmo's `include` — and the useful invariant is
/// not how many there are but that each one draws a diagnostic saying the
/// directive is real and unimplemented, rather than the unknown-mnemonic
/// refusal that sends a reader to check their own source.
#[test]
fn a_known_unsupported_spelling_says_which_it_is() {
    let mut checked = 0;
    for surface in directives::surfaces() {
        let assemble = assembler(surface.dialect);
        for directive in &surface.directives {
            if directive.category != Category::KnownUnsupported {
                continue;
            }
            for spelling in &directive.spellings() {
                let source = format!(" {spelling} \"x\"\n");
                let err = assemble(&source).expect_err("declared unimplemented");
                let message = err.to_string();
                assert!(
                    message.contains("does not implement"),
                    "{}: `{spelling}` is declared unsupported and refuses as \
                     something else: {message}",
                    surface.dialect
                );
                assert!(
                    !refused_by_name(&err, spelling),
                    "{}: `{spelling}` still reads as an unknown word: {message}",
                    surface.dialect
                );
                checked += 1;
            }
        }
    }
    // A category nothing declares is a category nothing checks. It was empty
    // for two plans; this is what notices if it empties again.
    assert!(
        checked > 0,
        "no dialect declares a `KnownUnsupported` spelling — pasmo's `include` \
         should be one"
    );
}
