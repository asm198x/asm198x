//! Bounded, shell-free execution with retained process evidence.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub(super) const MAX_LOG: u64 = 2 * 1024 * 1024;
pub(super) const MAX_ARTIFACT: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Process {
    pub argv: Vec<String>,
    pub cwd: String,
    pub exit: Option<i32>,
    pub failure: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub elapsed_millis: u128,
}

pub(super) fn digest(path: &Path, limit: u64) -> Result<(u64, String), String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if length > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()));
    }
    let mut reader = file.take(limit + 1);
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut count = 0;
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > limit {
            return Err("file grew past its size bound".into());
        }
        hash.update(&buffer[..n]);
    }
    if count != length {
        return Err("file changed length while hashing".into());
    }
    Ok((
        length,
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect(),
    ))
}

pub(super) fn bytes_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(super) fn resolve(name: &str) -> Result<PathBuf, String> {
    let candidates = if Path::new(name).components().count() > 1 {
        vec![PathBuf::from(name)]
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .flat_map(|dir| {
                let mut names = vec![dir.join(name)];
                if cfg!(windows) {
                    names.push(dir.join(format!("{name}.exe")));
                }
                names
            })
            .collect()
    };
    let path = candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("missing tool `{name}`"))?;
    // rustup and other executable proxies dispatch using argv[0]. Follow the
    // link when hashing its bytes, but keep the invoked name when executing.
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path))
    }
}

pub(super) fn run(
    argv: &[String],
    cwd: &Path,
    logs: &Path,
    name: &str,
    seconds: u64,
    watched: &[PathBuf],
) -> Result<Process, String> {
    if !cfg!(unix) {
        return Err("ecosystem execution requires Unix process-group support".into());
    }
    let stdout = logs.join(format!("{name}.stdout"));
    let stderr = logs.join(format!("{name}.stderr"));
    if stdout.exists() || stderr.exists() {
        return Err("process evidence already exists".into());
    }
    let overflow = Arc::new(AtomicBool::new(false));
    let capture = |reader: Option<Box<dyn Read + Send>>, overflow: Arc<AtomicBool>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut reader = reader.ok_or("missing process pipe".to_string())?;
            let mut buffer = [0; 8192];
            loop {
                let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                let keep = n.min((MAX_LOG as usize).saturating_sub(bytes.len()));
                bytes.extend_from_slice(&buffer[..keep]);
                if keep != n {
                    overflow.store(true, Ordering::Relaxed);
                }
            }
            Ok::<_, String>(bytes)
        })
    };
    let mut command = Command::new(argv.first().ok_or("empty command")?);
    command
        .args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let start = Instant::now();
    let mut result = Process {
        argv: argv.to_vec(),
        cwd: cwd.display().to_string(),
        exit: None,
        failure: None,
        stdout: String::new(),
        stderr: String::new(),
        elapsed_millis: 0,
    };
    match command.spawn() {
        Err(e) => {
            result.failure = Some(format!("cannot start command: {e}"));
            std::fs::write(&stdout, []).map_err(|e| e.to_string())?;
            std::fs::write(&stderr, []).map_err(|e| e.to_string())?;
        }
        Ok(mut child) => {
            let out = capture(
                child
                    .stdout
                    .take()
                    .map(|p| Box::new(p) as Box<dyn Read + Send>),
                overflow.clone(),
            );
            let err = capture(
                child
                    .stderr
                    .take()
                    .map(|p| Box::new(p) as Box<dyn Read + Send>),
                overflow.clone(),
            );
            loop {
                let too_large = overflow.load(Ordering::Relaxed)
                    || watched
                        .iter()
                        .any(|p| p.metadata().is_ok_and(|m| m.len() > MAX_ARTIFACT));
                if start.elapsed() > Duration::from_secs(seconds) || too_large {
                    result.failure = Some(
                        if too_large {
                            "process exceeded output size bound"
                        } else {
                            "process timed out"
                        }
                        .into(),
                    );
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                match child.try_wait() {
                    Ok(Some(status)) => {
                        result.exit = status.code();
                        break;
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                    Err(e) => {
                        result.failure = Some(format!("cannot wait for command: {e}"));
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
            // Kill the process tree we created, so a helper cannot outlive
            // its timed-out build. No unrelated process is selected.
            #[cfg(unix)]
            {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", "--", &format!("-{}", child.id())])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .status();
            }
            let out = out.join().map_err(|_| "stdout capture panicked")??;
            let err = err.join().map_err(|_| "stderr capture panicked")??;
            if overflow.load(Ordering::Relaxed) {
                result.failure = Some("process exceeded output size bound".into());
            }
            std::fs::write(&stdout, &out).map_err(|e| e.to_string())?;
            std::fs::write(&stderr, &err).map_err(|e| e.to_string())?;
        }
    }
    let read = |path: &Path| -> Result<String, String> {
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_LOG)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    };
    result.stdout = read(&stdout)?;
    result.stderr = read(&stderr)?;
    result.elapsed_millis = start.elapsed().as_millis();
    Ok(result)
}

impl Process {
    pub(super) fn succeeded(&self) -> bool {
        self.exit == Some(0) && self.failure.is_none()
    }
}

/// Metadata queries also obey time/log limits. Acquisition's existing Git
/// commands remain separate from this audit-only path.
pub(super) fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    use std::sync::atomic::AtomicUsize;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let git = resolve("git")?;
    let temporary = std::env::temp_dir().join(format!(
        "asm198x-ecosystem-meta-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    let mut argv = vec![
        git.display().to_string(),
        "-c".into(),
        "core.fsmonitor=false".into(),
    ];
    argv.extend(args.iter().map(|s| s.to_string()));
    let result = run(&argv, repo, &temporary, "git", 30, &[]);
    let cleanup = std::fs::remove_dir_all(&temporary);
    let result = result?;
    cleanup.map_err(|e| e.to_string())?;
    if !result.succeeded() {
        return Err(format!(
            "Git metadata failed: {}{}",
            result.stdout, result.stderr
        ));
    }
    Ok(if args.contains(&"-z") {
        result.stdout
    } else {
        result.stdout.trim().to_string()
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn directory(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("asm198x-process-{name}-{}", std::process::id()));
        std::fs::create_dir(&path).expect("isolated directory");
        path
    }
    #[test]
    fn executable_proxies_keep_the_invoked_name_and_hash_the_target_bytes() {
        use std::os::unix::fs::symlink;
        let dir = directory("proxy");
        let alias = dir.join("proxy-name");
        symlink("/bin/sh", &alias).expect("proxy");
        let resolved = resolve(alias.to_str().expect("path")).expect("resolve");
        assert_eq!(
            digest(&resolved, 128 * 1024 * 1024).expect("proxy hash"),
            digest(Path::new("/bin/sh"), 128 * 1024 * 1024).expect("target hash")
        );
        let result = run(
            &[
                resolved.display().to_string(),
                "-c".into(),
                "printf %s \"$0\"".into(),
            ],
            &dir,
            &dir,
            "proxy",
            5,
            &[],
        )
        .expect("run proxy");
        assert!(result.succeeded());
        assert_eq!(result.stdout, alias.display().to_string());
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
    #[test]
    fn digest_matches_independent_standard_vector_and_checks_the_bound() {
        let dir = directory("digest");
        let path = dir.join("input");
        std::fs::write(&path, b"abc").expect("input");
        assert_eq!(
            digest(&path, 3).expect("hash").1,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(digest(&path, 2).is_err());
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
    #[test]
    fn a_successful_process_with_excess_output_cannot_pass() {
        let dir = directory("overflow");
        // Test-owned script; build plans never admit a shell.
        let argv = vec![
            "/bin/sh".into(),
            "-c".into(),
            "dd if=/dev/zero bs=1048576 count=3 2>/dev/null".into(),
        ];
        let result = run(&argv, &dir, &dir, "output", 5, &[]).expect("capture");
        assert!(!result.succeeded());
        assert!(result.failure.expect("overflow").contains("size bound"));
        assert_eq!(result.stdout.len(), MAX_LOG as usize);
        assert_eq!(
            dir.join("output.stdout").metadata().expect("log").len(),
            MAX_LOG
        );
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
    #[test]
    fn timeouts_terminate_inherited_pipes_without_waiting_for_the_helper() {
        let dir = directory("timeout");
        let argv = vec!["/bin/sh".into(), "-c".into(), "sleep 20 & wait".into()];
        let start = Instant::now();
        let result = run(&argv, &dir, &dir, "timeout", 1, &[]).expect("capture");
        assert!(!result.succeeded());
        assert!(result.failure.expect("timeout").contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(5));
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
    #[test]
    fn nonzero_and_missing_executables_keep_the_failure_evidence() {
        let dir = directory("failure");
        let result = run(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "printf failure >&2; exit 7".into(),
            ],
            &dir,
            &dir,
            "exit",
            5,
            &[],
        )
        .expect("run");
        assert_eq!(result.exit, Some(7));
        assert!(!result.succeeded());
        assert_eq!(result.stderr, "failure");
        let result = run(
            &["/asm198x-missing-tool".into()],
            &dir,
            &dir,
            "missing",
            5,
            &[],
        )
        .expect("spawn evidence");
        assert!(!result.succeeded());
        assert!(
            result
                .failure
                .expect("spawn error")
                .contains("cannot start")
        );
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
