//! The gate for ca65's walk over Lang198x's front end
//! (`docs/plans/2026-10-27-ca65-walk/plan.md`).
//!
//! Every committed `ca65` verdict for the 6502, from the form audit, the
//! differential fuzz and the probes, is assembled through the walk and
//! checked against what ca65 V2.19 and ld65 did: the same bytes, or a
//! refusal where ca65 refused. Each case the walk does not match yet is
//! named here with the step that covers it, and the list may only shrink.
//!
//! The NES curriculum's programs are checked the same way, linked as the
//! curriculum links them: every `ca65-nes` verdict's ROM, by its digest.
//! Their source is Code198x's, found through `ASM198X_CODE_SAMPLES` at the
//! pinned revision; without it, that half has nothing to check.

mod support;

use support::verdicts::{code_samples_root, corpus_path, parse_curriculum_key, sha256_hex};
use verdict_corpus::{Corpus, Outcome, Suite};

/// The cases the walk does not match yet: a case's name, and the step of
/// the plan that covers it.
const NOT_YET: &[(&str, &str)] = &[];

#[test]
fn the_walk_gives_ca65s_verdicts() {
    let corpus = Corpus::read(&corpus_path("6502")).expect("the 6502 corpus");
    let mut checked = 0;
    let mut differ: Vec<(String, String)> = Vec::new();
    for verdict in corpus.verdicts() {
        if verdict.dialect != "ca65" || verdict.suite == Suite::Curriculum {
            continue;
        }
        let key = verdict.key();
        let ours = asm198x::assemble_ca65_lang198x(&key.source).map(|r| r.bytes);
        let matches = match &verdict.outcome {
            Outcome::Rejected { .. } => ours.is_err(),
            outcome => ours.as_ref().ok() == outcome.bytes().as_ref(),
        };
        checked += 1;
        if !matches {
            let ours = match &ours {
                Ok(bytes) => format!("{bytes:02X?}"),
                Err(e) => format!("refused: {e}"),
            };
            differ.push((
                key.case.clone(),
                format!(
                    "{} [{:?}]: ours {ours}, ca65 {:?}\n{}",
                    key.case, verdict.suite, verdict.outcome, key.source
                ),
            ));
        }
    }
    eprintln!(
        "ca65 walk: {checked} verdicts checked, {} differ",
        differ.len()
    );
    assert!(checked > 300, "the corpus lost its ca65 verdicts");
    let unnamed: Vec<&String> = differ
        .iter()
        .filter(|(case, _)| !NOT_YET.iter().any(|(named, _)| named == case))
        .map(|(_, detail)| detail)
        .collect();
    let stale: Vec<&str> = NOT_YET
        .iter()
        .filter(|(named, _)| !differ.iter().any(|(case, _)| case == named))
        .map(|(named, _)| *named)
        .collect();
    assert!(
        unnamed.is_empty(),
        "{} differences are not named:\n{}",
        unnamed.len(),
        unnamed
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        stale.is_empty(),
        "these now match ca65, so take them off the list: {stale:?}"
    );
}

/// The curriculum programs the walk does not build as ca65 and ld65 did
/// yet: a program's path, and the step of the plan that covers it.
const CURRICULUM_NOT_YET: &[(&str, &str)] = &[];

#[test]
fn the_walk_builds_the_nes_curriculum_as_ca65_did() {
    let Some(root) = code_samples_root() else {
        eprintln!("no curriculum checkout: set ASM198X_CODE_SAMPLES to check it");
        return;
    };
    let corpus = Corpus::read(&corpus_path("6502")).expect("the 6502 corpus");
    let mut checked = 0;
    let mut differ: Vec<(String, String)> = Vec::new();
    for verdict in corpus.verdicts() {
        if verdict.dialect != "ca65-nes" || verdict.suite != Suite::Curriculum {
            continue;
        }
        let key = verdict.key();
        let Outcome::Digest { digest } = &verdict.outcome else {
            continue;
        };
        let (relpath, _, source_digest) =
            parse_curriculum_key(&key.source).expect("a curriculum key");
        let source = std::fs::read_to_string(root.join(relpath))
            .unwrap_or_else(|e| panic!("{relpath}: {e}"));
        assert_eq!(
            sha256_hex(source.as_bytes()),
            source_digest,
            "{relpath} is not at the pinned revision"
        );
        checked += 1;
        let ours = asm198x::assemble_ca65_nes_lang198x(&source).map(|r| r.bytes);
        let matches = matches!(&ours, Ok(bytes) if sha256_hex(bytes) == *digest);
        if !matches {
            let ours = match &ours {
                Ok(bytes) => format!("digest {}", sha256_hex(bytes)),
                Err(e) => format!("refused: {e}"),
            };
            differ.push((relpath.to_owned(), format!("{relpath}: ours {ours}")));
        }
    }
    eprintln!(
        "ca65 walk: {checked} curriculum programs checked, {} differ",
        differ.len()
    );
    assert!(checked >= 51, "the corpus lost its ca65-nes verdicts");
    let unnamed: Vec<&String> = differ
        .iter()
        .filter(|(path, _)| !CURRICULUM_NOT_YET.iter().any(|(named, _)| named == path))
        .map(|(_, detail)| detail)
        .collect();
    let stale: Vec<&str> = CURRICULUM_NOT_YET
        .iter()
        .filter(|(named, _)| !differ.iter().any(|(path, _)| path == named))
        .map(|(named, _)| *named)
        .collect();
    assert!(
        unnamed.is_empty(),
        "{} programs differ and are not named:\n{}",
        unnamed.len(),
        unnamed
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        stale.is_empty(),
        "these now match ca65, so take them off the list: {stale:?}"
    );
}
