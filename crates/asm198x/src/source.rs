//! The multi-file source model (language-surface U1, KTD2/KTD8): the loader
//! seam, the `FileId`-allocating source map, and the include graph.
//!
//! Includes force a multi-file world, and three invariants anchor it here:
//!
//! - **The loader is a seam, not a hard-wired filesystem** (KTD8). Include and
//!   incbin resolution both go through [`SourceLoader`], which carries *both*
//!   a text load and a binary load and owns path canonicalization — so incbin
//!   can never fork resolution behaviour from include, and unit tests stay
//!   hermetic with the [`MemoryLoader`]. The CLI wires an [`FsLoader`] carrying
//!   the input file's directory plus the ordered `-I` search dirs.
//! - **One `FileId` space, one table** (KTD2). The [`SourceMap`] owns all
//!   `FileId` allocation: `FileId(0)` is the root input, and a file requested
//!   twice — however spelled — dedups to one id by canonical path. Its
//!   [`file_table`](SourceMap::file_table) is exactly the contract's
//!   `AssemblyResult::files` list (index ⇔ `FileId`).
//! - **Binary loads mint no `FileId`.** Spans only ever point into source
//!   files; incbin data is bytes at the *directive's* span, so
//!   [`SourceLoader::load_binary`] returns bytes without touching the map.
//!
//! The map also records the include graph — who included whom, at which line —
//! so the human renderer can emit rustc-style `included from <file>:<line>`
//! notes ([`SourceMap::include_chain`]). The include-capable entry points that
//! drive all of this land in U2; U1 is the foundation they build on.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::span::FileId;

/// A failed load: the request as written in the directive, the requesting
/// file (`None` for the root input / CLI), and the underlying reason. Carries
/// both names so an include failure is diagnosable from the message alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    /// The path as the directive spelled it.
    pub request: String,
    /// The canonical path of the file that asked, `None` when the root did.
    pub from: Option<String>,
    /// The underlying reason (not found, unreadable, …).
    pub message: String,
}

impl LoadError {
    fn new(request: &str, from: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            request: request.to_owned(),
            from: from.map(str::to_owned),
            message: message.into(),
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.from {
            Some(from) => write!(
                f,
                "cannot load `{}` (requested from {from}): {}",
                self.request, self.message
            ),
            None => write!(f, "cannot load `{}`: {}", self.request, self.message),
        }
    }
}

impl std::error::Error for LoadError {}

/// The file-loading seam (KTD8). Implementations own resolution *and*
/// canonicalization: `load_text` returns the canonical path (the
/// [`SourceMap`]'s dedup key) alongside the contents, and `load_binary`
/// resolves through the same machinery so include and incbin can never
/// disagree about where a name points. `from` is the requesting file's
/// canonical path (`None` for the root input), used for error reporting.
pub trait SourceLoader {
    /// Resolve `request` and load it as source text, returning
    /// `(canonical_path, contents)`.
    ///
    /// # Errors
    /// A [`LoadError`] naming the request and the requesting file when the
    /// target cannot be resolved or read.
    fn load_text(&self, request: &str, from: Option<&str>) -> Result<(String, String), LoadError>;

    /// Resolve `request` and load it as raw bytes (the incbin path). No
    /// `FileId` is minted — binary data has no spans.
    ///
    /// # Errors
    /// A [`LoadError`] naming the request and the requesting file when the
    /// target cannot be resolved or read.
    fn load_binary(&self, request: &str, from: Option<&str>) -> Result<Vec<u8>, LoadError>;
}

/// The filesystem loader the CLI wires: a relative request is tried against
/// the input file's directory first, then each `-I` search directory in
/// command-line order; an absolute request is used as-is. Resolution order
/// beyond that is a per-dialect semantic, probed in U2+ (KTD5/KTD8).
pub struct FsLoader {
    /// The root input's directory — the first search location.
    base: PathBuf,
    /// Ordered `-I` search directories, tried after `base`.
    search: Vec<PathBuf>,
}

impl FsLoader {
    /// A loader rooted at `base` (the input file's directory) with the ordered
    /// `-I` search directories.
    pub fn new(base: impl Into<PathBuf>, search: Vec<PathBuf>) -> Self {
        Self {
            base: base.into(),
            search,
        }
    }

    /// The first existing candidate for `request`, in search order.
    fn resolve(&self, request: &str) -> Option<PathBuf> {
        let req = Path::new(request);
        if req.is_absolute() {
            return req.exists().then(|| req.to_path_buf());
        }
        std::iter::once(&self.base)
            .chain(self.search.iter())
            .map(|dir| dir.join(req))
            .find(|candidate| candidate.exists())
    }

    /// Resolve and canonicalize, so two spellings of one file share a key.
    fn resolve_canonical(&self, request: &str, from: Option<&str>) -> Result<PathBuf, LoadError> {
        let path = self
            .resolve(request)
            .ok_or_else(|| LoadError::new(request, from, "file not found"))?;
        std::fs::canonicalize(&path).map_err(|e| LoadError::new(request, from, e.to_string()))
    }
}

impl SourceLoader for FsLoader {
    fn load_text(&self, request: &str, from: Option<&str>) -> Result<(String, String), LoadError> {
        let canonical = self.resolve_canonical(request, from)?;
        let contents = std::fs::read_to_string(&canonical)
            .map_err(|e| LoadError::new(request, from, e.to_string()))?;
        Ok((canonical.to_string_lossy().into_owned(), contents))
    }

    fn load_binary(&self, request: &str, from: Option<&str>) -> Result<Vec<u8>, LoadError> {
        let canonical = self.resolve_canonical(request, from)?;
        std::fs::read(&canonical).map_err(|e| LoadError::new(request, from, e.to_string()))
    }
}

/// The hermetic in-memory loader for unit tests: registered names are the
/// canonical paths (no filesystem, no normalization), so tests control the
/// whole resolution space explicitly.
#[derive(Default)]
pub struct MemoryLoader {
    texts: HashMap<String, String>,
    binaries: HashMap<String, Vec<u8>>,
}

impl MemoryLoader {
    /// An empty loader; register files with [`text`](Self::text) and
    /// [`binary`](Self::binary).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a source file under `path` (its canonical name).
    #[must_use]
    pub fn text(mut self, path: impl Into<String>, contents: impl Into<String>) -> Self {
        self.texts.insert(path.into(), contents.into());
        self
    }

    /// Register a binary file under `path` (its canonical name).
    #[must_use]
    pub fn binary(mut self, path: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.binaries.insert(path.into(), bytes);
        self
    }
}

impl SourceLoader for MemoryLoader {
    fn load_text(&self, request: &str, from: Option<&str>) -> Result<(String, String), LoadError> {
        self.texts
            .get(request)
            .map(|contents| (request.to_owned(), contents.clone()))
            .ok_or_else(|| LoadError::new(request, from, "file not found"))
    }

    fn load_binary(&self, request: &str, from: Option<&str>) -> Result<Vec<u8>, LoadError> {
        self.binaries
            .get(request)
            .cloned()
            .ok_or_else(|| LoadError::new(request, from, "file not found"))
    }
}

/// One loaded source file: its canonical path, its contents, and where it was
/// included from (`None` for the root input).
struct SourceFile {
    path: String,
    contents: String,
    /// The includer and the 1-based line of the include directive; the first
    /// inclusion wins when a deduped file is requested again.
    included_from: Option<(FileId, u32)>,
}

/// The one `FileId` space (KTD2): allocation, dedup by canonical path, and the
/// include graph. `FileId(0)` is always the root input; every include a loader
/// resolves gets one id per canonical path however many times — and however
/// spelled — it is requested.
pub struct SourceMap {
    /// `files[i]` ⇔ `FileId(i)`.
    files: Vec<SourceFile>,
    /// Canonical path → allocated id (the dedup index).
    by_path: HashMap<String, FileId>,
}

impl SourceMap {
    /// A map holding the root input as `FileId(0)`. The root's path is stored
    /// as given (the CLI's spelling — it names the file in messages and the
    /// contract table); includes are keyed by the loader's canonical paths.
    pub fn new(root_path: impl Into<String>, contents: impl Into<String>) -> Self {
        let path = root_path.into();
        let mut by_path = HashMap::new();
        by_path.insert(path.clone(), FileId(0));
        Self {
            files: vec![SourceFile {
                path,
                contents: contents.into(),
                included_from: None,
            }],
            by_path,
        }
    }

    /// Load `request` through `loader`, requested by `from` at 1-based `line`
    /// (the include directive's position, recorded in the include graph). An
    /// already-loaded canonical path returns its existing `FileId` without
    /// reloading; a new one is allocated the next id.
    ///
    /// # Errors
    /// The loader's [`LoadError`] when the target cannot be resolved or read.
    pub fn load(
        &mut self,
        loader: &dyn SourceLoader,
        request: &str,
        from: FileId,
        line: u32,
    ) -> Result<FileId, LoadError> {
        let from_path = self.path(from).map(str::to_owned);
        let (canonical, contents) = loader.load_text(request, from_path.as_deref())?;
        if let Some(&id) = self.by_path.get(&canonical) {
            return Ok(id);
        }
        let id = FileId(self.files.len() as u32);
        self.by_path.insert(canonical.clone(), id);
        self.files.push(SourceFile {
            path: canonical,
            contents,
            included_from: Some((from, line)),
        });
        Ok(id)
    }

    /// The path of `id`, if allocated (canonical for includes; the root's is
    /// the spelling it was created with).
    #[must_use]
    pub fn path(&self, id: FileId) -> Option<&str> {
        self.files.get(id.0 as usize).map(|f| f.path.as_str())
    }

    /// The contents of `id`, if allocated.
    #[must_use]
    pub fn contents(&self, id: FileId) -> Option<&str> {
        self.files.get(id.0 as usize).map(|f| f.contents.as_str())
    }

    /// The contract's file table (`AssemblyResult::files`): one path per
    /// allocated `FileId`, in id order, entry 0 the root input.
    #[must_use]
    pub fn file_table(&self) -> Vec<String> {
        self.files.iter().map(|f| f.path.clone()).collect()
    }

    /// The include chain of `id`, innermost hop first: each element is
    /// `(includer_path, line_of_the_include_directive)`, walking back to the
    /// root. Empty for the root itself — it was included from nowhere.
    #[must_use]
    pub fn include_chain(&self, id: FileId) -> Vec<(String, u32)> {
        let mut chain = Vec::new();
        let mut cursor = self.files.get(id.0 as usize).and_then(|f| f.included_from);
        while let Some((parent, line)) = cursor {
            let Some(file) = self.files.get(parent.0 as usize) else {
                break;
            };
            chain.push((file.path.clone(), line));
            cursor = file.included_from;
        }
        chain
    }
}
