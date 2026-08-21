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

use asm198x::directives::{Category, DialectSurface};
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

/// A word no dialect declares assembles nowhere.
///
/// This is the other half of R3: the declaration is the only route in, so
/// something outside it falls through to instruction resolution and is turned
/// away. `include` would be the obvious probe and is the wrong one — most of
/// these dialects implement it.
///
/// The assertion is that it fails, not how, because the failure is worded
/// three ways and one of them names nothing at all: `unknown instruction
/// `frobnicate``, ``frobnicate` has no form for operands `1``, and the ca65
/// family's `no suitable addressing mode for this operand`. A reader given the
/// third has to guess whether they mistyped the mnemonic or the operand. That
/// is a diagnostic gap rather than a dispatch one, so it is recorded here and
/// not fixed here.
#[test]
fn an_undeclared_spelling_assembles_nowhere() {
    for surface in directives::surfaces() {
        let assemble = assembler(surface.dialect);
        assert!(
            directives::lookup(&surface.directives, "frobnicate").is_none(),
            "{}: the probe must not be a declared spelling",
            surface.dialect
        );
        assert!(
            assemble(" frobnicate 1\n").is_err(),
            "{}: `frobnicate` is not a directive and not an instruction",
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

/// Nothing is declared `KnownUnsupported` yet, stated rather than left unsaid.
///
/// The category exists for pasmo's include and asl's semantic pseudo-ops
/// (#87). When either lands, this count changes and someone reads the row.
#[test]
fn the_known_unsupported_count_is_zero() {
    let unsupported: Vec<String> = directives::surfaces()
        .iter()
        .flat_map(|s: &DialectSurface| {
            s.directives
                .iter()
                .filter(|d| d.category == Category::KnownUnsupported)
                .map(|d| format!("{}: {}", s.dialect, d.id))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(unsupported.is_empty(), "{unsupported:?}");
}
