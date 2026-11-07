//! The time limit on reference tool runs (#1039), held with ordinary POSIX
//! tools standing in for a reference assembler.
#![cfg(unix)]

#[path = "support/run.rs"]
mod run;

use std::process::Command;
use std::time::{Duration, Instant};

/// A tool that never exits is killed at the limit, and the failure names the
/// command and the source it was given, instead of the test hanging.
#[test]
fn a_tool_that_never_exits_is_killed_and_named() {
    let started = Instant::now();
    let failure = std::panic::catch_unwind(|| {
        let _ = run::output_within(
            Command::new("sleep").arg("30"),
            "p=q\nq=p\n.byte p\n",
            Duration::from_millis(200),
        );
    })
    .expect_err("an overrunning tool must fail the test");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "it waited for the tool"
    );
    let message = failure
        .downcast_ref::<String>()
        .expect("a formatted panic message");
    assert!(message.contains("sleep 30 did not exit"), "{message}");
    assert!(message.contains("p=q\nq=p\n.byte p"), "{message}");
}

/// Within the limit, a run gives what `Command::output` gives: the status
/// and everything the tool wrote, even past what one pipe buffer holds.
#[test]
fn a_tool_that_exits_gives_its_status_and_output() {
    let script = "head -c 300000 /dev/zero; head -c 200000 /dev/zero >&2; exit 3";
    let bounded = run::output(Command::new("sh").args(["-c", script]), "-").expect("run");
    let plain = Command::new("sh")
        .args(["-c", script])
        .output()
        .expect("run");
    assert_eq!(bounded.status.code(), Some(3));
    assert_eq!(bounded.status, plain.status);
    assert_eq!(bounded.stdout.len(), 300_000);
    assert_eq!(bounded.stderr.len(), 200_000);
    assert_eq!(bounded.stdout, plain.stdout);
    assert_eq!(bounded.stderr, plain.stderr);
}

/// A tool that cannot be started is an error to the caller, as it is from
/// `Command::output`, so the suites still report a missing tool their way.
#[test]
fn a_tool_that_cannot_start_is_an_error() {
    assert!(run::output(&mut Command::new("asm198x-no-such-tool"), "-").is_err());
}
