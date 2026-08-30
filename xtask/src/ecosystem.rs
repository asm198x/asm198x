//! The real-world ecosystem corpus: pinned, licensed upstream source trees.
//!
//! This command deliberately fetches but never executes upstream code. Native
//! and Asm198x build invocations are data for the audit runner that follows;
//! reviewing admission and acquiring source must not imply permission to run a
//! third party's Makefile or scripts.

use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, Deserialize)]
struct Manifest {
    schema: u32,
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
struct Project {
    id: String,
    name: String,
    cpu: String,
    dialect: String,
    upstream: String,
    commit: String,
    license: License,
    targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
struct License {
    spdx: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct Target {
    id: String,
    cwd: String,
    sources: Vec<String>,
    reference: Vec<String>,
    outputs: Vec<String>,
}

pub(crate) fn run(repo: &Path, args: &[String]) -> ExitCode {
    let manifest = match read(repo) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("xtask ecosystem: {e}");
            return ExitCode::FAILURE;
        }
    };
    let problems = validate(&manifest);
    if !problems.is_empty() {
        eprintln!("ecosystem manifest is invalid:");
        for problem in problems {
            eprintln!("  {problem}");
        }
        return ExitCode::FAILURE;
    }

    match args.first().map(String::as_str) {
        Some("check") => {
            println!(
                "{} admitted project(s), {} build target(s), all pinned and licensed",
                manifest.projects.len(),
                manifest
                    .projects
                    .iter()
                    .map(|p| p.targets.len())
                    .sum::<usize>()
            );
            ExitCode::SUCCESS
        }
        Some("list") | None => {
            for project in &manifest.projects {
                println!(
                    "{}\t{}\t{}\t{}\t{} target(s)",
                    project.id,
                    project.cpu,
                    project.dialect,
                    project.name,
                    project.targets.len()
                );
            }
            ExitCode::SUCCESS
        }
        Some("fetch") => {
            let Some(destination) = args.get(1) else {
                eprintln!("usage: cargo xtask ecosystem fetch <directory> [project-id]");
                return ExitCode::FAILURE;
            };
            let selected = args.get(2).map(String::as_str);
            fetch(&manifest, Path::new(destination), selected)
        }
        Some("verify") => {
            let Some(destination) = args.get(1) else {
                eprintln!("usage: cargo xtask ecosystem verify <directory> [project-id]");
                return ExitCode::FAILURE;
            };
            verify(
                &manifest,
                Path::new(destination),
                args.get(2).map(String::as_str),
            )
        }
        Some(other) => {
            eprintln!(
                "xtask ecosystem: unknown action `{other}`\n\n\
                 usage: cargo xtask ecosystem check|list|fetch|verify <directory> [project-id]"
            );
            ExitCode::FAILURE
        }
    }
}

fn manifest_path(repo: &Path) -> PathBuf {
    repo.join("ecosystem/corpus.json")
}

fn read(repo: &Path) -> Result<Manifest, String> {
    let path = manifest_path(repo);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("cannot parse {}: {e}", path.display()))
}

fn validate(manifest: &Manifest) -> Vec<String> {
    let mut problems = Vec::new();
    if manifest.schema != 1 {
        problems.push(format!("schema must be 1, got {}", manifest.schema));
    }
    if manifest.projects.is_empty() {
        problems.push("projects must not be empty".into());
    }

    let known_dialects: BTreeSet<&str> = asm198x::dialect_table::DIALECTS
        .iter()
        .map(|d| d.name)
        .collect();
    let mut project_ids = BTreeSet::new();
    for project in &manifest.projects {
        let label = format!("project `{}`", project.id);
        if project.id.is_empty()
            || !project
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            problems.push(format!(
                "{label}: id must be lower-case ASCII, digits and hyphens"
            ));
        }
        if !project_ids.insert(project.id.as_str()) {
            problems.push(format!("{label}: duplicate id"));
        }
        if project.name.trim().is_empty() || project.cpu.trim().is_empty() {
            problems.push(format!("{label}: name and cpu must not be empty"));
        }
        if !known_dialects.contains(project.dialect.as_str()) {
            problems.push(format!("{label}: unknown dialect `{}`", project.dialect));
        }
        if !project.upstream.starts_with("https://github.com/")
            || !project.upstream.ends_with(".git")
        {
            problems.push(format!(
                "{label}: upstream must be an HTTPS GitHub clone URL ending in .git"
            ));
        }
        if project.commit.len() != 40 || !project.commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            problems.push(format!(
                "{label}: commit must be a full 40-character Git object id"
            ));
        }
        if project.license.spdx.trim().is_empty() || !safe_relative(&project.license.path) {
            problems.push(format!(
                "{label}: license needs an SPDX expression and safe relative path"
            ));
        }
        if project.targets.is_empty() {
            problems.push(format!("{label}: must admit at least one build target"));
        }
        let mut target_ids = BTreeSet::new();
        for target in &project.targets {
            let target_label = format!("{label}, target `{}`", target.id);
            if target.id.is_empty() || !target_ids.insert(target.id.as_str()) {
                problems.push(format!("{target_label}: id must be non-empty and unique"));
            }
            if !safe_relative(&target.cwd)
                || target.sources.is_empty()
                || target.sources.iter().any(|p| !safe_relative(p))
                || target.outputs.is_empty()
                || target.outputs.iter().any(|p| !safe_relative(p))
            {
                problems.push(format!(
                    "{target_label}: cwd, sources and outputs must be non-empty safe relative paths"
                ));
            }
            if target.reference.is_empty() || target.reference.iter().any(|a| a.is_empty()) {
                problems.push(format!("{target_label}: reference argv must not be empty"));
            }
        }
    }
    problems
}

fn safe_relative(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn fetch(manifest: &Manifest, destination: &Path, selected: Option<&str>) -> ExitCode {
    let projects: Vec<&Project> = manifest
        .projects
        .iter()
        .filter(|project| selected.is_none_or(|id| project.id == id))
        .collect();
    if projects.is_empty() {
        eprintln!(
            "xtask ecosystem: no project named `{}`",
            selected.unwrap_or_default()
        );
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::create_dir_all(destination) {
        eprintln!(
            "xtask ecosystem: cannot create {}: {e}",
            destination.display()
        );
        return ExitCode::FAILURE;
    }

    for project in projects {
        let checkout = destination.join(&project.id);
        if checkout.exists() {
            match git_output(&checkout, &["rev-parse", "HEAD"]) {
                Ok(head) if head.trim() == project.commit => {
                    if let Err(e) = verify_project(project, &checkout) {
                        eprintln!("xtask ecosystem: {e}");
                        return ExitCode::FAILURE;
                    }
                    println!("held and verified {} at {}", project.id, project.commit);
                    continue;
                }
                Ok(head) => {
                    eprintln!(
                        "xtask ecosystem: {} exists at {}, expected {}; refusing to overwrite it",
                        checkout.display(),
                        head.trim(),
                        project.commit
                    );
                }
                Err(e) => eprintln!("xtask ecosystem: {}: {e}", checkout.display()),
            }
            return ExitCode::FAILURE;
        }

        let clone = Command::new("git")
            .args([
                "clone",
                "--filter=blob:none",
                "--no-checkout",
                &project.upstream,
            ])
            .arg(&checkout)
            .status();
        if !matches!(clone, Ok(status) if status.success()) {
            eprintln!("xtask ecosystem: clone failed for {}", project.id);
            return ExitCode::FAILURE;
        }
        let checkout_status = Command::new("git")
            .current_dir(&checkout)
            .args(["checkout", "--detach", &project.commit])
            .status();
        if !matches!(checkout_status, Ok(status) if status.success()) {
            eprintln!("xtask ecosystem: checkout failed for {}", project.id);
            return ExitCode::FAILURE;
        }
        if let Err(e) = verify_project(project, &checkout) {
            eprintln!("xtask ecosystem: {e}");
            return ExitCode::FAILURE;
        }
        println!("fetched {} at {}", project.id, project.commit);
    }
    ExitCode::SUCCESS
}

fn verify(manifest: &Manifest, destination: &Path, selected: Option<&str>) -> ExitCode {
    let projects: Vec<&Project> = manifest
        .projects
        .iter()
        .filter(|project| selected.is_none_or(|id| project.id == id))
        .collect();
    if projects.is_empty() {
        eprintln!(
            "xtask ecosystem: no project named `{}`",
            selected.unwrap_or_default()
        );
        return ExitCode::FAILURE;
    }
    for project in projects {
        if let Err(e) = verify_project(project, &destination.join(&project.id)) {
            eprintln!("xtask ecosystem: {e}");
            return ExitCode::FAILURE;
        }
        println!("verified {} at {}", project.id, project.commit);
    }
    ExitCode::SUCCESS
}

fn verify_project(project: &Project, checkout: &Path) -> Result<(), String> {
    let head = git_output(checkout, &["rev-parse", "HEAD"])?;
    if head.trim() != project.commit {
        return Err(format!(
            "{} is at {}, expected {}",
            checkout.display(),
            head.trim(),
            project.commit
        ));
    }
    let license = checkout.join(&project.license.path);
    if !license.is_file() {
        return Err(format!(
            "{} has no recorded license file at {}",
            project.id,
            license.display()
        ));
    }
    for target in &project.targets {
        let cwd = checkout.join(&target.cwd);
        if !cwd.is_dir() {
            return Err(format!(
                "{} target {} has no working directory at {}",
                project.id,
                target.id,
                cwd.display()
            ));
        }
        for source in &target.sources {
            let path = checkout.join(source);
            if !path.exists() {
                return Err(format!(
                    "{} target {} has no declared input at {}",
                    project.id,
                    target.id,
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_manifest_is_valid() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo");
        let manifest = read(repo).expect("read committed manifest");
        assert_eq!(validate(&manifest), Vec::<String>::new());
    }

    #[test]
    fn paths_cannot_escape_the_checkout() {
        assert!(safe_relative("src/main.asm"));
        assert!(safe_relative("."));
        assert!(!safe_relative("../main.asm"));
        assert!(!safe_relative("/tmp/main.asm"));
        assert!(!safe_relative(""));
    }
}
