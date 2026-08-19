//! Recorded reference-assembler verdicts — the byte-identical guarantee,
//! provable without the reference tools.
//!
//! Asm198x's central claim is that its output is byte-identical to the real
//! assembler for the dialect. That claim is arbitrated by shelling out to
//! `acme`, `ca65`, `asl`, `pasmo`, `sjasmplus`, `vasm`, `lwasm` and `rgbasm` —
//! so it can only be checked on a machine that has all of them, which today is
//! one machine. Every such suite is `#[ignore]`d, CI installs no reference
//! tool, and an outside contributor cannot prove anything in a pull request.
//!
//! A verdict is one recorded fact: *this arbiter, at this identity, given this
//! source text, produced these bytes* (or rejected it, and why). Recorded once
//! in **live mode** where the tools exist, committed, and **replayed** forever
//! by tests that need no tools at all.
//!
//! # What a verdict is not
//!
//! It is not an expectation someone wrote down. Nothing here is authored by
//! hand: every record is the observed output of a real tool run, and the
//! arbiter's identity travels with it so a fact can always be traced to what
//! produced it. That is the difference between a corpus and a pile of goldens —
//! a golden says what we think should happen, a verdict says what did.
//!
//! # Identity, corroboration, and alarms
//!
//! Verdicts are keyed on **behavioural identity** (the tool's own version
//! self-report) plus the source text, because that is what determines
//! behaviour. The **binary digest** rides along as provenance rather than as
//! part of the key, which makes the useful distinction possible:
//!
//! - the same identity and text producing the same bytes from two *different*
//!   binaries is one fact **corroborated** twice — a rebuilt or repackaged
//!   arbiter agreeing with itself;
//! - the same identity and text producing *different* bytes is an
//!   [`Resolution::Alarm`]. Something is wrong with the chain of trust, and it
//!   must be adjudicated rather than silently resolved by recency.
//!
//! An alarm is settled by a [`Record::Supersede`], which names the record it
//! retires and why. The retired record stays in the file — inert, still
//! walkable — because the history of a corrected fact is worth as much as the
//! correction.
//!
//! # On-disk shape
//!
//! One NDJSON file per CPU: one JSON object per line, appended, never rewritten.
//! An unrecognised `t` is **skipped, not fatal**, so a corpus written by a newer
//! producer still replays on an older checkout — the same additive-evolution
//! posture Debug198x takes.
//!
//! Reference bytes are stored as uppercase hex rather than a digest, so a
//! divergence is legible in a diff without any tool: you can see which byte
//! moved.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Which suite produced a verdict. Kept as a closed set so a typo cannot
/// silently create a category nothing ever replays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Suite {
    /// One `isa` form, synthesised and round-tripped through the reference.
    Form,
    /// A mnemonic-group chunk of an opcode-space sweep.
    SweepChunk,
    /// A hand-written differential probe (a directive or syntax shape).
    Probe,
    /// A seeded differential-fuzzer case.
    Fuzz,
    /// A curated curriculum program, recorded as a digest rather than bytes.
    Curriculum,
}

/// The tool that produced a verdict, and how confidently it can be identified.
///
/// `identity` is the tool's own version self-report — the thing that actually
/// predicts behaviour, and so the thing verdicts are keyed on. `digest` is a
/// hash of the binary that ran, carried as provenance: it never keys a lookup,
/// but it is what lets two agreeing records be recognised as corroboration and
/// two disagreeing ones be traced.
///
/// The digest is an opaque string here. Computing it belongs to the harness
/// that has the binary in hand, not to the format that stores it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Arbiter {
    /// The executable's name, as invoked (`"asl"`, `"ca65"`).
    pub tool: String,
    /// The tool's behavioural self-report (`"1.42 Beta [Bld 250]"`).
    pub identity: String,
    /// A hash of the binary that ran. Provenance only — never part of the key.
    pub digest: String,
}

/// What the arbiter did with the source text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum Outcome {
    /// It assembled, producing exactly these bytes (uppercase hex).
    Bytes { hex: String },
    /// It assembled, and the result is recorded as a digest rather than bytes —
    /// the curriculum path, where the payload is large and the source is not
    /// ours to copy into this repo.
    Digest { digest: String },
    /// It **deliberately** rejected the source, with a diagnostic attributable
    /// to the text. A crash, an I/O failure or a missing tool is not this: it is
    /// not a verdict at all and must never be recorded.
    Rejected { diagnostic: String },
    /// Both assemblers accepted and produced *different* bytes, and that
    /// difference is known and tracked rather than a regression. The
    /// `divergence` id joins this fact to the in-repo expectation of our own
    /// output, so neither half can go missing unnoticed.
    Divergence { divergence: String, hex: String },
}

impl Outcome {
    /// The reference bytes, where the outcome carries them literally.
    #[must_use]
    pub fn bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::Bytes { hex } | Self::Divergence { hex, .. } => decode_hex(hex),
            Self::Digest { .. } | Self::Rejected { .. } => None,
        }
    }
}

/// One recorded fact: an arbiter, a case, and what happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    /// Which suite recorded it.
    pub suite: Suite,
    /// The CPU whose corpus file this belongs to (`"8080"`, `"z80"`).
    pub cpu: String,
    /// The source dialect the text is written in (`"asl"`, `"acme"`).
    pub dialect: String,
    /// A short human label for the case (`"lda #imm"`, `"colon-inline IF"`).
    /// Never part of the key — it is there so a diff reads.
    pub case: String,
    /// The exact source text the arbiter was given. Stored verbatim rather than
    /// hashed: it is the key, and a key you can read is a key you can debug.
    pub source: String,
    /// Who produced the fact.
    pub arbiter: Arbiter,
    /// What they did with it.
    #[serde(flatten)]
    pub outcome: Outcome,
}

impl Verdict {
    /// The lookup key: behavioural identity plus source text. Deliberately
    /// excludes the binary digest, so that two builds of the same release
    /// corroborate one fact instead of forking it.
    #[must_use]
    pub fn key(&self) -> Key {
        Key {
            identity: self.arbiter.identity.clone(),
            source: self.source.clone(),
        }
    }

    /// A stable content id, used to name this record in a supersede. Derived
    /// from every field including the digest, so two genuinely distinct
    /// observations never share one.
    #[must_use]
    pub fn id(&self) -> String {
        // Not a cryptographic identity — a name for one line in one file. The
        // digest is already carried separately for provenance.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |s: &str| {
            for b in s.as_bytes() {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        mix(&self.cpu);
        mix(&self.dialect);
        mix(&self.case);
        mix(&self.source);
        mix(&self.arbiter.tool);
        mix(&self.arbiter.identity);
        mix(&self.arbiter.digest);
        mix(&serde_json::to_string(&self.outcome).unwrap_or_default());
        format!("{h:016x}")
    }
}

/// What a lookup is keyed on. See [`Verdict::key`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    /// The arbiter's behavioural self-report.
    pub identity: String,
    /// The source text it was given.
    pub source: String,
}

/// A line in a corpus file. Anything whose `t` is not recognised parses as
/// [`Record::Unknown`] and is carried, not rejected.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    /// A recorded fact.
    Verdict(Box<Verdict>),
    /// An adjudication retiring an earlier record.
    Supersede(Supersede),
    /// A record kind this build does not know. Skipped by every lookup, kept so
    /// a newer producer's corpus still replays here.
    Unknown(serde_json::Value),
}

/// Retires one record in favour of the rest, settling an [`Resolution::Alarm`].
///
/// The retired record is not deleted. A corrected fact and the reason it needed
/// correcting are both worth keeping, and a supersede chain that cannot be
/// walked backwards explains nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Supersede {
    /// The [`Verdict::id`] being retired.
    pub retires: String,
    /// Why — free text, for the human reading the diff.
    pub reason: String,
}

/// How a set of verdicts sharing one [`Key`] resolves.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution<'a> {
    /// One agreed outcome. `corroborations` lists every distinct binary digest
    /// that produced it — more than one means independent confirmation.
    Fact {
        /// The outcome every live record agrees on.
        outcome: &'a Outcome,
        /// Distinct binary digests that produced it, sorted.
        corroborations: Vec<&'a str>,
    },
    /// Live records disagree about what the arbiter did. Not resolvable by
    /// recency: it needs a [`Supersede`].
    Alarm {
        /// The conflicting records, in file order.
        conflicting: Vec<&'a Verdict>,
    },
}

/// A parsed corpus: every record in file order, plus the resolved lookup.
#[derive(Clone, Debug, Default)]
pub struct Corpus {
    records: Vec<Record>,
}

impl Corpus {
    /// Parse NDJSON. A blank line is skipped; a line that is not JSON at all is
    /// an error, because that is corruption rather than evolution.
    ///
    /// # Errors
    ///
    /// Returns the 1-based line number and message of the first unparseable line.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut records = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(line).map_err(|e| ParseError {
                line: i + 1,
                message: e.to_string(),
            })?;
            records.push(classify(value));
        }
        Ok(Self { records })
    }

    /// Read a corpus file. A file that does not exist is an empty corpus, not an
    /// error — a CPU with nothing recorded yet is an ordinary state.
    ///
    /// # Errors
    ///
    /// Returns a read or parse failure.
    pub fn read(path: &Path) -> Result<Self, ParseError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ParseError {
                line: 0,
                message: e.to_string(),
            }),
        }
    }

    /// Every record, in file order — including superseded and unknown ones, so
    /// a chain stays walkable.
    #[must_use]
    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// The verdicts this build understands, in file order, including retired
    /// ones.
    pub fn verdicts(&self) -> impl Iterator<Item = &Verdict> {
        self.records.iter().filter_map(|r| match r {
            Record::Verdict(v) => Some(&**v),
            _ => None,
        })
    }

    /// Ids retired by a supersede.
    #[must_use]
    pub fn retired(&self) -> Vec<&str> {
        self.records
            .iter()
            .filter_map(|r| match r {
                Record::Supersede(s) => Some(s.retires.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Resolve one case: what does the corpus say this arbiter did with this
    /// text? `None` if nothing was ever recorded, or if everything recorded has
    /// been retired.
    #[must_use]
    pub fn resolve(&self, key: &Key) -> Option<Resolution<'_>> {
        let retired = self.retired();
        let live: Vec<&Verdict> = self
            .verdicts()
            .filter(|v| v.arbiter.identity == key.identity && v.source == key.source)
            .filter(|v| !retired.contains(&v.id().as_str()))
            .collect();
        let first = live.first()?;

        if live.iter().all(|v| v.outcome == first.outcome) {
            let mut corroborations: Vec<&str> =
                live.iter().map(|v| v.arbiter.digest.as_str()).collect();
            corroborations.sort_unstable();
            corroborations.dedup();
            return Some(Resolution::Fact {
                outcome: &first.outcome,
                corroborations,
            });
        }
        Some(Resolution::Alarm { conflicting: live })
    }

    /// Every distinct case in the corpus, resolved. Sorted, so a caller
    /// iterating for a report gets a stable order.
    #[must_use]
    pub fn resolved(&self) -> BTreeMap<Key, Resolution<'_>> {
        let mut keys: Vec<Key> = self.verdicts().map(Verdict::key).collect();
        keys.sort();
        keys.dedup();
        keys.into_iter()
            .filter_map(|k| self.resolve(&k).map(|r| (k, r)))
            .collect()
    }
}

/// Classify one parsed line. An unrecognised `t` becomes [`Record::Unknown`]
/// rather than an error — and so does a *recognised* `t` whose payload this
/// build cannot deserialize, since refusing to load the whole file over one
/// record we do not understand is the failure mode this policy exists to avoid.
fn classify(value: serde_json::Value) -> Record {
    match value.get("t").and_then(serde_json::Value::as_str) {
        Some("verdict") => serde_json::from_value(value.clone())
            .map_or(Record::Unknown(value), |v| Record::Verdict(Box::new(v))),
        Some("supersede") => {
            serde_json::from_value(value.clone()).map_or(Record::Unknown(value), Record::Supersede)
        }
        _ => Record::Unknown(value),
    }
}

/// Append records to a CPU's corpus file, creating it if absent.
///
/// Appending is the only write. A corpus is never rewritten in place, because a
/// fact that can be edited away is not evidence.
///
/// # Errors
///
/// Returns any I/O or serialization failure.
pub fn append(path: &Path, records: &[Record]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for record in records {
        writeln!(file, "{}", to_line(record)?)?;
    }
    Ok(())
}

/// Serialize one record to its NDJSON line. Field order follows the struct
/// declarations, so output is byte-stable across runs and diffs stay readable.
///
/// # Errors
///
/// Returns a serialization failure.
pub fn to_line(record: &Record) -> std::io::Result<String> {
    let value = match record {
        Record::Verdict(v) => {
            let mut m = serde_json::to_value(&**v).map_err(std::io::Error::other)?;
            insert_tag(&mut m, "verdict");
            m
        }
        Record::Supersede(s) => {
            let mut m = serde_json::to_value(s).map_err(std::io::Error::other)?;
            insert_tag(&mut m, "supersede");
            m
        }
        Record::Unknown(v) => v.clone(),
    };
    serde_json::to_string(&value).map_err(std::io::Error::other)
}

/// Tag the object with its record kind.
///
/// Key order is `serde_json`'s, which sorts — so `t` lands alphabetically
/// rather than first. That is the deterministic order the corpus wants: a
/// record's bytes depend only on its content, never on the order a producer
/// happened to build it in, so re-recording an unchanged fact produces an
/// identical line and diffs show only what actually moved.
fn insert_tag(value: &mut serde_json::Value, tag: &str) {
    if let Some(map) = value.as_object_mut() {
        map.insert("t".to_string(), serde_json::Value::String(tag.to_string()));
    }
}

/// Uppercase hex, the storage form for reference bytes.
#[must_use]
pub fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02X}");
    }
    s
}

/// Parse [`encode_hex`] output. `None` if it is not clean hex.
#[must_use]
pub fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

/// A corpus line that is not JSON at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number, or 0 for a whole-file failure.
    pub line: usize,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "corpus unreadable: {}", self.message)
        } else {
            write!(f, "corpus line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for ParseError {}

pub mod derive;

#[cfg(test)]
mod tests {
    use super::*;

    fn arbiter(digest: &str) -> Arbiter {
        Arbiter {
            tool: "asl".to_string(),
            identity: "1.42 Beta [Bld 250]".to_string(),
            digest: digest.to_string(),
        }
    }

    fn verdict(digest: &str, hex: &str) -> Verdict {
        Verdict {
            suite: Suite::Form,
            cpu: "8080".to_string(),
            dialect: "asl".to_string(),
            case: "mvi a,imm".to_string(),
            source: " mvi a,12h\n".to_string(),
            arbiter: arbiter(digest),
            outcome: Outcome::Bytes {
                hex: hex.to_string(),
            },
        }
    }

    fn key() -> Key {
        Key {
            identity: "1.42 Beta [Bld 250]".to_string(),
            source: " mvi a,12h\n".to_string(),
        }
    }

    /// Every field survives the write/read round trip. The corpus is evidence,
    /// so a field that quietly fails to persist is a fact quietly lost.
    #[test]
    fn a_verdict_round_trips_every_field() {
        let original = verdict("sha-a", "3E12");
        let line = to_line(&Record::Verdict(Box::new(original.clone()))).expect("serialize");
        let parsed = Corpus::parse(&line).expect("parse");
        assert_eq!(parsed.verdicts().collect::<Vec<_>>(), vec![&original]);
        assert!(
            line.contains(r#""t":"verdict""#),
            "kind is recorded: {line}"
        );
        // Serialization is a pure function of content: the same fact written
        // twice is byte-identical, which is what keeps corpus diffs honest.
        assert_eq!(
            line,
            to_line(&Record::Verdict(Box::new(original))).expect("serialize"),
        );
    }

    /// A record kind this build does not know is carried, not fatal — and the
    /// records around it still resolve. This is the whole reason an older
    /// checkout can replay a corpus a newer producer wrote.
    #[test]
    fn an_unknown_record_kind_is_skipped_not_fatal() {
        let text = format!(
            "{}\n{}\n",
            r#"{"t":"provenance-note","note":"from a later build"}"#,
            to_line(&Record::Verdict(Box::new(verdict("sha-a", "3E12")))).expect("serialize"),
        );
        let corpus = Corpus::parse(&text).expect("parse");
        assert_eq!(corpus.records().len(), 2, "the unknown record is kept");
        assert!(matches!(corpus.records()[0], Record::Unknown(_)));
        assert!(
            matches!(corpus.resolve(&key()), Some(Resolution::Fact { .. })),
            "the verdict beside it still resolves"
        );
    }

    /// Two different binaries reporting the same version and producing the same
    /// bytes are one fact, confirmed twice — not a conflict. Keying on
    /// behaviour rather than on the binary is what makes that distinction
    /// available at all.
    #[test]
    fn the_same_bytes_from_two_binaries_corroborate_one_fact() {
        let text = format!(
            "{}\n{}\n",
            to_line(&Record::Verdict(Box::new(verdict("sha-a", "3E12")))).expect("ser"),
            to_line(&Record::Verdict(Box::new(verdict("sha-b", "3E12")))).expect("ser"),
        );
        let corpus = Corpus::parse(&text).expect("parse");
        match corpus.resolve(&key()) {
            Some(Resolution::Fact {
                outcome,
                corroborations,
            }) => {
                assert_eq!(outcome.bytes(), Some(vec![0x3E, 0x12]));
                assert_eq!(corroborations, vec!["sha-a", "sha-b"]);
            }
            other => panic!("expected one corroborated fact, got {other:?}"),
        }
    }

    /// The same version producing *different* bytes is not resolvable by
    /// recency. Something in the trust chain is wrong and a human has to say
    /// which record is right.
    #[test]
    fn the_same_identity_disagreeing_raises_an_alarm() {
        let text = format!(
            "{}\n{}\n",
            to_line(&Record::Verdict(Box::new(verdict("sha-a", "3E12")))).expect("ser"),
            to_line(&Record::Verdict(Box::new(verdict("sha-b", "3E99")))).expect("ser"),
        );
        let corpus = Corpus::parse(&text).expect("parse");
        match corpus.resolve(&key()) {
            Some(Resolution::Alarm { conflicting }) => assert_eq!(conflicting.len(), 2),
            other => panic!("expected an alarm, got {other:?}"),
        }
    }

    /// A supersede settles the alarm: the retired record stops counting, the
    /// survivor becomes the fact — and the retired one is still in the file,
    /// still readable, because the correction's history is part of the record.
    #[test]
    fn a_supersede_retires_the_loser_and_leaves_it_walkable() {
        let wrong = verdict("sha-a", "3E99");
        let right = verdict("sha-b", "3E12");
        let text = format!(
            "{}\n{}\n{}\n",
            to_line(&Record::Verdict(Box::new(wrong.clone()))).expect("ser"),
            to_line(&Record::Verdict(Box::new(right.clone()))).expect("ser"),
            to_line(&Record::Supersede(Supersede {
                retires: wrong.id(),
                reason: "recorded against a patched build; see #61".to_string(),
            }))
            .expect("ser"),
        );
        let corpus = Corpus::parse(&text).expect("parse");
        match corpus.resolve(&key()) {
            Some(Resolution::Fact { outcome, .. }) => {
                assert_eq!(outcome.bytes(), Some(vec![0x3E, 0x12]));
            }
            other => panic!("expected the survivor to be the fact, got {other:?}"),
        }
        assert!(
            corpus.verdicts().any(|v| *v == wrong),
            "the retired record stays in the file"
        );
        assert_eq!(corpus.retired(), vec![wrong.id()]);
    }

    /// A tracked divergence keeps its join id, so the in-repo half of the pair
    /// can be found. Losing the tag would turn a known difference back into an
    /// unexplained one.
    #[test]
    fn a_divergence_keeps_its_join_id() {
        let mut v = verdict("sha-a", "3E12");
        v.outcome = Outcome::Divergence {
            divergence: "issue-36-truncation".to_string(),
            hex: "3EFF".to_string(),
        };
        let line = to_line(&Record::Verdict(Box::new(v.clone()))).expect("ser");
        let corpus = Corpus::parse(&line).expect("parse");
        assert_eq!(corpus.verdicts().next(), Some(&v));
        assert_eq!(v.outcome.bytes(), Some(vec![0x3E, 0xFF]));
    }

    /// A deliberate rejection is a verdict; it carries the diagnostic that makes
    /// it attributable to the source rather than to the environment.
    #[test]
    fn a_rejection_is_a_verdict_and_keeps_its_diagnostic() {
        let mut v = verdict("sha-a", "");
        v.outcome = Outcome::Rejected {
            diagnostic: "value out of range".to_string(),
        };
        let corpus =
            Corpus::parse(&to_line(&Record::Verdict(Box::new(v))).expect("ser")).expect("parse");
        match corpus.resolve(&key()) {
            Some(Resolution::Fact { outcome, .. }) => assert!(matches!(
                outcome,
                Outcome::Rejected { diagnostic } if diagnostic == "value out of range"
            )),
            other => panic!("expected a rejection fact, got {other:?}"),
        }
    }

    /// An empty corpus is an ordinary state — a CPU nobody has recorded yet —
    /// and so is a file that does not exist. Neither is an error.
    #[test]
    fn an_empty_corpus_is_not_an_error() {
        assert_eq!(Corpus::parse("").expect("parse").verdicts().count(), 0);
        assert_eq!(Corpus::parse("\n\n").expect("parse").verdicts().count(), 0);
        let missing = Path::new("/nonexistent/verdicts/nothing-here.ndjson");
        assert_eq!(Corpus::read(missing).expect("read").verdicts().count(), 0);
    }

    /// Corruption is not evolution. An unknown *kind* is skipped, but a line
    /// that is not JSON means the file is damaged, and saying so with a line
    /// number beats replaying a corpus that is missing facts.
    #[test]
    fn a_corrupt_line_reports_where() {
        let err = Corpus::parse("{\"t\":\"verdict\"}\nnot json at all\n").expect_err("corrupt");
        assert_eq!(err.line, 2);
        assert!(err.to_string().starts_with("corpus line 2:"), "{err}");
    }

    /// Nothing recorded for a case means no verdict — distinct from a recorded
    /// rejection, and the reason replay can tell "we never checked this" from
    /// "the tool refused it".
    #[test]
    fn an_unrecorded_case_resolves_to_nothing() {
        let corpus =
            Corpus::parse(&to_line(&Record::Verdict(Box::new(verdict("a", "3E12")))).expect("ser"))
                .expect("parse");
        let unseen = Key {
            identity: "1.42 Beta [Bld 250]".to_string(),
            source: " nop\n".to_string(),
        };
        assert_eq!(corpus.resolve(&unseen), None);
    }

    /// Hex is the storage form because a diff of it is readable. Round-tripping
    /// it must be exact, including the empty case.
    #[test]
    fn hex_round_trips_including_empty() {
        for bytes in [vec![], vec![0x00], vec![0x3E, 0x12, 0xFF]] {
            assert_eq!(decode_hex(&encode_hex(&bytes)), Some(bytes));
        }
        assert_eq!(decode_hex("3E1"), None, "odd length is not hex");
        assert_eq!(decode_hex("3EZZ"), None, "non-hex digits rejected");
    }

    /// Appending is the only write, and it must not disturb what is already
    /// there — the file is evidence, not a cache.
    #[test]
    fn appending_preserves_what_was_already_recorded() {
        let dir = std::env::temp_dir().join("asm198x-verdict-append");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("8080.ndjson");

        append(&path, &[Record::Verdict(Box::new(verdict("a", "3E12")))]).expect("first");
        append(&path, &[Record::Verdict(Box::new(verdict("b", "3E12")))]).expect("second");

        let corpus = Corpus::read(&path).expect("read");
        assert_eq!(corpus.verdicts().count(), 2);
        match corpus.resolve(&key()) {
            Some(Resolution::Fact { corroborations, .. }) => {
                assert_eq!(corroborations, vec!["a", "b"]);
            }
            other => panic!("expected a corroborated fact, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
