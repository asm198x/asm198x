//! Opt-in build execution and its evidence-backed compatibility table.
use super::{
    Manifest, Project, Target,
    plan::BuildPlan,
    process::{self, Process},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Equivalent,
    ReferenceFailure,
    AssemblerRejected,
    PostprocessFailure,
    ArtifactMismatch,
    InfrastructureUnavailable,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Artifact {
    path: String,
    length: u64,
    sha256: String,
    #[serde(default)]
    hex: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Identity {
    path: String,
    sha256: String,
    version: Process,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Assembler {
    source_commit: String,
    cargo_lock_sha256: String,
    executable_sha256: String,
    build: Process,
    version: Process,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct StepResult {
    stage: String,
    process: Process,
    #[serde(default)]
    outputs: Vec<Artifact>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Pair {
    reference: Artifact,
    assembler: Artifact,
    equal: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct LicenseEvidence {
    spdx: String,
    path: String,
    sha256: String,
    text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Record {
    project: String,
    target: String,
    source_commit: String,
    source_tree_sha256: Option<String>,
    #[serde(default)]
    license: Option<LicenseEvidence>,
    plan_sha256: Option<String>,
    outcome: Outcome,
    first_failure: Option<String>,
    tools: BTreeMap<String, Identity>,
    adapters: Vec<Artifact>,
    reference_steps: Vec<StepResult>,
    assembler_steps: Vec<StepResult>,
    comparisons: Vec<Pair>,
    issues: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Report {
    schema: u32,
    captured_unix_seconds: u64,
    manifest_sha256: String,
    assembler: Assembler,
    #[serde(default)]
    reference_report_sha256: Option<String>,
    #[serde(default)]
    reference_captured_unix_seconds: Option<u64>,
    selected: Vec<String>,
    records: Vec<Record>,
}

fn finish(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask ecosystem: {e}");
            ExitCode::FAILURE
        }
    }
}
pub(super) fn audit(repo: &Path, manifest: &Manifest, args: &[String]) -> ExitCode {
    finish(audit_inner(repo, manifest, args))
}
pub(super) fn replay(repo: &Path, manifest: &Manifest, args: &[String]) -> ExitCode {
    finish(replay_inner(repo, manifest, args))
}
pub(super) fn ledger(repo: &Path, manifest: &Manifest, args: &[String]) -> ExitCode {
    finish(ledger_inner(repo, manifest, args))
}

fn json_write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("report exceeded size bound".into());
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
fn manifest_hash(repo: &Path) -> Result<String, String> {
    Ok(process::bytes_hash(
        &std::fs::read(super::manifest_path(repo)).map_err(|e| e.to_string())?,
    ))
}
fn plan_hash(plan: &BuildPlan) -> Result<String, String> {
    Ok(process::bytes_hash(
        &serde_json::to_vec(plan).map_err(|e| e.to_string())?,
    ))
}
fn reviewed_issues(repo: &Path, project: &Project, target: &Target) -> Result<Vec<String>, String> {
    let path = repo.join("ecosystem/results/current.json");
    if path.exists() {
        let previous = read_report(&path)?;
        let expected_plan = target.build.as_ref().map(plan_hash).transpose()?;
        if let Some(record) = previous.records.iter().find(|r| {
            r.project == project.id
                && r.target == target.id
                && r.source_commit == project.commit
                && r.plan_sha256 == expected_plan
        }) {
            return Ok(record.issues.clone());
        }
    }
    Ok(target
        .build
        .as_ref()
        .map_or_else(Vec::new, |p| p.issues.clone()))
}

fn key(project: &Project, target: &Target) -> String {
    format!("{}/{}", project.id, target.id)
}
fn targets(manifest: &Manifest) -> impl Iterator<Item = (&Project, &Target)> {
    manifest
        .projects
        .iter()
        .flat_map(|p| p.targets.iter().map(move |t| (p, t)))
}

/// Only fresh, task-owned directories may be deleted by this command.
struct OwnedTree(PathBuf);
impl OwnedTree {
    fn new(path: PathBuf) -> Result<Self, String> {
        std::fs::create_dir(&path)
            .map_err(|e| format!("cannot create fresh {}: {e}", path.display()))?;
        Ok(Self(path))
    }
}
impl Drop for OwnedTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn within(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if !super::safe_relative(relative) {
        return Err(format!("unsafe relative path `{relative}`"));
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = root.join(relative);
    let mut existing = path.as_path();
    while !existing.exists() {
        existing = existing.parent().ok_or("path has no existing parent")?;
    }
    if !existing
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(&root)
    {
        return Err(format!("path escapes its workspace: {relative}"));
    }
    // Dangling symlinks do not count as existing paths, so inspect every
    // component too rather than following only the deepest existing ancestor.
    let mut at = root;
    for component in Path::new(relative).components() {
        at.push(component);
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!("symlinks are not build paths: {relative}"));
        }
    }
    Ok(path)
}

fn source_files(checkout: &Path) -> Result<Vec<String>, String> {
    let status = process::git(
        checkout,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if !status.is_empty() {
        return Err(format!("{} has source changes", checkout.display()));
    }
    let files = process::git(checkout, &["ls-files", "-z"])?;
    let files: Vec<_> = files
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    if files.is_empty() {
        return Err("source checkout has no tracked files".into());
    }
    Ok(files)
}
fn tree_hash(root: &Path, files: &[String], excluded: &BTreeSet<String>) -> Result<String, String> {
    let mut records = Vec::new();
    let mut total = 0;
    for file in files.iter().filter(|f| !excluded.contains(*f)) {
        let path = within(root, file)?;
        let (length, hash) = process::digest(&path, 512 * 1024 * 1024)?;
        total += length;
        if total > 512 * 1024 * 1024 {
            return Err("source tree exceeds 512 MiB".into());
        }
        records.push((file, length, hash));
    }
    Ok(process::bytes_hash(
        &serde_json::to_vec(&records).map_err(|e| e.to_string())?,
    ))
}
fn copy_tree(source: &Path, destination: &Path, files: &[String]) -> Result<(), String> {
    for file in files {
        let from = within(source, file)?;
        let to = within(destination, file)?;
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(from, to).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn artifact(root: &Path, relative: &str, keep: bool) -> Result<Artifact, String> {
    let path = within(root, relative)?;
    let (length, sha256) = process::digest(&path, process::MAX_ARTIFACT)?;
    if length == 0 {
        return Err(format!("empty build output `{relative}`"));
    }
    let hex = if keep && length <= 65536 {
        Some(
            std::fs::read(&path)
                .map_err(|e| e.to_string())?
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        )
    } else {
        None
    };
    Ok(Artifact {
        path: relative.to_string(),
        length,
        sha256,
        hex,
    })
}

fn build_assembler(repo: &Path, work: &Path, logs: &Path) -> Result<(Assembler, PathBuf), String> {
    let _ = source_files(repo)?;
    let commit = process::git(repo, &["rev-parse", "HEAD"])?;
    let lock = process::digest(&repo.join("Cargo.lock"), process::MAX_ARTIFACT)?.1;
    let target = work.join("asm-build");
    let argv = vec![
        process::resolve("cargo")?.display().to_string(),
        "build".into(),
        "--locked".into(),
        "--release".into(),
        "-p".into(),
        "asm198x".into(),
        "--features".into(),
        "lua".into(),
        "--target-dir".into(),
        target.display().to_string(),
    ];
    let build = process::run(&argv, repo, logs, "build-assembler", 600, &[])?;
    if !build.succeeded() {
        return Err(format!(
            "Asm198x producer build failed: {}{}",
            build.stdout, build.stderr
        ));
    }
    let _ = source_files(repo)?;
    if process::git(repo, &["rev-parse", "HEAD"])? != commit
        || process::digest(&repo.join("Cargo.lock"), process::MAX_ARTIFACT)?.1 != lock
    {
        return Err("Asm198x source or lock changed during its build".into());
    }
    let program = target.join("release").join(if cfg!(windows) {
        "asm198x.exe"
    } else {
        "asm198x"
    });
    let executable_sha256 = process::digest(&program, 128 * 1024 * 1024)?.1;
    let version = process::run(
        &[program.display().to_string(), "--version".into()],
        repo,
        logs,
        "assembler-version",
        10,
        &[],
    )?;
    if !version.succeeded() {
        return Err("built Asm198x version probe failed".into());
    }
    Ok((
        Assembler {
            source_commit: commit,
            cargo_lock_sha256: lock,
            executable_sha256,
            build,
            version,
        },
        program,
    ))
}

fn audit_inner(repo: &Path, manifest: &Manifest, args: &[String]) -> Result<(), String> {
    let [sources, destination, selection @ ..] = args else {
        return Err("usage: ecosystem audit <sources> <new-results> --all|--target=<project/target> [--record-findings]".into());
    };
    let requested: BTreeSet<_> = selection
        .iter()
        .filter_map(|s| s.strip_prefix("--target=").map(str::to_string))
        .collect();
    let all = selection.iter().any(|s| s == "--all");
    if all == !requested.is_empty()
        || selection
            .iter()
            .any(|s| s != "--all" && s != "--record-findings" && !s.starts_with("--target="))
    {
        return Err("select --all or explicit --target=project/target entries".into());
    }
    let available: BTreeSet<_> = targets(manifest).map(|(p, t)| key(p, t)).collect();
    if !requested.is_subset(&available) {
        return Err("unknown selected target".into());
    }
    let selected: Vec<_> = targets(manifest)
        .filter(|(p, t)| all || requested.contains(&key(p, t)))
        .collect();
    if selected.is_empty() {
        return Err("empty audit selection".into());
    }
    let destination = Path::new(destination);
    if !destination.is_absolute() {
        return Err("result directory must be absolute".into());
    }
    std::fs::create_dir(destination).map_err(|e| format!("result directory must be new: {e}"))?;
    let logs = destination.join("logs");
    std::fs::create_dir(&logs).map_err(|e| e.to_string())?;
    let work = OwnedTree::new(destination.join("work"))?;
    let (assembler, program) = build_assembler(repo, &work.0, &logs)?;
    let mut report = Report {
        schema: 1,
        captured_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs(),
        manifest_sha256: manifest_hash(repo)?,
        assembler,
        reference_report_sha256: None,
        reference_captured_unix_seconds: None,
        selected: selected.iter().map(|(p, t)| key(p, t)).collect(),
        records: Vec::new(),
    };
    for (project, target) in selected {
        let mut record = Record {
            project: project.id.clone(),
            target: target.id.clone(),
            source_commit: project.commit.clone(),
            source_tree_sha256: None,
            license: None,
            plan_sha256: None,
            outcome: Outcome::Unknown,
            first_failure: None,
            tools: BTreeMap::new(),
            adapters: Vec::new(),
            reference_steps: Vec::new(),
            assembler_steps: Vec::new(),
            comparisons: Vec::new(),
            issues: reviewed_issues(repo, project, target)?,
        };
        if let Err(e) = target_audit(
            &AuditContext {
                repo,
                sources: Path::new(sources),
                work: &work.0,
                logs: &logs,
                program: &program,
            },
            project,
            target,
            &mut record,
            None,
        ) {
            record.outcome = Outcome::InfrastructureUnavailable;
            record.first_failure = Some(e);
        }
        println!(
            "{}/{}: {:?}{}",
            project.id,
            target.id,
            record.outcome,
            record
                .first_failure
                .as_ref()
                .map_or(String::new(), |s| format!(" — {s}"))
        );
        report.records.push(record);
        json_write(&destination.join("report.json"), &report)?;
    }
    validate_report(repo, manifest, &report)?;
    let findings = selection.iter().any(|s| s == "--record-findings");
    if report.records.iter().any(|r| {
        !matches!(r.outcome, Outcome::Equivalent)
            && !(findings
                && matches!(
                    r.outcome,
                    Outcome::AssemblerRejected
                        | Outcome::PostprocessFailure
                        | Outcome::ArtifactMismatch
                ))
    }) {
        return Err(
            "audit contains non-equivalent or unavailable targets; inspect report.json".into(),
        );
    }
    Ok(())
}

struct AuditContext<'a> {
    repo: &'a Path,
    sources: &'a Path,
    work: &'a Path,
    logs: &'a Path,
    program: &'a Path,
}

fn target_audit(
    context: &AuditContext<'_>,
    project: &Project,
    target: &Target,
    record: &mut Record,
    control: Option<&Record>,
) -> Result<(), String> {
    let AuditContext {
        repo,
        sources,
        work,
        logs,
        program,
    } = *context;
    let Some(plan) = &target.build else {
        record.first_failure = Some("no reviewed complete build plan".into());
        return Ok(());
    };
    record.plan_sha256 = Some(plan_hash(plan)?);
    let checkout = sources.join(&project.id);
    super::verify_project(project, &checkout)?;
    let license_path = within(&checkout, &project.license.path)?;
    let (_, license_hash) = process::digest(&license_path, 65536)?;
    record.license = Some(LicenseEvidence {
        spdx: project.license.spdx.clone(),
        path: project.license.path.clone(),
        sha256: license_hash,
        text: std::fs::read_to_string(license_path).map_err(|e| e.to_string())?,
    });
    let files = source_files(&checkout)?;
    let original = tree_hash(&checkout, &files, &BTreeSet::new())?;
    record.source_tree_sha256 = Some(original.clone());
    let name = format!("{}--{}", project.id, target.id);
    let mut executables = BTreeMap::new();
    executables.insert("asm198x".to_string(), program.to_path_buf());
    if let Some(control) = control {
        record.tools = control.tools.clone();
    }
    for tool in &plan.tools {
        if control.is_some()
            && !plan
                .assembler
                .iter()
                .any(|s| s.argv.first() == Some(&tool.name))
        {
            continue;
        }
        let path = process::resolve(&tool.name)?;
        let sha256 = process::digest(&path, 128 * 1024 * 1024)?.1;
        let mut argv = vec![path.display().to_string()];
        argv.extend(tool.probe.clone());
        let version = process::run(
            &argv,
            repo,
            logs,
            &format!("{name}-version-{}", tool.name),
            10,
            &[],
        )?;
        let text = format!("{}{}", version.stdout, version.stderr);
        let matches = version.succeeded()
            && tool
                .versions
                .iter()
                .any(|wanted| text.lines().any(|line| line.trim() == wanted));
        record.tools.insert(
            tool.name.clone(),
            Identity {
                path: path.display().to_string(),
                sha256,
                version,
            },
        );
        if !matches {
            return Err(format!(
                "{} version is outside the reviewed identities",
                tool.name
            ));
        }
        executables.insert(tool.name.clone(), path);
    }
    for path in &plan.adapters {
        record.adapters.push(artifact(repo, path, false)?);
    }
    let owned = OwnedTree::new(work.join(&name))?;
    let native = owned.0.join("reference");
    let asm = owned.0.join("assembler");
    for root in [&native, &asm] {
        std::fs::create_dir(root).map_err(|e| e.to_string())?;
        copy_tree(&checkout, root, &files)?;
        if tree_hash(root, &files, &BTreeSet::new())? != original {
            return Err("source copy changed tracked bytes".into());
        }
        for path in &plan.adapters {
            let to = within(root, &format!(".asm198x-adapters/{path}"))?;
            if to.exists() {
                return Err("adapter would replace upstream source".into());
            }
            std::fs::create_dir_all(to.parent().ok_or("adapter has no parent")?)
                .map_err(|e| e.to_string())?;
            std::fs::copy(within(repo, path)?, to).map_err(|e| e.to_string())?;
        }
        for path in &plan.directories {
            std::fs::create_dir_all(within(root, path)?).map_err(|e| e.to_string())?;
        }
    }
    let mut excluded = BTreeSet::new();
    for step in plan.reference.iter().chain(&plan.assembler) {
        for path in &step.outputs {
            excluded.insert(path.clone());
        }
    }
    for input in &target.sources {
        if excluded
            .iter()
            .any(|output| output == input || Path::new(output).starts_with(input))
        {
            return Err(format!("build output overlaps a declared input: {input}"));
        }
    }
    let protected = tree_hash(&checkout, &files, &excluded)?;
    for (side, root, steps) in [
        ("reference", &native, &plan.reference),
        ("assembler", &asm, &plan.assembler),
    ] {
        // Delete only explicitly declared generated files from these fresh
        // copies, so a committed sample ROM cannot masquerade as new output.
        for path in &excluded {
            let output = within(root, path)?;
            if output.is_file() {
                std::fs::remove_file(output).map_err(|e| e.to_string())?;
            }
        }
        if side == "reference"
            && let Some(control) = control
        {
            record.reference_steps = control.reference_steps.clone();
            continue;
        }
        for (index, step) in steps.iter().enumerate() {
            let mut argv = vec![
                executables
                    .get(&step.argv[0])
                    .ok_or("unreviewed executable")?
                    .display()
                    .to_string(),
            ];
            for argument in &step.argv[1..] {
                if let Some(adapter) = argument.strip_prefix("@adapter:") {
                    if !plan.adapters.iter().any(|p| p == adapter) {
                        return Err("undeclared adapter argument".into());
                    }
                    argv.push(
                        within(root, &format!(".asm198x-adapters/{adapter}"))?
                            .display()
                            .to_string(),
                    );
                } else {
                    argv.push(argument.clone());
                }
            }
            let watched: Vec<_> = step
                .outputs
                .iter()
                .map(|p| within(root, p))
                .collect::<Result<_, _>>()?;
            let process = process::run(
                &argv,
                &within(root, &step.cwd)?,
                logs,
                &format!("{name}-{side}-{index}"),
                step.timeout_seconds,
                &watched,
            )?;
            let mut failure = if !process.succeeded() {
                let streams = format!("{}{}", process.stderr, process.stdout);
                let reason = process.failure.as_deref().unwrap_or_else(|| {
                    streams
                        .lines()
                        .find(|line| {
                            let line = line.to_ascii_lowercase();
                            ["error", "invalid", "unknown", "unsupported"]
                                .iter()
                                .any(|word| line.contains(word))
                        })
                        .or_else(|| streams.lines().find(|line| !line.trim().is_empty()))
                        .unwrap_or("process exited unsuccessfully")
                });
                Some(format!(
                    "{side} {} step {} failed: {reason}",
                    step.stage,
                    index + 1
                ))
            } else {
                watched
                    .iter()
                    .find_map(|path| process::digest(path, process::MAX_ARTIFACT).err())
                    .or_else(|| {
                        watched
                            .iter()
                            .find(|p| p.metadata().is_ok_and(|m| m.len() == 0))
                            .map(|p| format!("empty build output {}", p.display()))
                    })
            };
            let mut outputs = Vec::new();
            if process.succeeded() {
                for path in &step.outputs {
                    match artifact(
                        root,
                        path,
                        side == "reference" && plan.compare.iter().any(|p| &p.reference == path),
                    ) {
                        Ok(output) => outputs.push(output),
                        Err(error) => {
                            failure.get_or_insert(error);
                        }
                    }
                }
            }
            let steps = if side == "reference" {
                &mut record.reference_steps
            } else {
                &mut record.assembler_steps
            };
            steps.push(StepResult {
                stage: step.stage.clone(),
                process,
                outputs,
            });
            if tree_hash(root, &files, &excluded)? != protected {
                return Err("build changed protected upstream source".into());
            }
            if let Some(failure) = failure {
                record.outcome = if side == "reference" {
                    Outcome::ReferenceFailure
                } else if step.stage == "assemble" {
                    Outcome::AssemblerRejected
                } else {
                    Outcome::PostprocessFailure
                };
                record.first_failure = Some(failure);
                return Ok(());
            }
        }
    }
    for pair in &plan.compare {
        let reference = if let Some(control) = control {
            recorded_reference(control, &pair.reference)
                .ok_or("missing captured native final artifact")?
                .clone()
        } else {
            artifact(&native, &pair.reference, true)?
        };
        let assembler = artifact(&asm, &pair.assembler, true)?;
        let equal = reference.length == assembler.length && reference.sha256 == assembler.sha256;
        record.comparisons.push(Pair {
            reference,
            assembler,
            equal,
        });
    }
    record.outcome = if record.comparisons.iter().all(|p| p.equal) {
        Outcome::Equivalent
    } else {
        Outcome::ArtifactMismatch
    };
    if matches!(record.outcome, Outcome::ArtifactMismatch) {
        record.first_failure = Some("final artifact bytes differ".into());
    }
    if tree_hash(&checkout, &files, &BTreeSet::new())? != original {
        return Err("acquired source changed during audit".into());
    }
    Ok(())
}

fn recorded_reference<'a>(record: &'a Record, path: &str) -> Option<&'a Artifact> {
    record
        .reference_steps
        .iter()
        .rev()
        .flat_map(|step| &step.outputs)
        .find(|artifact| artifact.path == path)
}

fn read_report(path: &Path) -> Result<Report, String> {
    if path.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("report exceeds size bound".into());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("report exceeds size bound".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid ecosystem report: {e}"))
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate_report(repo: &Path, manifest: &Manifest, report: &Report) -> Result<(), String> {
    let problems = super::validate(manifest);
    if !problems.is_empty() {
        return Err(format!(
            "invalid admission manifest: {}",
            problems.join("; ")
        ));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    if report.captured_unix_seconds > now + 300
        || report
            .reference_report_sha256
            .as_deref()
            .is_some_and(|hash| !valid_hash(hash))
        || report.reference_report_sha256.is_some()
            && report.reference_captured_unix_seconds.is_none()
        || report
            .reference_captured_unix_seconds
            .is_some_and(|time| time > report.captured_unix_seconds)
    {
        return Err("invalid capture date or reference replay provenance".into());
    }
    if report.schema != 1
        || report.manifest_sha256 != manifest_hash(repo)?
        || report.records.is_empty()
        || report.selected.is_empty()
        || report.records.len() != report.selected.len()
        || report.assembler.source_commit.len() != 40
        || !report
            .assembler
            .source_commit
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || !valid_hash(&report.assembler.executable_sha256)
        || !valid_hash(&report.assembler.cargo_lock_sha256)
        || !report.assembler.build.succeeded()
        || !report.assembler.version.succeeded()
    {
        return Err(
            "report is empty, incomplete, stale or has no successful Asm198x producer build".into(),
        );
    }
    let selected: BTreeSet<_> = report.selected.iter().map(String::as_str).collect();
    let records: BTreeSet<_> = report
        .records
        .iter()
        .map(|r| format!("{}/{}", r.project, r.target))
        .collect();
    if records.len() != report.records.len()
        || selected.len() != report.selected.len()
        || !selected.iter().all(|key| records.contains(*key))
    {
        return Err("duplicate or unmatched target records".into());
    }
    for record in &report.records {
        if record
            .issues
            .iter()
            .any(|issue| !issue.starts_with("https://github.com/") || !issue.contains("/issues/"))
        {
            return Err("invalid evidence issue link".into());
        }

        let (project, target) = targets(manifest)
            .find(|(p, t)| p.id == record.project && t.id == record.target)
            .ok_or("report names an unadmitted target")?;
        if record.source_commit != project.commit
            || record.plan_sha256 != target.build.as_ref().map(plan_hash).transpose()?
        {
            return Err(format!(
                "{}/{} has stale source or build plan",
                record.project, record.target
            ));
        }
        if matches!(
            record.outcome,
            Outcome::AssemblerRejected | Outcome::PostprocessFailure | Outcome::ArtifactMismatch
        ) {
            let plan = target
                .build
                .as_ref()
                .ok_or("interpreted Asm result without a complete plan")?;
            if record.reference_steps.len() != plan.reference.len()
                || record
                    .reference_steps
                    .iter()
                    .any(|s| !s.process.succeeded())
            {
                return Err("Asm result has no successful native control".into());
            }
        }
        if matches!(
            record.outcome,
            Outcome::Equivalent
                | Outcome::AssemblerRejected
                | Outcome::PostprocessFailure
                | Outcome::ArtifactMismatch
        ) {
            let plan = target
                .build
                .as_ref()
                .ok_or("interpreted result without a plan")?;
            let license = record
                .license
                .as_ref()
                .ok_or("native evidence has no retained licence notice")?;
            if license.spdx != project.license.spdx
                || license.path != project.license.path
                || license.text.is_empty()
                || license.sha256 != process::bytes_hash(license.text.as_bytes())
            {
                return Err("native evidence has inconsistent licence identity".into());
            }
            for pair in &plan.compare {
                let produced = record
                    .reference_steps
                    .iter()
                    .rev()
                    .flat_map(|step| &step.outputs)
                    .find(|artifact| artifact.path == pair.reference)
                    .ok_or("native control has no final artifact identity")?;
                if produced.length == 0 || !valid_hash(&produced.sha256) {
                    return Err("invalid native control artifact".into());
                }
            }
            for tool in &plan.tools {
                let identity = record
                    .tools
                    .get(&tool.name)
                    .ok_or("native control has no required tool identity")?;
                let text = format!("{}{}", identity.version.stdout, identity.version.stderr);
                if !valid_hash(&identity.sha256)
                    || !identity.version.succeeded()
                    || !tool
                        .versions
                        .iter()
                        .any(|wanted| text.lines().any(|line| line.trim() == wanted))
                {
                    return Err(
                        "native control tool identity is outside the reviewed versions".into(),
                    );
                }
            }
            if record
                .reference_steps
                .iter()
                .zip(&plan.reference)
                .any(|(actual, expected)| actual.stage != expected.stage)
            {
                return Err("native control stages do not match the reviewed plan".into());
            }
        }
        for artifact in record
            .reference_steps
            .iter()
            .chain(&record.assembler_steps)
            .flat_map(|step| &step.outputs)
        {
            if artifact.length == 0 || !valid_hash(&artifact.sha256) {
                return Err("invalid recorded build artifact".into());
            }
            if let Some(hex) = &artifact.hex {
                let bytes = decode_hex(hex)?;
                if bytes.len() as u64 != artifact.length
                    || process::bytes_hash(&bytes) != artifact.sha256
                {
                    return Err("recorded artifact bytes disagree with their identity".into());
                }
            }
        }
        if matches!(record.outcome, Outcome::Equivalent) {
            let plan = target.build.as_ref().ok_or("equivalence without a plan")?;
            if record.first_failure.is_some()
                || record.comparisons.len() != plan.compare.len()
                || record.reference_steps.len() != plan.reference.len()
                || record.assembler_steps.len() != plan.assembler.len()
                || record
                    .reference_steps
                    .iter()
                    .chain(&record.assembler_steps)
                    .any(|s| !s.process.succeeded())
                || record.tools.len() != plan.tools.len()
                || record
                    .source_tree_sha256
                    .as_deref()
                    .is_none_or(|h| !valid_hash(h))
            {
                return Err("equivalence is missing successful build evidence".into());
            }
            for (pair, expected) in record.comparisons.iter().zip(&plan.compare) {
                if !pair.equal
                    || pair.reference.path != expected.reference
                    || pair.assembler.path != expected.assembler
                    || pair.reference.length == 0
                    || pair.reference.length != pair.assembler.length
                    || !valid_hash(&pair.reference.sha256)
                    || pair.reference.sha256 != pair.assembler.sha256
                {
                    return Err("equivalence has inconsistent artifact identities".into());
                }
                let native = record
                    .reference_steps
                    .iter()
                    .rev()
                    .flat_map(|step| &step.outputs)
                    .find(|artifact| artifact.path == pair.reference.path)
                    .ok_or("missing native final artifact")?;
                if native.length != pair.reference.length || native.sha256 != pair.reference.sha256
                {
                    return Err("comparison does not identify the native final artifact".into());
                }
                if let Some(hex) = &pair.reference.hex {
                    let bytes = decode_hex(hex)?;
                    if bytes.len() as u64 != pair.reference.length
                        || process::bytes_hash(&bytes) != pair.reference.sha256
                    {
                        return Err(
                            "captured reference bytes disagree with artifact identity".into()
                        );
                    }
                }
            }
        } else if record.first_failure.is_none() {
            return Err("non-equivalent record has no first boundary".into());
        }
    }
    Ok(())
}
fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    if !hex.len().is_multiple_of(2) || !hex.is_ascii() {
        return Err("invalid captured hex".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn replay_inner(repo: &Path, manifest: &Manifest, args: &[String]) -> Result<(), String> {
    let [sources, reference, destination] = args else {
        return Err(
            "usage: ecosystem replay <sources> <reference-report.json> <new-results>".into(),
        );
    };
    let reference = Path::new(reference);
    let control = read_report(reference)?;
    validate_report(repo, manifest, &control)?;
    let controls: Vec<_> = control
        .records
        .iter()
        .filter(|r| {
            matches!(
                r.outcome,
                Outcome::Equivalent
                    | Outcome::AssemblerRejected
                    | Outcome::PostprocessFailure
                    | Outcome::ArtifactMismatch
            )
        })
        .collect();
    if !controls
        .iter()
        .any(|r| matches!(r.outcome, Outcome::Equivalent))
    {
        return Err("reference report contains no known successes to regression-gate".into());
    }
    let destination = Path::new(destination);
    if !destination.is_absolute() {
        return Err("result directory must be absolute".into());
    }
    std::fs::create_dir(destination).map_err(|e| format!("result directory must be new: {e}"))?;
    let logs = destination.join("logs");
    std::fs::create_dir(&logs).map_err(|e| e.to_string())?;
    let work = OwnedTree::new(destination.join("work"))?;
    let (assembler, program) = build_assembler(repo, &work.0, &logs)?;
    let mut report = Report {
        schema: 1,
        captured_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs(),
        manifest_sha256: manifest_hash(repo)?,
        assembler,
        reference_report_sha256: Some(process::digest(reference, 64 * 1024 * 1024)?.1),
        reference_captured_unix_seconds: Some(
            control
                .reference_captured_unix_seconds
                .unwrap_or(control.captured_unix_seconds),
        ),
        selected: controls
            .iter()
            .map(|r| format!("{}/{}", r.project, r.target))
            .collect(),
        records: Vec::new(),
    };
    let mut regressions = Vec::new();
    for native in controls {
        let (project, target) = targets(manifest)
            .find(|(p, t)| p.id == native.project && t.id == native.target)
            .ok_or("unknown reference target")?;
        let mut record = native.clone();
        record.reference_steps.clear();
        record.assembler_steps.clear();
        record.comparisons.clear();
        record.first_failure = None;
        record.outcome = Outcome::Unknown;
        if let Err(e) = target_audit(
            &AuditContext {
                repo,
                sources: Path::new(sources),
                work: &work.0,
                logs: &logs,
                program: &program,
            },
            project,
            target,
            &mut record,
            Some(native),
        ) {
            record.outcome = Outcome::InfrastructureUnavailable;
            record.first_failure = Some(e);
        }
        println!(
            "{}/{}: {:?}{}",
            record.project,
            record.target,
            record.outcome,
            record
                .first_failure
                .as_ref()
                .map_or(String::new(), |message| format!(" — {message}"))
        );
        if matches!(
            record.outcome,
            Outcome::InfrastructureUnavailable | Outcome::ReferenceFailure | Outcome::Unknown
        ) || matches!(native.outcome, Outcome::Equivalent)
            && !matches!(record.outcome, Outcome::Equivalent)
        {
            regressions.push(format!("{}/{}", record.project, record.target));
        }
        report.records.push(record);
        json_write(&destination.join("report.json"), &report)?;
    }
    validate_report(repo, manifest, &report)?;
    if !regressions.is_empty() {
        return Err(format!(
            "known success regressed or replay infrastructure failed: {}; inspect report.json",
            regressions.join(", ")
        ));
    }
    Ok(())
}

fn complete_cohort(manifest: &Manifest, report: &Report) -> Result<(), String> {
    let expected: BTreeSet<_> = targets(manifest).map(|(p, t)| key(p, t)).collect();
    let actual: BTreeSet<_> = report
        .records
        .iter()
        .map(|r| format!("{}/{}", r.project, r.target))
        .collect();
    if expected != actual {
        return Err("publishing requires the complete admitted cohort; a focused capture cannot replace known results".into());
    }
    Ok(())
}

fn ledger_inner(repo: &Path, manifest: &Manifest, args: &[String]) -> Result<(), String> {
    let [report_path, flag] = args else {
        return Err("usage: ecosystem ledger <report.json> --write|--check|--validate".into());
    };
    let report = read_report(Path::new(report_path))?;
    validate_report(repo, manifest, &report)?;
    if flag == "--validate" {
        println!("{} complete target records validated", report.records.len());
        return Ok(());
    }
    if flag != "--write" && flag != "--check" {
        return Err("unknown ledger action".into());
    }
    complete_cohort(manifest, &report)?;
    let committed = repo.join("ecosystem/results/current.json");
    if Path::new(report_path)
        .canonicalize()
        .map_err(|e| e.to_string())?
        != committed.canonicalize().map_err(|e| e.to_string())?
    {
        return Err("copy the reviewed capture to ecosystem/results/current.json before publishing its table".into());
    }
    let path = repo.join("docs/ecosystem-corpus.md");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let (updated, blocks) = crate::docs::regenerate(&text, &path, repo)?;
    if blocks == 0 {
        return Err("ecosystem table has no generated block".into());
    }
    if flag == "--check" {
        if updated != text {
            return Err("published ecosystem table is stale; regenerate it".into());
        }
    } else {
        std::fs::write(path, updated).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn markdown(repo: &Path) -> Result<String, String> {
    let manifest = super::read(repo)?;
    let report = read_report(&repo.join("ecosystem/results/current.json"))?;
    validate_report(repo, &manifest, &report)?;
    complete_cohort(&manifest, &report)?;
    Ok(table(&manifest, &report))
}

fn utc(seconds: u64) -> String {
    let mut days = seconds / 86400;
    let mut year = 1970u32;
    let leap = |year| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    while year < 10000 {
        let length = if leap(year) { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while month < 11 && days >= months[month] {
        days -= months[month];
        month += 1;
    }
    let time = seconds % 86400;
    format!(
        "{year:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        month + 1,
        days + 1,
        time / 3600,
        time / 60 % 60,
        time % 60
    )
}

fn table(manifest: &Manifest, report: &Report) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "\nCaptured at **{}**. Asm198x revision `{}`; executable SHA-256 `{}`.\n\nManifest SHA-256 `{}`. These results measure assembly artifact equivalence; emulator execution and real hardware are separate checks. Unrun targets remain unknown.\n\n| Project / target | Dialect | Result | First boundary / exact output |\n|---|---|---|---|\n",
        utc(report.captured_unix_seconds),
        report.assembler.source_commit,
        report.assembler.executable_sha256,
        report.manifest_sha256
    );
    if let Some(reference) = &report.reference_report_sha256 {
        let _ = writeln!(
            out,
            "Reference replay uses captured native control `{reference}` from **{}**. It does not refresh the native control date.\n",
            utc(report.reference_captured_unix_seconds.unwrap_or(0))
        );
    }
    for (project, target) in targets(manifest) {
        let row = report
            .records
            .iter()
            .find(|r| r.project == project.id && r.target == target.id);
        let (outcome, boundary) = row.map_or(("unknown".to_string(), "not run".to_string()), |r| {
            let outcome = serde_json::to_string(&r.outcome)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            let mut boundary = r.first_failure.clone().unwrap_or_else(|| {
                r.comparisons
                    .iter()
                    .map(|p| {
                        format!(
                            "`{}` ({} bytes, `{}`)",
                            p.reference.path, p.reference.length, p.reference.sha256
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            });
            for step in r.reference_steps.iter().chain(&r.assembler_steps) {
                boundary = boundary.replace(&format!("{}/", step.process.cwd), "");
            }
            for issue in &r.issues {
                let _ = write!(boundary, " [issue]({issue})");
            }
            (outcome, boundary)
        });
        let _ = writeln!(
            out,
            "| {} / `{}` | {} | {} | {} |",
            project.name,
            target.id,
            project.dialect,
            outcome,
            boundary
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('|', "\\|")
                .replace(['\n', '\r'], " ")
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo")
            .to_path_buf()
    }
    fn temporary() -> OwnedTree {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        OwnedTree::new(std::env::temp_dir().join(format!(
            "asm198x-ecosystem-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
        .expect("isolated tree")
    }
    fn successful_process() -> Process {
        Process {
            argv: vec!["test-owned-validator-control".into()],
            cwd: ".".into(),
            exit: Some(0),
            failure: None,
            stdout: String::new(),
            stderr: String::new(),
            elapsed_millis: 0,
        }
    }
    /// Synthetic validator data, never represented as native audit evidence.
    fn control(manifest: &Manifest) -> Report {
        let (project, target) = targets(manifest).next().expect("target");
        let plan = target.build.as_ref().expect("reviewed plan");
        let hash = process::bytes_hash(b"abc");
        let output = |path: &str, keep: bool| Artifact {
            path: path.into(),
            length: 3,
            sha256: hash.clone(),
            hex: keep.then(|| "616263".into()),
        };
        let tools = plan
            .tools
            .iter()
            .map(|t| {
                (
                    t.name.clone(),
                    Identity {
                        path: t.name.clone(),
                        sha256: hash.clone(),
                        version: Process {
                            stdout: t.versions[0].clone(),
                            ..successful_process()
                        },
                    },
                )
            })
            .collect();
        Report {
            schema: 1,
            captured_unix_seconds: 1,
            manifest_sha256: manifest_hash(&repo()).expect("manifest hash"),
            reference_report_sha256: None,
            reference_captured_unix_seconds: None,
            assembler: Assembler {
                source_commit: "1".repeat(40),
                cargo_lock_sha256: hash.clone(),
                executable_sha256: hash.clone(),
                build: successful_process(),
                version: successful_process(),
            },
            selected: vec![key(project, target)],
            records: vec![Record {
                project: project.id.clone(),
                target: target.id.clone(),
                source_commit: project.commit.clone(),
                source_tree_sha256: Some(hash.clone()),
                license: Some(LicenseEvidence {
                    spdx: project.license.spdx.clone(),
                    path: project.license.path.clone(),
                    text: "test-owned validator control".into(),
                    sha256: process::bytes_hash(b"test-owned validator control"),
                }),
                plan_sha256: Some(plan_hash(plan).expect("plan hash")),
                outcome: Outcome::Equivalent,
                first_failure: None,
                tools,
                adapters: vec![],
                reference_steps: plan
                    .reference
                    .iter()
                    .map(|s| StepResult {
                        stage: s.stage.clone(),
                        process: successful_process(),
                        outputs: s.outputs.iter().map(|path| output(path, false)).collect(),
                    })
                    .collect(),
                assembler_steps: plan
                    .assembler
                    .iter()
                    .map(|s| StepResult {
                        stage: s.stage.clone(),
                        process: successful_process(),
                        outputs: s.outputs.iter().map(|path| output(path, false)).collect(),
                    })
                    .collect(),
                comparisons: plan
                    .compare
                    .iter()
                    .map(|p| Pair {
                        reference: output(&p.reference, true),
                        assembler: output(&p.assembler, false),
                        equal: true,
                    })
                    .collect(),
                issues: vec![],
            }],
        }
    }
    #[test]
    fn evidence_rejects_empty_stale_duplicate_incomplete_and_wrong_byte_results() {
        let repo = repo();
        let manifest = super::super::read(&repo).expect("manifest");
        let report = control(&manifest);
        validate_report(&repo, &manifest, &report).expect("positive control");
        let mut changed = report.clone();
        changed.records.clear();
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.manifest_sha256 = "0".repeat(64);
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.records.push(changed.records[0].clone());
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.records[0].source_commit = "0".repeat(40);
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.records[0].tools.clear();
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.records[0].reference_steps[0].outputs.clear();
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.records[0].reference_steps.clear();
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.records[0].assembler_steps[0].process.exit = Some(1);
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.records[0].comparisons[0].reference.hex = Some("616264".into());
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.records[0].comparisons[0].assembler.sha256 = "0".repeat(64);
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report.clone();
        changed.records[0].comparisons.clear();
        assert!(validate_report(&repo, &manifest, &changed).is_err());
        let mut changed = report;
        changed.assembler.build.failure = Some("producer timed out".into());
        assert!(validate_report(&repo, &manifest, &changed).is_err());
    }
    #[test]
    fn missing_and_empty_artifacts_fail_and_source_mutations_change_the_tree() {
        let dir = temporary();
        let files = vec!["input.asm".to_string()];
        std::fs::write(dir.0.join("input.asm"), b" db 1\n").expect("source");
        let before = tree_hash(&dir.0, &files, &BTreeSet::new()).expect("source hash");
        assert!(artifact(&dir.0, "missing", true).is_err());
        std::fs::write(dir.0.join("empty"), []).expect("empty");
        assert!(artifact(&dir.0, "empty", true).is_err());
        std::fs::write(dir.0.join("input.asm"), b" db 2\n").expect("mutation");
        assert_ne!(
            tree_hash(&dir.0, &files, &BTreeSet::new()).expect("changed hash"),
            before
        );
        assert!(within(&dir.0, "../escape").is_err());
        assert!(within(&dir.0, "/escape").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn existing_and_dangling_symlinks_cannot_escape_workspaces() {
        use std::os::unix::fs::symlink;
        let dir = temporary();
        let outside = temporary();
        symlink(&outside.0, dir.0.join("escape")).expect("link");
        symlink(outside.0.join("absent"), dir.0.join("dangling")).expect("dangling link");
        assert!(within(&dir.0, "escape/output").is_err());
        assert!(within(&dir.0, "dangling/output").is_err());
    }
    #[test]
    fn replay_uses_the_finalised_artifact_and_supports_hash_only_controls() {
        let report =
            read_report(&repo().join("ecosystem/results/current.json")).expect("native capture");
        let gameboy = report
            .records
            .iter()
            .find(|r| r.project == "gbdev-gb-asm-tutorial")
            .expect("Game Boy control");
        let path = &gameboy.comparisons[0].reference.path;
        let finalised = recorded_reference(gameboy, path).expect("final ROM");
        assert_eq!(finalised.sha256, gameboy.comparisons[0].reference.sha256);
        let linked = gameboy
            .reference_steps
            .iter()
            .flat_map(|step| &step.outputs)
            .find(|artifact| &artifact.path == path)
            .expect("linked ROM");
        assert_ne!(
            linked.sha256, finalised.sha256,
            "finalisation must be observed"
        );
        let mut next = report
            .records
            .iter()
            .find(|r| r.project == "howprice-specnext-invaders")
            .expect("NEX control")
            .clone();
        let original = recorded_reference(&next, "bin/invaders.nex")
            .expect("native final NEX")
            .clone();
        for step in &mut next.reference_steps {
            for output in &mut step.outputs {
                output.hex = None;
            }
        }
        let identity = recorded_reference(&next, "bin/invaders.nex").expect("hash-only control");
        assert!(identity.hex.is_none());
        assert_eq!(identity.length, original.length);
        assert_eq!(identity.sha256, original.sha256);
    }

    #[test]
    fn focused_captures_cannot_replace_the_published_complete_cohort() {
        let manifest = super::super::read(&repo()).expect("manifest");
        let partial = control(&manifest);
        validate_report(&repo(), &manifest, &partial).expect("valid focused result");
        assert!(complete_cohort(&manifest, &partial).is_err());
        let complete = read_report(&repo().join("ecosystem/results/current.json"))
            .expect("retained native capture");
        validate_report(&repo(), &manifest, &complete).expect("valid complete capture");
        complete_cohort(&manifest, &complete).expect("positive publication control");
    }
    #[test]
    fn capture_dates_use_gregorian_utc_and_replay_keeps_the_native_date() {
        assert_eq!(utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc(951782400), "2000-02-29 00:00:00 UTC");
        let manifest = super::super::read(&repo()).expect("manifest");
        let mut report = control(&manifest);
        report.captured_unix_seconds = 86400;
        report.reference_report_sha256 = Some("1".repeat(64));
        report.reference_captured_unix_seconds = Some(1);
        validate_report(&repo(), &manifest, &report).expect("dated replay control");
        let text = table(&manifest, &report);
        assert!(text.contains("1970-01-02 00:00:00 UTC"));
        assert!(text.contains("1970-01-01 00:00:01 UTC"));
        report.reference_captured_unix_seconds = Some(86401);
        assert!(validate_report(&repo(), &manifest, &report).is_err());
    }
    #[test]
    fn native_hunk_comparisons_cannot_accidentally_select_raw_asm_output() {
        let manifest = super::super::read(&repo()).expect("manifest");
        let mut plan = targets(&manifest)
            .find(|(project, _)| project.dialect == "vasm")
            .expect("hunk target")
            .1
            .build
            .clone()
            .expect("plan");
        assert!(plan.problems().is_empty());
        for step in &mut plan.assembler {
            step.argv.retain(|arg| arg != "--exe" && arg != "--hunkexe");
        }
        assert!(
            plan.problems()
                .iter()
                .any(|problem| problem.contains("executable framing"))
        );
    }
    #[test]
    fn incomplete_plans_cannot_admit_shells_unproduced_outputs_or_empty_comparisons() {
        let manifest = super::super::read(&repo()).expect("manifest");
        let mut plan = targets(&manifest)
            .next()
            .expect("target")
            .1
            .build
            .clone()
            .expect("plan");
        assert!(plan.problems().is_empty());
        plan.compare.clear();
        assert!(!plan.problems().is_empty());
        let mut plan = targets(&manifest)
            .next()
            .expect("target")
            .1
            .build
            .clone()
            .expect("plan");
        plan.tools[0].name = "sh".into();
        assert!(!plan.problems().is_empty());
        let mut plan = targets(&manifest)
            .next()
            .expect("target")
            .1
            .build
            .clone()
            .expect("plan");
        plan.compare[0].reference = "pre-existing.rom".into();
        assert!(!plan.problems().is_empty());
        let mut plan = targets(&manifest)
            .next()
            .expect("target")
            .1
            .build
            .clone()
            .expect("plan");
        plan.assembler[0].timeout_seconds = 0;
        assert!(!plan.problems().is_empty());
    }
}
