//! Per-process scratch directories for the reference-arbitrated suites (#547).
//!
//! A suite hands its source to a reference tool through the filesystem, so
//! two processes running the same suite against one fixed path overwrite each
//! other's files between the write and the tool's read. The failure is quiet:
//! hundreds of byte mismatches that do not exist, and — because the
//! differential records what it saw — false verdicts written to
//! `tests/verdicts/`. Keying the directory by process id means concurrent
//! checkouts cannot see each other's files; removing it on drop means a run
//! leaves nothing for the next one to pick up.

// Every test binary that includes `support` compiles its own copy; not all
// of them take scratch space.
#![allow(dead_code)]

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};

/// A scratch directory owned by this process, gone when dropped.
pub struct Scratch(PathBuf);

/// Create `asm198x-<suite>-<pid>` under the temp directory. Two suites in one
/// test binary run on separate threads and must not share, so the suite name
/// is part of the key alongside the pid.
pub fn dir(suite: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!("asm198x-{suite}-{}", std::process::id()));
    fs::create_dir_all(&path).expect("scratch dir");
    Scratch(path)
}

impl Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
