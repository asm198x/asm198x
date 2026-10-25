//! Pattern-library programs: how one is set up, keyed and built by Asm198x.
//!
//! Recording (the live curriculum suite), replay, and the PasmoNext gate all
//! come through here, so they agree on the three things a pattern verdict
//! depends on: the include path a source is built with, the include closure
//! its key digests, and the Asm198x command its outcome is compared with.
//!
//! The command is the CLI the curriculum's Makefiles run, not a library
//! entry point, because the Game Boy ROM is finished by the CLI: `--gb-rom`
//! pads and checksums the image, and the curriculum ships that image.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use asm198x::source::{FsLoader, LoadError, SourceLoader};
use verdict_corpus::patterns::{self, Tool};

use super::verdicts::sha256_hex;

/// One pattern source, ready to build.
pub struct Job {
    /// The source, relative to the Code198x checkout (`code-samples/…`): the
    /// path in its verdict's key.
    pub relpath: String,
    /// The source's absolute path.
    pub source: PathBuf,
    /// The folder the build runs in: the source's own.
    pub dir: PathBuf,
    /// The source's file name, as the build names it.
    pub name: String,
    /// The pattern folder given as an include path, relative to `dir`.
    pub include: String,
    /// The tool that arbitrates it.
    pub tool: Tool,
    /// Where a generated include was written, added to the include path.
    pub generated: Option<PathBuf>,
}

impl Job {
    /// Set up `source`, a file under `root` (the Code198x checkout). A
    /// generated include it needs is written under `scratch`.
    ///
    /// # Errors
    /// A source outside the checkout, a machine the table does not know, or
    /// a generator that failed.
    pub fn new(root: &Path, source: &Path, scratch: &Path) -> Result<Self, String> {
        let relpath = source
            .strip_prefix(root)
            .map_err(|_| format!("{} is not under {}", source.display(), root.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        let in_samples = relpath
            .strip_prefix("code-samples/")
            .ok_or_else(|| format!("{relpath} is not in code-samples"))?;
        let machine = in_samples.split('/').next().unwrap_or_default();
        let tool = patterns::tool(machine)
            .ok_or_else(|| format!("{machine} has no entry in the pattern table"))?;
        let dir = source.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("{relpath} has no file name"))?
            .to_string();
        let include = if patterns::folder(source) == dir {
            ".".to_string()
        } else {
            "..".to_string()
        };
        let generated = match patterns::generated(in_samples) {
            Some(g) => Some(generate(g, &dir, scratch)?),
            None => None,
        };
        Ok(Self {
            relpath,
            source: source.to_path_buf(),
            dir,
            name,
            include,
            tool,
            generated,
        })
    }

    /// The include path, in order: the pattern folder, then any generated
    /// include's folder. Both the reference and Asm198x are given this.
    pub fn include_dirs(&self) -> Vec<String> {
        let mut dirs = vec![self.include.clone()];
        if let Some(g) = &self.generated {
            dirs.push(g.to_string_lossy().into_owned());
        }
        dirs
    }

    /// The verdict key: path, variant, and the digest of the include
    /// closure.
    ///
    /// # Errors
    /// The source cannot be read, or Asm198x read a file the key cannot name.
    pub fn key(&self, root: &Path) -> Result<String, String> {
        Ok(format!(
            "{}#{}@{}",
            self.relpath,
            self.tool.variant(),
            self.closure_digest(root)?
        ))
    }

    /// A digest of every file the program reads: the source and each include
    /// and binary it loads, each named by its path in the checkout.
    ///
    /// The files are those Asm198x's include walk loads, the same walk the
    /// CLI runs. That is the reference's set too wherever the two agree, and
    /// where they do not, the verdict records a difference anyway. A change
    /// to any of them, such as a shared routine under `cross-platform/`,
    /// changes the key, so the old verdict stops answering for the new text
    /// rather than passing on it.
    ///
    /// # Errors
    /// The source cannot be read, or Asm198x read a file outside the checkout
    /// and the generated folder, which no key could name the same way twice.
    pub fn closure_digest(&self, root: &Path) -> Result<String, String> {
        let text =
            std::fs::read_to_string(&self.source).map_err(|e| format!("{}: {e}", self.relpath))?;
        let search: Vec<PathBuf> = self
            .include_dirs()
            .iter()
            .map(|d| self.dir.join(d))
            .collect();
        let loader = Recording {
            inner: FsLoader::new(&self.dir, search),
            seen: RefCell::default(),
        };
        // Only which files were read matters here; the outcome is compared
        // through the CLI.
        let _ = match self.tool {
            Tool::Pasmo => asm198x::assemble_pasmonext_files(&text, &self.name, &loader),
            Tool::Acme => asm198x::assemble_acme_files(&text, &self.name, &loader),
            Tool::Ca65 => asm198x::assemble_ca65_files(&text, &self.name, &loader),
            Tool::Lwasm => asm198x::assemble_lwasm_files(&text, &self.name, &loader),
            Tool::Vasm => asm198x::assemble_vasm_exe_files(&text, &self.name, &loader),
            Tool::Rgbasm => asm198x::assemble_rgbasm_files(&text, &self.name, &loader),
        };
        let root = root
            .canonicalize()
            .map_err(|e| format!("{}: {e}", root.display()))?;
        let generated = self.generated.as_ref().and_then(|g| g.canonicalize().ok());
        let mut files = BTreeMap::new();
        files.insert(self.relpath.clone(), sha256_hex(text.as_bytes()));
        for (path, bytes) in loader.seen.into_inner() {
            let path = PathBuf::from(&path);
            let name = if let Ok(rel) = path.strip_prefix(&root) {
                rel.to_string_lossy().replace('\\', "/")
            } else if let Some(rel) = generated.as_ref().and_then(|g| path.strip_prefix(g).ok()) {
                format!("<generated>/{}", rel.to_string_lossy().replace('\\', "/"))
            } else {
                return Err(format!(
                    "{} read {}, which is outside the checkout",
                    self.relpath,
                    path.display()
                ));
            };
            files.insert(name, sha256_hex(&bytes));
        }
        let listing: String = files
            .iter()
            .map(|(name, digest)| format!("{name}\t{digest}\n"))
            .collect();
        Ok(sha256_hex(listing.as_bytes()))
    }

    /// Build the program the way the curriculum's Makefile does, with the
    /// Asm198x CLI: its output, or its last line of complaint.
    ///
    /// # Errors
    /// The CLI rejected the program; the message is its last line.
    pub fn assemble(&self, scratch: &Path) -> Result<Vec<u8>, String> {
        let out = scratch.join("asm198x-pattern.bin");
        let _ = std::fs::remove_file(&out);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_asm198x"));
        cmd.current_dir(&self.dir);
        match self.tool {
            Tool::Pasmo => cmd.args(["--dialect", "pasmonext"]),
            Tool::Vasm => cmd.args(["--dialect", "vasm", "--exe"]),
            Tool::Rgbasm => cmd.args(["--dialect", "rgbasm", "--gb-rom"]),
            tool => cmd.args(["--dialect", tool.dialect()]),
        };
        for dir in self.include_dirs() {
            cmd.arg("-I").arg(dir);
        }
        cmd.arg(&self.name).arg("-o").arg(&out);
        let output = cmd
            .output()
            .map_err(|e| format!("cannot run asm198x: {e}"))?;
        if output.status.success() {
            std::fs::read(&out).map_err(|e| format!("asm198x wrote no output: {e}"))
        } else {
            Err(last_line(&output))
        }
    }
}

/// The last non-blank line a process wrote, either stream: where the tools
/// put the error that stopped them.
pub fn last_line(output: &std::process::Output) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("(no message)")
        .to_string()
}

/// Run a generated include's command, writing under `scratch`, and return
/// the folder to add to the include path.
fn generate(g: &patterns::Generated, dir: &Path, scratch: &Path) -> Result<PathBuf, String> {
    let folder = scratch.join("generated");
    let out = folder.join(g.include);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let (program, args) = g
        .command
        .split_first()
        .ok_or("an empty generator command")?;
    let output = Command::new(program)
        .args(args)
        .arg(&out)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("cannot run {program} for {}: {e}", g.source))?;
    if !output.status.success() {
        return Err(format!(
            "generating {} for {} failed: {}",
            g.include,
            g.source,
            last_line(&output)
        ));
    }
    Ok(folder)
}

/// An `FsLoader` that remembers every file it hands over.
struct Recording {
    inner: FsLoader,
    /// Canonical path to contents.
    seen: RefCell<BTreeMap<String, Vec<u8>>>,
}

impl SourceLoader for Recording {
    fn load_text(&self, request: &str, from: Option<&str>) -> Result<(String, String), LoadError> {
        let (canonical, text) = self.inner.load_text(request, from)?;
        self.seen
            .borrow_mut()
            .insert(canonical.clone(), text.clone().into_bytes());
        Ok((canonical, text))
    }

    fn load_binary(&self, request: &str, from: Option<&str>) -> Result<Vec<u8>, LoadError> {
        let bytes = self.inner.load_binary(request, from)?;
        if let Some(canonical) = self.inner.resolve_text(request, from) {
            self.seen.borrow_mut().insert(canonical, bytes.clone());
        }
        Ok(bytes)
    }

    fn resolve_text(&self, request: &str, from: Option<&str>) -> Option<String> {
        self.inner.resolve_text(request, from)
    }
}

/// Every pattern source in the checkout, across every machine that has any.
/// `root` is the Code198x checkout.
pub fn all_sources(root: &Path) -> Vec<PathBuf> {
    let samples = root.join("code-samples");
    patterns::machines(&samples)
        .iter()
        .flat_map(|m| patterns::sources(&samples.join(m)))
        .collect()
}
