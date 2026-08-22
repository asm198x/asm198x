//! Who arbitrated — the reference tool's behavioural identity and the binary
//! that produced it.
//!
//! Every reference-arbitrated suite has so far asked one question about its
//! tools: *is it there?* That is enough to decide whether to run. It is not
//! enough to record what the run proved, because a fact is only worth as much
//! as the account of who established it (#61, R1).
//!
//! Two things are captured, and the difference between them is the design:
//!
//! - **Behavioural identity** — the tool's own version self-report. This is
//!   what predicts behaviour, so it is what verdicts are keyed on.
//! - **Binary digest** — SHA-256 of the executable that actually ran. Carried
//!   as provenance and never part of the key, so two builds of one release
//!   *corroborate* a fact rather than forking it, while two binaries claiming
//!   one version and producing different bytes raise an alarm that can be
//!   traced to the specific binaries.
//!
//! # Why the self-report cannot be guessed
//!
//! The tools do not agree on any of it, so this table is probed, not assumed:
//!
//! | tool | flag | where the version is |
//! |---|---|---|
//! | `acme`, `ca65`, `ld65`, `lwasm`, `rgbasm`, `rgblink`, `sjasmplus` | `--version` | first line |
//! | `pasmo`, `vasmm68k_mot` | `-v` | first line |
//! | `asl`, `p2bin` | `-v` | **second** line, after a "no input files" error |
//!
//! `asl` and `p2bin` exit non-zero while reporting their version, so a
//! successful exit cannot be required either. Rather than trust a line number,
//! each tool names a marker its identity line must contain — a version printed
//! one line further down is still found, and an unrecognisable response is
//! reported as such instead of silently keying every verdict on an error
//! message.
//!
//! # What probing already found
//!
//! The binary installed here as `pasmo` reports **`PasmoNext v0.1.3 … Modified
//! by C Kirby`** — a fork, not stock pasmo. Presence-only gating could never
//! have shown that, and every pasmo verdict recorded on this machine is a
//! verdict about the fork. That is precisely the confusion behavioural identity
//! exists to prevent.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{LazyLock, Mutex};

use sha2::{Digest, Sha256};

/// A reference tool, as it was when it arbitrated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolIdentity {
    /// The executable name, as invoked.
    pub tool: String,
    /// The tool's own version self-report — the key.
    pub identity: String,
    /// SHA-256 of the executable that ran, lower-case hex — provenance.
    pub digest: String,
    /// Where the executable was found.
    pub path: PathBuf,
}

/// How to ask one tool what it is: the flag, and a marker its answer must
/// contain. The marker is what makes this robust to the version moving between
/// lines, and what stops an unrecognised response being recorded as an identity.
struct Probe {
    flag: &'static str,
    marker: &'static str,
}

/// The probe table. Every entry was established by running the tool, not by
/// reading its documentation — several disagree with what `--version` would
/// suggest.
fn probe_for(tool: &str) -> Option<Probe> {
    let probe = match tool {
        "acme" => Probe {
            flag: "--version",
            marker: "ACME",
        },
        "ca65" => Probe {
            flag: "--version",
            marker: "ca65",
        },
        "ld65" => Probe {
            flag: "--version",
            marker: "ld65",
        },
        "lwasm" => Probe {
            flag: "--version",
            marker: "lwasm",
        },
        "rgbasm" => Probe {
            flag: "--version",
            marker: "rgbasm",
        },
        "rgblink" => Probe {
            flag: "--version",
            marker: "rgblink",
        },
        "sjasmplus" => Probe {
            flag: "--version",
            marker: "SjASMPlus",
        },
        // Reports on `-v`, and only after refusing to run for want of input.
        "pasmo" => Probe {
            flag: "-v",
            marker: "asmo",
        },
        "vasmm68k_mot" => Probe {
            flag: "-v",
            marker: "vasm",
        },
        "asl" => Probe {
            flag: "-v",
            marker: "Macro Assembler",
        },
        "p2bin" => Probe {
            flag: "-v",
            marker: "P2BIN",
        },
        _ => return None,
    };
    Some(probe)
}

/// Every identity captured this process, so a suite running thousands of cases
/// asks each tool exactly once.
static CACHE: LazyLock<Mutex<HashMap<String, Option<ToolIdentity>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Identify `tool`, or `None` if it is absent, unknown to the probe table, or
/// answers unrecognisably.
///
/// Memoized per process: the first call runs the tool and hashes its binary,
/// and every later call is a map lookup. Without that, a sweep would re-probe
/// once per case for no new information.
pub fn identify(tool: &str) -> Option<ToolIdentity> {
    if let Ok(cache) = CACHE.lock()
        && let Some(hit) = cache.get(tool)
    {
        return hit.clone();
    }
    let captured = capture(tool);
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(tool.to_string(), captured.clone());
    }
    captured
}

/// Run the probe and hash the binary. The uncached path.
fn capture(tool: &str) -> Option<ToolIdentity> {
    let probe = probe_for(tool)?;
    let path = resolve(tool)?;
    // Exit status is ignored on purpose: `asl` and `p2bin` report their version
    // while exiting non-zero over the missing input file.
    let out = Command::new(&path).arg(probe.flag).output().ok()?;
    let identity = first_line_containing(&out, probe.marker)?;
    Some(ToolIdentity {
        tool: tool.to_string(),
        identity,
        digest: digest_of(&path)?,
        path,
    })
}

/// The first line of the tool's output — either stream — carrying `marker`.
fn first_line_containing(out: &std::process::Output, marker: &str) -> Option<String> {
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    stdout
        .lines()
        .chain(stderr.lines())
        .find(|l| l.contains(marker))
        .map(|l| l.trim().to_string())
}

/// Lower-case hex, which is how every digest in the corpus is written.
///
/// sha2 0.11 returns a `hybrid_array::Array` rather than a `GenericArray`, and
/// that type implements no `LowerHex`, so `format!("{:x}", …)` no longer
/// compiles. `verdict_corpus::encode_hex` is the wrong replacement: it emits
/// **upper**-case, for the byte payloads a verdict carries. Swapping the case
/// of a recorded digest would change every `Verdict::id` and leave the corpus
/// mixed, so the case is part of the format rather than a detail.
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// SHA-256 of a file, lower-case hex.
fn digest_of(path: &PathBuf) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(hex_lower(hasher.finalize().as_slice()))
}

/// Find `tool` on `PATH`, so the binary can be hashed rather than merely run.
/// An absolute or relative path is taken as given.
fn resolve(tool: &str) -> Option<PathBuf> {
    if tool.contains('/') {
        let direct = PathBuf::from(tool);
        return direct.is_file().then_some(direct);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(tool))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unknown tool is not identified — better than inventing an identity
    /// from whatever it happened to print.
    #[test]
    fn a_tool_outside_the_probe_table_is_not_identified() {
        assert_eq!(identify("definitely-not-a-reference-assembler"), None);
    }

    /// A tool that is not installed is not identified, which is the ordinary
    /// state on a machine without the references.
    #[test]
    fn an_absent_tool_is_not_identified() {
        assert!(probe_for("acme").is_some(), "acme is in the table");
        if resolve("acme").is_none() {
            assert_eq!(identify("acme"), None);
        }
    }

    /// The marker search reads both streams and finds a version that is not on
    /// the first line — the shape `asl` and `p2bin` actually produce.
    #[test]
    fn the_marker_finds_a_version_below_the_first_line() {
        let out = std::process::Output {
            status: std::process::Command::new("true")
                .status()
                .expect("run true"),
            stdout: b"asl: no input files (-help for help)\nMacro Assembler 1.42 Beta [Bld 309]\n"
                .to_vec(),
            stderr: Vec::new(),
        };
        assert_eq!(
            first_line_containing(&out, "Macro Assembler").as_deref(),
            Some("Macro Assembler 1.42 Beta [Bld 309]")
        );
        assert_eq!(first_line_containing(&out, "nothing here"), None);
    }

    /// Resolution finds a real binary on PATH so it can be hashed, and hashing
    /// it is deterministic — the same file always digests the same.
    #[test]
    fn a_resolved_binary_digests_deterministically() {
        let Some(path) = resolve("sh") else {
            return; // No `sh`: nothing to assert, and not this test's business.
        };
        let once = digest_of(&path).expect("digest");
        assert_eq!(once, digest_of(&path).expect("digest"), "digest is stable");
        assert_eq!(once.len(), 64, "SHA-256 renders as 64 hex characters");
        assert!(once.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// The digest is of file *content*, so two paths holding the same bytes
    /// agree — a repackaged arbiter corroborates rather than forks.
    #[test]
    fn identical_bytes_at_two_paths_digest_the_same() {
        let dir = std::env::temp_dir().join("asm198x-identity-digest");
        let _ = std::fs::create_dir_all(&dir);
        let (a, b) = (dir.join("one"), dir.join("two"));
        std::fs::write(&a, b"same bytes").expect("write");
        std::fs::write(&b, b"same bytes").expect("write");
        assert_eq!(digest_of(&a), digest_of(&b));
        std::fs::write(&b, b"other bytes").expect("write");
        assert_ne!(digest_of(&a), digest_of(&b));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Known-answer check against the SHA-256 test vector for the empty input,
    /// so a wrong hash cannot pass as a plausible-looking one.
    #[test]
    fn the_digest_matches_the_published_sha256_vector() {
        let dir = std::env::temp_dir().join("asm198x-identity-vector");
        let _ = std::fs::create_dir_all(&dir);
        let empty = dir.join("empty");
        std::fs::write(&empty, b"").expect("write");
        assert_eq!(
            digest_of(&empty).as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Identification is memoized, so a sweep of thousands of cases pays for it
    /// once. Observable through the cache rather than by timing.
    #[test]
    fn identification_is_captured_once_per_process() {
        let tool = "sjasmplus";
        let first = identify(tool);
        assert!(
            CACHE.lock().expect("cache").contains_key(tool),
            "the answer is cached, present or not"
        );
        assert_eq!(first, identify(tool), "the cached answer is reused");
    }
}
