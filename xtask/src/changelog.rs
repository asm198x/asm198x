//! The newest changelog entry has to be written for a reader.
//!
//! `/releases` renders `crates/asm198x/CHANGELOG.md`, so its entries are
//! published prose rather than an internal record. release-plz drafts them from
//! commit subjects, and two things about this repository make that draft an
//! unreliable finished article:
//!
//! - A squash-merge subject without a conventional-commit prefix lands under
//!   **`### Other`** as its raw subject. Every merge here is a squash.
//! - A change under `docs/` belongs to no package, so release-plz never sees
//!   it — and a release whose substance is documentation gets an entry that
//!   omits it entirely.
//!
//! Neither is a bug in release-plz. The first is a convention we can keep; the
//! second follows from the book living outside the crate, which
//! `decisions/one-documentation-surface.md` settled for better reasons.
//!
//! So the entry is authored, and this is what stops that being a step someone
//! remembers. It checks the **newest** entry only: older ones are history and
//! are left as they are.

use std::path::{Path, PathBuf};

/// Where the published changelog lives.
pub fn path(repo: &Path) -> PathBuf {
    repo.join("crates/asm198x/CHANGELOG.md")
}

/// What the newest entry looks like.
pub struct Report {
    /// The version heading it was found under.
    pub version: String,
    /// Section headings the entry carries.
    pub sections: Vec<String>,
}

/// Read the newest released entry — the first `## [x.y.z]` heading, skipping
/// the `## [Unreleased]` placeholder that carries nothing.
///
/// # Errors
/// An unreadable file, or one with no released entry at all.
pub fn newest(repo: &Path) -> Result<Report, String> {
    let text = std::fs::read_to_string(path(repo))
        .map_err(|e| format!("{}: {e}", path(repo).display()))?;

    let mut version = None;
    let mut sections = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## [") {
            let name = rest.split(']').next().unwrap_or_default();
            if name.eq_ignore_ascii_case("unreleased") {
                continue;
            }
            if version.is_some() {
                break; // into the previous entry; stop.
            }
            version = Some(name.to_string());
            continue;
        }
        if version.is_some()
            && let Some(heading) = line.strip_prefix("### ")
        {
            sections.push(heading.trim().to_string());
        }
    }

    match version {
        Some(version) => Ok(Report { version, sections }),
        None => Err("the changelog carries no released entry".to_string()),
    }
}

/// Fail when the newest entry still reads like a draft.
///
/// # Errors
/// The entry carries an `Other` section, which is where release-plz files a
/// commit it could not classify.
pub fn check(repo: &Path) -> Result<String, String> {
    let report = newest(repo)?;
    if report
        .sections
        .iter()
        .any(|s| s.eq_ignore_ascii_case("other"))
    {
        return Err(format!(
            "v{} still has an `### Other` section.\n\
             \n\
             That is where release-plz files a commit whose subject carried no \
             conventional-commit prefix, as its raw subject — and this file is \
             published at `/releases`, so those subjects are what a reader gets.\n\
             \n\
             Rewrite the entry as what changed and why it matters, grouped under \
             Added / Fixed / Changed. Do it immediately before merging the \
             release: release-plz regenerates the entry on every push to main, \
             so an early edit is liable to be overwritten.\n\
             \n\
             See `decisions/changelog-is-authored.md`.",
            report.version
        ));
    }
    Ok(format!(
        "v{} reads as {} section(s), none of them `Other`",
        report.version,
        report.sections.len()
    ))
}

#[cfg(test)]
mod tests {
    /// The check keys on the newest entry only, because older ones are history.
    /// Several releases before this gate existed carry `Other`, and rewriting
    /// them now would be inventing a record of what someone meant at the time.
    #[test]
    fn only_the_newest_entry_is_judged() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        let report = super::newest(&repo).expect("a released entry exists");
        assert!(
            !report.version.is_empty(),
            "the newest entry should name a version"
        );
        assert!(
            !report.sections.is_empty(),
            "v{} carries no sections at all",
            report.version
        );
    }

    #[test]
    fn the_committed_changelog_passes_its_own_gate() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        super::check(&repo).expect("the newest entry is written for a reader");
    }
}
