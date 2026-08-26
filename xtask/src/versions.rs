//! Check that every reference-version claim in the tree names a version this
//! project has actually observed.
//!
//! Provenance rots quietly. A comment saying "measured against ca65 2.19" reads
//! as evidence, and stays readable long after the tool it names has gone — or,
//! in that case, long after anyone noticed the binary never reported 2.19 at
//! all. Nothing catches it, because nothing else in the build reads prose.
//!
//! The verdict corpus does record what was observed: every line carries the
//! `identity` string the tool printed when it answered. That makes it the one
//! place in the repo that cannot claim a version it did not see, and this check
//! measures the prose against it.
//!
//! A claim that matches nothing recorded is not automatically wrong — a tool
//! upgraded since the measurement leaves a true claim behind it. Those go in
//! `decisions/reference-versions.md` with the reason they stand. What the check
//! stops is the third case: a version nobody ever ran, sitting in a comment
//! that reads like a measurement.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A tool whose versions are claimed in prose, and how to spot a claim.
struct Tool {
    /// The name as the ledger and the corpus spell it.
    name: &'static str,
    /// How a version claim about it reads, as a prefix to look for. The version
    /// is whatever follows, up to the first character a version cannot contain.
    prefixes: &'static [&'static str],
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "ca65",
        prefixes: &["ca65 V", "ca65 "],
    },
    Tool {
        name: "lwasm",
        prefixes: &["lwtools ", "lwasm ", "lwasm from lwtools "],
    },
    Tool {
        name: "acme",
        prefixes: &["acme ", "ACME, release "],
    },
    Tool {
        name: "sjasmplus",
        prefixes: &["sjasmplus ", "SjASMPlus Z80 Cross-Assembler v"],
    },
    Tool {
        name: "rgbasm",
        prefixes: &["rgbasm v", "rgbasm "],
    },
    Tool {
        name: "vasm",
        prefixes: &["vasm "],
    },
    Tool {
        name: "pasmo",
        prefixes: &["pasmo ", "PasmoNext v"],
    },
];

/// One claim found in the tree.
struct Claim {
    tool: &'static str,
    version: String,
    file: PathBuf,
    line: usize,
}

/// Pull the version out of the text following a prefix. A version runs while
/// the characters are digits, dots and lowercase letters — `2.0f`, `V2.18`,
/// `1.21.0`, `4.25` — and stops at anything else.
fn version_at(rest: &str) -> Option<String> {
    let v: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || c.is_ascii_lowercase())
        .collect();
    let v = v.trim_end_matches('.').to_string();
    // A version has to start with a digit and carry a dot, or it is a word.
    (v.chars().next().is_some_and(|c| c.is_ascii_digit()) && v.contains('.')).then_some(v)
}

/// Every version string the corpus ever recorded, per tool.
fn observed(repo: &Path) -> BTreeMap<&'static str, BTreeSet<String>> {
    let mut out: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    let dir = repo.join("crates/asm198x/tests/verdicts");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "ndjson") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for identity in text.match_indices("\"identity\":\"").map(|(i, m)| {
            let rest = &text[i + m.len()..];
            &rest[..rest.find('"').unwrap_or(rest.len())]
        }) {
            for tool in TOOLS {
                for prefix in tool.prefixes {
                    if let Some(at) = identity.find(prefix)
                        && let Some(v) = version_at(&identity[at + prefix.len()..])
                    {
                        out.entry(tool.name).or_default().insert(v);
                    }
                }
            }
        }
    }
    out
}

/// Every version claim in the prose and the comments.
fn claims(repo: &Path) -> Vec<Claim> {
    let mut out = Vec::new();
    let roots = [
        repo.join("decisions"),
        repo.join("crates"),
        repo.join("xtask/src"),
        repo.join("docs"),
    ];
    let mut stack: Vec<PathBuf> = roots.to_vec();
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            // The corpus is the evidence, not a claim about it.
            if path.ends_with("verdicts") || path.ends_with("book") {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&path) {
                stack.extend(entries.flatten().map(|e| e.path()));
            }
            continue;
        }
        let is_text = matches!(path.extension().and_then(|e| e.to_str()), Some("md" | "rs"));
        if !is_text {
            continue;
        }
        // Two files quote a wrong claim in order to describe it: the record
        // this check points at, and this check. Reading them as claims would
        // make the mechanism fail on its own account of itself.
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(name, "reference-versions.md" | "versions.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            for tool in TOOLS {
                for prefix in tool.prefixes {
                    let mut from = 0;
                    while let Some(at) = line[from..].find(prefix) {
                        let start = from + at + prefix.len();
                        if let Some(version) = version_at(&line[start..]) {
                            out.push(Claim {
                                tool: tool.name,
                                version,
                                file: path.strip_prefix(repo).unwrap_or(&path).to_path_buf(),
                                line: n + 1,
                            });
                        }
                        from = start.max(from + at + 1);
                    }
                }
            }
        }
    }
    out
}

/// The claims a record says stand even though nothing recorded matches them,
/// as `tool version` pairs read out of `decisions/reference-versions.md`.
fn allowed(repo: &Path) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    let Ok(text) = std::fs::read_to_string(repo.join("decisions/reference-versions.md")) else {
        return out;
    };
    // Rows of the superseded table: `| tool | version | why |`.
    for line in text.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() >= 4 && !cells[1].is_empty() && !cells[2].is_empty() {
            out.insert((cells[1].to_string(), cells[2].to_string()));
        }
    }
    out
}

/// Run the check. Returns the claims that name a version nothing recorded, and
/// that no record excuses.
pub fn check(repo: &Path) -> Result<Vec<String>, String> {
    let seen = observed(repo);
    if seen.is_empty() {
        return Err("no tool identities in the verdict corpus — nothing to check against".into());
    }
    let allow = allowed(repo);
    let mut unbacked = Vec::new();
    for claim in claims(repo) {
        let known = seen
            .get(claim.tool)
            .is_some_and(|v| v.contains(&claim.version));
        if known || allow.contains(&(claim.tool.to_string(), claim.version.clone())) {
            continue;
        }
        let recorded = seen
            .get(claim.tool)
            .map(|v| v.iter().cloned().collect::<Vec<_>>().join(", "))
            .unwrap_or_else(|| "nothing".into());
        unbacked.push(format!(
            "{}:{}: claims {} {} — the corpus recorded {}",
            claim.file.display(),
            claim.line,
            claim.tool,
            claim.version,
            recorded
        ));
    }
    unbacked.sort();
    unbacked.dedup();
    Ok(unbacked)
}

/// Print what the corpus recorded, for a reader rather than a gate.
pub fn report(repo: &Path) -> String {
    let mut out = String::from("# Reference versions this project has observed\n#\n");
    out.push_str("# Read from the `identity` of every verdict in the corpus.\n\n");
    for (tool, versions) in observed(repo) {
        out.push_str(&format!(
            "{tool:<12} {}\n",
            versions.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    out
}
