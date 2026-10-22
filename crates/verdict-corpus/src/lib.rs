//! Recorded reference-assembler verdicts — the byte-identical guarantee,
//! provable without the reference tools.
//!
//! Asm198x's central claim is that its output is byte-identical to the real
//! assembler for the dialect. That claim is arbitrated by shelling out to
//! `acme`, `ca65`, `asl`, `pasmo`, `sjasmplus`, `vasm`, `lwasm` and `rgbasm`,
//! so it can only be checked where they are installed: the arbiter image. CI
//! installs none of them, so an outside contributor could prove nothing in a
//! pull request.
//!
//! A verdict is one recorded fact: *given this source text, the reference
//! produced these bytes* (or rejected it, and why). Recorded in **live mode**
//! where the tools exist, committed, and **replayed** by tests that need no
//! tools at all.
//!
//! # What a verdict is not
//!
//! It is not an expectation someone wrote down. Every record is the observed
//! output of a real tool run. That is the difference between a corpus and a
//! pile of goldens: a golden says what we think should happen, a verdict says
//! what did.
//!
//! # One verdict per case, from the current tools
//!
//! The corpus answers one question: *what do the reference tools in use now
//! do?* Each [`Key`] (suite, case, tool, dialect, source text) holds exactly
//! one verdict. Recording a case again **replaces** its verdict, so moving to a new
//! tool version and re-running growth regenerates the corpus, and the git diff
//! is the review: each changed line is either a bug of ours the old version
//! hid, or the reference changing behaviour.
//!
//! The tool's version self-report travels with each verdict as
//! [`Arbiter::identity`], so a reader can see what answered and growth can
//! prune verdicts no current tool re-recorded. It is not part of the key.
//! History is git's job, not the file's.
//!
//! (Until October 2026 verdicts were keyed on the version string, carried a
//! binary digest, and were append-only, with supersede records settling
//! conflicts. `decisions/verdicts-track-current-tools.md` records why that
//! went.)
//!
//! # On-disk shape
//!
//! One NDJSON file per CPU, one JSON object per line, in canonical order: sorted
//! by key and written whole, so the same facts always produce the same file and
//! a diff shows only what moved. A line whose `t` is not recognised is **kept,
//! not fatal**, so a corpus written by a newer producer still replays on an
//! older checkout.
//!
//! Reference bytes are stored as uppercase hex rather than a digest, so a
//! divergence is legible in a diff without any tool: you can see which byte
//! moved.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Which suite produced a verdict. Kept as a closed set so a typo cannot
/// silently create a category nothing ever replays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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

/// The tool that produced a verdict.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Arbiter {
    /// The executable's name, as invoked (`"asl"`, `"ca65"`).
    pub tool: String,
    /// The tool's own version self-report (`"rgbasm v1.0.3"`), recorded so a
    /// reader can see what answered and growth can tell a verdict from a tool
    /// version no longer in use. Not part of the key.
    pub identity: String,
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
    /// Part of the key: two cases can share source text (an alias and its
    /// mnemonic, a form and a one-instruction sweep chunk) and each is counted.
    pub case: String,
    /// The exact source text the arbiter was given. Stored verbatim rather than
    /// hashed: a key you can read is a key you can debug.
    pub source: String,
    /// Who produced the fact.
    pub arbiter: Arbiter,
    /// What they did with it.
    #[serde(flatten)]
    pub outcome: Outcome,
}

impl Verdict {
    /// The case this verdict answers. Recording the same case again replaces
    /// the verdict rather than adding a second one.
    #[must_use]
    pub fn key(&self) -> Key {
        Key {
            suite: self.suite,
            case: self.case.clone(),
            tool: self.arbiter.tool.clone(),
            dialect: self.dialect.clone(),
            source: self.source.clone(),
        }
    }
}

/// What identifies a case. Field order is the corpus's canonical line order,
/// so a file groups by suite and then reads in case order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    /// The suite that names the case.
    pub suite: Suite,
    /// The case's label.
    pub case: String,
    /// The reference tool that answers it.
    pub tool: String,
    /// The dialect the source is written in. One tool can serve two dialects
    /// (`rgbasm` and `rgbasm-banked` probes), and the same text in each is a
    /// different case.
    pub dialect: String,
    /// The source text it was given.
    pub source: String,
}

/// A parsed corpus file: one verdict per [`Key`], plus any lines this build
/// does not understand, kept so they survive a rewrite.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Corpus {
    verdicts: BTreeMap<Key, Verdict>,
    unknown: Vec<serde_json::Value>,
}

impl Corpus {
    /// Parse NDJSON. A blank line is skipped; a line that is not JSON at all is
    /// an error, because that is corruption rather than evolution. When two
    /// lines answer the same case (a merge artefact) the later one wins, and
    /// [`Corpus::render`] no longer matches the file, which `xtask corpus
    /// --check` reports.
    ///
    /// # Errors
    ///
    /// Returns the 1-based line number and message of the first unparseable line.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut corpus = Self::default();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(line).map_err(|e| ParseError {
                line: i + 1,
                message: e.to_string(),
            })?;
            match value.get("t").and_then(serde_json::Value::as_str) {
                // A recognised kind whose payload this build cannot read is
                // kept as unknown too: refusing the whole file over one record
                // is the failure this policy exists to avoid.
                Some("verdict") => match serde_json::from_value::<Verdict>(value.clone()) {
                    Ok(verdict) => {
                        corpus.insert(verdict);
                    }
                    Err(_) => corpus.unknown.push(value),
                },
                _ => corpus.unknown.push(value),
            }
        }
        Ok(corpus)
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

    /// Every verdict, in canonical order.
    pub fn verdicts(&self) -> impl Iterator<Item = &Verdict> {
        self.verdicts.values()
    }

    /// How many cases the corpus answers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.verdicts.len()
    }

    /// Whether the corpus answers nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.verdicts.is_empty()
    }

    /// The verdict for one case, if it has been recorded.
    #[must_use]
    pub fn get(&self, key: &Key) -> Option<&Verdict> {
        self.verdicts.get(key)
    }

    /// Lines this build does not understand, in the order they were read.
    #[must_use]
    pub fn unknown(&self) -> &[serde_json::Value] {
        &self.unknown
    }

    /// Record a verdict, replacing any earlier verdict for the same case.
    /// Returns the verdict it replaced.
    pub fn insert(&mut self, verdict: Verdict) -> Option<Verdict> {
        self.verdicts.insert(verdict.key(), verdict)
    }

    /// Keep only the verdicts `keep` accepts. Returns how many were removed.
    pub fn retain(&mut self, mut keep: impl FnMut(&Verdict) -> bool) -> usize {
        let before = self.verdicts.len();
        self.verdicts.retain(|_, v| keep(v));
        before - self.verdicts.len()
    }

    /// The canonical file text: one line per verdict in key order, then any
    /// unknown lines in the order they were read. A pure function of content,
    /// so recording unchanged facts rewrites an identical file.
    ///
    /// # Errors
    ///
    /// Returns a serialization failure.
    pub fn render(&self) -> Result<String, serde_json::Error> {
        let mut out = String::new();
        for verdict in self.verdicts.values() {
            out.push_str(&to_line(verdict)?);
            out.push('\n');
        }
        for value in &self.unknown {
            out.push_str(&serde_json::to_string(value)?);
            out.push('\n');
        }
        Ok(out)
    }

    /// Write the canonical file, creating its directory if needed. An empty
    /// corpus with nothing unknown removes the file instead, so a CPU whose
    /// last verdict was pruned leaves nothing behind.
    ///
    /// # Errors
    ///
    /// Returns any I/O or serialization failure.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if self.verdicts.is_empty() && self.unknown.is_empty() {
            return match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = self.render().map_err(std::io::Error::other)?;
        // Written to a sibling and renamed, so an interrupted run leaves the old
        // file or the new one, never half of either. The sibling's name is
        // unique to this write, so two writers can never rename each other's
        // half-written file into place.
        static WRITES: AtomicU64 = AtomicU64::new(0);
        let n = WRITES.fetch_add(1, Ordering::Relaxed);
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(".{}.{n}.tmp", std::process::id()));
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }
}

/// Run `f` holding the exclusive lock for `path`: a process-wide mutex, so
/// threads in one test binary queue, and an OS file lock, so separate
/// processes do. Every read-modify-write of a corpus file goes through here;
/// without it, two live tests flushing the same CPU file at once would each
/// rewrite it from the state before the other's verdicts.
///
/// The lock file lives in the temporary directory, named for the path, so it
/// never appears beside the corpus to be committed.
///
/// # Errors
///
/// Returns a failure to take the lock, or whatever `f` returns.
pub fn locked<T>(path: &Path, f: impl FnOnce() -> std::io::Result<T>) -> std::io::Result<T> {
    static IN_PROCESS: Mutex<()> = Mutex::new(());
    let _guard = IN_PROCESS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let absolute = std::path::absolute(path)?;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in absolute.as_os_str().as_encoded_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let lock_path = std::env::temp_dir().join(format!("asm198x-verdicts-{h:016x}.lock"));
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;
    lock.lock()?;
    let result = f();
    let _ = lock.unlock();
    result
}

/// Record verdicts into a CPU's corpus file: each replaces any earlier verdict
/// for the same case, and the file is rewritten in canonical order. Returns how
/// many verdicts were new or changed.
///
/// # Errors
///
/// Returns any read, parse, I/O or serialization failure.
pub fn merge(path: &Path, verdicts: impl IntoIterator<Item = Verdict>) -> std::io::Result<usize> {
    locked(path, || {
        let mut corpus = Corpus::read(path).map_err(std::io::Error::other)?;
        let mut changed = 0;
        for verdict in verdicts {
            let fresh = verdict.clone();
            if corpus.insert(verdict).as_ref() != Some(&fresh) {
                changed += 1;
            }
        }
        if changed > 0 {
            corpus.write(path)?;
        }
        Ok(changed)
    })
}

/// The environment variable naming a growth run's manifest. When it is set,
/// every recorder flush also notes what it recorded there.
pub const MANIFEST_ENV: &str = "ASM198X_GROWTH_MANIFEST";

/// What one growth run recorded into one corpus file, for one suite and tool.
/// Growth prunes only what a run actually reached, so this is its evidence.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Recorded {
    /// The corpus file's name (`"z80.ndjson"`).
    pub file: String,
    /// The suite that recorded.
    pub suite: Suite,
    /// The tool that answered.
    pub tool: String,
    /// The version line it reported.
    pub identity: String,
    /// For a curriculum verdict, its key (`path#variant@digest`), so a file
    /// re-recorded under a new digest can retire the verdict for the old one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Recorded {
    /// The manifest entries for verdicts flushed into `file`: one per suite and
    /// tool, plus one per curriculum verdict.
    pub fn of<'a>(file: &str, verdicts: impl IntoIterator<Item = &'a Verdict>) -> Vec<Self> {
        let mut out: Vec<Self> = verdicts
            .into_iter()
            .map(|v| Self {
                file: file.to_string(),
                suite: v.suite,
                tool: v.arbiter.tool.clone(),
                identity: v.arbiter.identity.clone(),
                source: (v.suite == Suite::Curriculum).then(|| v.source.clone()),
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// Append entries to a growth manifest, under its lock.
///
/// # Errors
///
/// Returns any I/O or serialization failure.
pub fn append_manifest(path: &Path, entries: &[Recorded]) -> std::io::Result<()> {
    use std::io::Write as _;
    locked(path, || {
        let mut text = String::new();
        for entry in entries {
            text.push_str(&serde_json::to_string(entry).map_err(std::io::Error::other)?);
            text.push('\n');
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(text.as_bytes())
    })
}

/// Read a growth manifest. A missing file is an empty manifest.
///
/// # Errors
///
/// Returns a read failure or a line that does not parse.
pub fn read_manifest(path: &Path) -> Result<Vec<Recorded>, ParseError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(ParseError {
                line: 0,
                message: e.to_string(),
            });
        }
    };
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line).map_err(|e| ParseError {
                line: i + 1,
                message: e.to_string(),
            })
        })
        .collect()
}

/// Serialize one verdict to its NDJSON line.
///
/// Key order is `serde_json`'s, which sorts, so `t` lands alphabetically rather
/// than first. That is the order the corpus wants: a line's bytes depend only on
/// its content, so re-recording an unchanged fact produces an identical line.
///
/// # Errors
///
/// Returns a serialization failure.
pub fn to_line(verdict: &Verdict) -> Result<String, serde_json::Error> {
    let mut value = serde_json::to_value(verdict)?;
    if let Some(map) = value.as_object_mut() {
        map.insert("t".to_string(), serde_json::Value::String("verdict".into()));
    }
    serde_json::to_string(&value)
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

pub mod curriculum;
pub mod derive;
pub mod files;

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(identity: &str, hex: &str) -> Verdict {
        Verdict {
            suite: Suite::Form,
            cpu: "8080".to_string(),
            dialect: "asl".to_string(),
            case: "mvi a,imm".to_string(),
            source: " mvi a,12h\n".to_string(),
            arbiter: Arbiter {
                tool: "asl".to_string(),
                identity: identity.to_string(),
            },
            outcome: Outcome::Bytes {
                hex: hex.to_string(),
            },
        }
    }

    fn line(v: &Verdict) -> String {
        to_line(v).expect("serialize")
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("asm198x-verdicts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("8080.ndjson")
    }

    /// Every field survives the write/read round trip, and serialization is a
    /// pure function of content.
    #[test]
    fn a_verdict_round_trips_every_field() {
        let original = verdict("1.42 Beta [Bld 309]", "3E12");
        let corpus = Corpus::parse(&line(&original)).expect("parse");
        assert_eq!(corpus.verdicts().collect::<Vec<_>>(), vec![&original]);
        assert!(line(&original).contains(r#""t":"verdict""#));
        assert_eq!(line(&original), line(&original.clone()));
    }

    /// Recording a case again replaces its verdict: one case, one answer, from
    /// whichever tool answered last.
    #[test]
    fn recording_a_case_again_replaces_it() {
        let mut corpus = Corpus::default();
        assert_eq!(corpus.insert(verdict("old", "3E12")), None);
        let replaced = corpus.insert(verdict("new", "3E99"));
        assert_eq!(
            replaced.map(|v| v.arbiter.identity),
            Some("old".to_string())
        );
        assert_eq!(corpus.len(), 1);
        let only = corpus.verdicts().next().expect("one verdict");
        assert_eq!(only.arbiter.identity, "new");
        assert_eq!(only.outcome.bytes(), Some(vec![0x3E, 0x99]));
    }

    /// Two cases sharing source text (an alias and its mnemonic) are two
    /// verdicts, because coverage counts each.
    #[test]
    fn cases_sharing_source_are_counted_separately() {
        let mut alias = verdict("v", "3E12");
        alias.case = "alias of mvi".to_string();
        let mut corpus = Corpus::default();
        corpus.insert(verdict("v", "3E12"));
        corpus.insert(alias);
        assert_eq!(corpus.len(), 2);
    }

    /// When a merge leaves two lines for one case, the later wins, and the file
    /// is visibly not canonical.
    #[test]
    fn a_doubled_case_keeps_the_later_line_and_is_not_canonical() {
        let text = format!(
            "{}\n{}\n",
            line(&verdict("a", "3E12")),
            line(&verdict("b", "3E99"))
        );
        let corpus = Corpus::parse(&text).expect("parse");
        assert_eq!(corpus.len(), 1);
        assert_eq!(
            corpus
                .verdicts()
                .next()
                .map(|v| v.arbiter.identity.as_str()),
            Some("b")
        );
        assert_ne!(corpus.render().expect("render"), text);
    }

    /// The canonical file is sorted by key, whatever order verdicts arrived in,
    /// so the same facts always produce the same bytes.
    #[test]
    fn rendering_is_canonical_whatever_the_insertion_order() {
        let mut a = verdict("v", "00");
        a.case = "a".to_string();
        let mut b = verdict("v", "01");
        b.case = "b".to_string();
        let mut forward = Corpus::default();
        forward.insert(a.clone());
        forward.insert(b.clone());
        let mut backward = Corpus::default();
        backward.insert(b);
        backward.insert(a);
        let text = forward.render().expect("render");
        assert_eq!(text, backward.render().expect("render"));
        assert_eq!(
            Corpus::parse(&text)
                .expect("parse")
                .render()
                .expect("render"),
            text
        );
    }

    /// A record kind this build does not know is kept through a rewrite, and the
    /// verdicts beside it still read.
    #[test]
    fn an_unknown_record_kind_survives_a_rewrite() {
        let note = r#"{"note":"from a later build","t":"provenance-note"}"#;
        let text = format!("{note}\n{}\n", line(&verdict("v", "3E12")));
        let corpus = Corpus::parse(&text).expect("parse");
        assert_eq!(corpus.len(), 1);
        assert_eq!(corpus.unknown().len(), 1);
        assert!(
            corpus
                .render()
                .expect("render")
                .ends_with(&format!("{note}\n"))
        );
    }

    /// A verdict line from the old format, with a binary digest, still reads.
    #[test]
    fn an_old_format_line_with_a_digest_still_reads() {
        let old = r#"{"arbiter":{"digest":"d589","identity":"Macro Assembler 1.42 Beta [Bld 309]","tool":"asl"},"case":"LDN 1","cpu":"1802","dialect":"asl","hex":"01","outcome":"bytes","source":"\tldn 1\n","suite":"form","t":"verdict"}"#;
        let corpus = Corpus::parse(old).expect("parse");
        let v = corpus.verdicts().next().expect("reads as a verdict");
        assert_eq!(v.arbiter.identity, "Macro Assembler 1.42 Beta [Bld 309]");
        assert!(
            !corpus.render().expect("render").contains("digest"),
            "the digest is not written back"
        );
    }

    /// A tracked divergence keeps its join id.
    #[test]
    fn a_divergence_keeps_its_join_id() {
        let mut v = verdict("v", "3E12");
        v.outcome = Outcome::Divergence {
            divergence: "issue-36-truncation".to_string(),
            hex: "3EFF".to_string(),
        };
        let corpus = Corpus::parse(&line(&v)).expect("parse");
        assert_eq!(corpus.verdicts().next(), Some(&v));
        assert_eq!(v.outcome.bytes(), Some(vec![0x3E, 0xFF]));
    }

    /// A deliberate rejection is a verdict and keeps its diagnostic.
    #[test]
    fn a_rejection_is_a_verdict_and_keeps_its_diagnostic() {
        let mut v = verdict("v", "");
        v.outcome = Outcome::Rejected {
            diagnostic: "value out of range".to_string(),
        };
        let corpus = Corpus::parse(&line(&v)).expect("parse");
        assert!(matches!(
            &corpus.get(&v.key()).expect("recorded").outcome,
            Outcome::Rejected { diagnostic } if diagnostic == "value out of range"
        ));
    }

    /// Empty text and a missing file are both an empty corpus, not an error.
    #[test]
    fn an_empty_corpus_is_not_an_error() {
        assert!(Corpus::parse("").expect("parse").is_empty());
        assert!(Corpus::parse("\n\n").expect("parse").is_empty());
        let missing = Path::new("/nonexistent/verdicts/nothing-here.ndjson");
        assert!(Corpus::read(missing).expect("read").is_empty());
    }

    /// Corruption is not evolution: a line that is not JSON is an error naming
    /// its line.
    #[test]
    fn a_corrupt_line_reports_where() {
        let err = Corpus::parse("{\"t\":\"verdict\"}\nnot json at all\n").expect_err("corrupt");
        assert_eq!(err.line, 2);
        assert!(err.to_string().starts_with("corpus line 2:"), "{err}");
    }

    /// An unrecorded case resolves to nothing: "we never checked this" is not
    /// "the tool refused it".
    #[test]
    fn an_unrecorded_case_resolves_to_nothing() {
        let corpus = Corpus::parse(&line(&verdict("v", "3E12"))).expect("parse");
        let mut unseen = verdict("v", "3E12").key();
        unseen.source = " nop\n".to_string();
        assert_eq!(corpus.get(&unseen), None);
    }

    /// Hex round-trips exactly, including the empty case.
    #[test]
    fn hex_round_trips_including_empty() {
        for bytes in [vec![], vec![0x00], vec![0x3E, 0x12, 0xFF]] {
            assert_eq!(decode_hex(&encode_hex(&bytes)), Some(bytes));
        }
        assert_eq!(decode_hex("3E1"), None, "odd length is not hex");
        assert_eq!(decode_hex("3EZZ"), None, "non-hex digits rejected");
    }

    /// Merging counts only what is new or changed, and re-merging the same facts
    /// changes nothing on disk.
    #[test]
    fn merging_counts_changes_and_is_idempotent() {
        let path = temp_path("merge");
        assert_eq!(merge(&path, [verdict("v1", "3E12")]).expect("first"), 1);
        let first = std::fs::read_to_string(&path).expect("written");
        assert_eq!(merge(&path, [verdict("v1", "3E12")]).expect("again"), 0);
        assert_eq!(std::fs::read_to_string(&path).expect("unchanged"), first);
        assert_eq!(
            merge(&path, [verdict("v2", "3E12")]).expect("new version"),
            1
        );
        let corpus = Corpus::read(&path).expect("read");
        assert_eq!(corpus.len(), 1);
        assert_eq!(
            corpus
                .verdicts()
                .next()
                .map(|v| v.arbiter.identity.as_str()),
            Some("v2")
        );
        let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
    }

    /// Concurrent merges into one file lose nothing. Live tests run in parallel
    /// threads and several flush the same CPU file; this is the reason merges
    /// take a lock.
    #[test]
    fn concurrent_merges_into_one_file_lose_nothing() {
        let path = temp_path("concurrent");
        let threads: Vec<_> = (0..4)
            .map(|t| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for batch in 0..25 {
                        let verdicts = (0..10).map(|i| {
                            let mut v = verdict("v", "00");
                            v.case = format!("t{t} b{batch} i{i}");
                            v
                        });
                        merge(&path, verdicts).expect("merge");
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().expect("thread");
        }
        assert_eq!(Corpus::read(&path).expect("read").len(), 4 * 25 * 10);
        let dir = path.parent().expect("dir");
        let leftovers: Vec<_> = std::fs::read_dir(dir)
            .expect("dir")
            .flatten()
            .filter(|e| e.path() != path)
            .collect();
        assert!(leftovers.is_empty(), "no temporary files are left behind");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The same text in two dialects of one tool is two cases.
    #[test]
    fn the_same_text_in_two_dialects_is_two_cases() {
        let mut banked = verdict("v", "3E12");
        banked.dialect = "asl-banked".to_string();
        let mut corpus = Corpus::default();
        corpus.insert(verdict("v", "3E12"));
        corpus.insert(banked);
        assert_eq!(corpus.len(), 2);
    }

    /// A manifest round-trips, and lists each suite and tool once.
    #[test]
    fn a_manifest_names_each_group_once() {
        let path = temp_path("manifest");
        std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
        let entries = Recorded::of("8080.ndjson", &[verdict("v", "00"), verdict("v", "01")]);
        assert_eq!(entries.len(), 1);
        append_manifest(&path, &entries).expect("append");
        append_manifest(&path, &entries).expect("append again");
        assert_eq!(read_manifest(&path).expect("read").len(), 2);
        let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
    }

    /// Pruning the last verdict removes the file rather than leaving it empty.
    #[test]
    fn an_emptied_corpus_removes_its_file() {
        let path = temp_path("prune");
        merge(&path, [verdict("old", "3E12")]).expect("write");
        let mut corpus = Corpus::read(&path).expect("read");
        assert_eq!(corpus.retain(|v| v.arbiter.identity != "old"), 1);
        corpus.write(&path).expect("write");
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
    }
}
