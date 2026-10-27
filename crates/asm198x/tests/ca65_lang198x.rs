//! The gate for ca65's walk over Lang198x's front end
//! (`docs/plans/2026-10-27-ca65-walk/plan.md`).
//!
//! Every committed `ca65` verdict for the 6502, from the form audit, the
//! differential fuzz and the probes, is assembled through the walk and
//! checked against what ca65 V2.19 and ld65 did: the same bytes, or a
//! refusal where ca65 refused. Each case the walk does not match yet is
//! named here with the step that covers it, and the list may only shrink.

mod support;

use support::verdicts::corpus_path;
use verdict_corpus::{Corpus, Outcome, Suite};

/// The cases the walk does not match yet: a case's name, and the step of
/// the plan that covers it.
const NOT_YET: &[(&str, &str)] = &[
    // Functions that ask about the program or count tokens.
    (".defined finds a name further out", "6d"),
    (".ref and .const keep source order", "6d"),
    (".tcount of an opening brace", "6d"),
    // Unsupported by Lang198x's ca65 front end, as its preprocessing plan
    // records: `.ident` of a name that reads as a register, a `.endrepeat`
    // after other tokens on its line, and a byte above 127 made by
    // `.sprintf`. Each waits for a decision on how to hold such text.
    ("a generated name is a repeat counter", "unsupported"),
    ("a generated name is imported", "unsupported"),
    ("a generated register-spelt constant", "unsupported"),
    ("a generated register-spelt label", "unsupported"),
    (
        "a generated register-spelt label is exported",
        "unsupported",
    ),
    ("a label on .endrepeat", "unsupported"),
    ("a macro argument keeps a function's comma", "unsupported"),
];

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
