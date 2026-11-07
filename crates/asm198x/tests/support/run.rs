//! Running a reference tool, with a limit on how long it may take (#1039).
//!
//! A reference build with a bug can loop forever: Homebrew's ca65 V2.18 spun
//! for 15 hours on a two-line circular definition that the arbiter's V2.19
//! refuses in a second. An unbounded call turns that into a test that never
//! finishes and never says which source it is stuck on. Every reference run
//! goes through [`output`], which kills a tool that overruns and fails the
//! test with the tool's command line and the source it was given.

#![allow(dead_code)]

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// How long one reference tool run may take. Every reference assembler
/// finishes a probe, a curriculum unit or a pattern source in about a second,
/// so this is generous enough for a loaded CI runner and still ends a hung
/// run well inside the workflows' own job limits.
pub const LIMIT: Duration = Duration::from_secs(60);

/// Run `command` as [`Command::output`] does, within [`LIMIT`].
///
/// `source` names what the tool was given, for the failure message: the
/// probe's text, or the path of the file it builds.
///
/// # Errors
/// As [`Command::output`]: the tool could not be started.
///
/// # Panics
/// When the tool is still running after [`LIMIT`]. It is killed first, so a
/// hung reference never outlives the test.
pub fn output(command: &mut Command, source: &str) -> std::io::Result<Output> {
    output_within(command, source, LIMIT)
}

/// [`output`] with the limit given.
pub fn output_within(
    command: &mut Command,
    source: &str,
    limit: Duration,
) -> std::io::Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Drain both pipes while waiting, so a tool that writes more than a pipe
    // holds is not blocked on a full one and mistaken for a hung tool.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "{} did not exit within {}s, so it was killed. A reference tool \
                 that hangs gives no verdict; this one was given:\n{source}",
                describe(command),
                limit.as_secs_f64()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// The command line as a person would type it, quoted where it has spaces.
fn describe(command: &Command) -> String {
    let quote = |s: &std::ffi::OsStr| {
        let s = s.to_string_lossy();
        if s.contains(' ') {
            format!("'{s}'")
        } else {
            s.into_owned()
        }
    };
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(quote)
        .collect::<Vec<_>>()
        .join(" ")
}
