//! A verdict section for each of ca65's 6502 relatives, from cc65's own
//! opcode tests.
//!
//! Asm198x claims a CPU only once that CPU has its own section of the
//! verdict corpus. The 6502X, 6502DTV, 65SC02, 65C02, W65C02, 65CE02, 4510
//! and 45GS02 have no disassembler here to drive a form audit, so their sections
//! come from an independent source instead: every instruction line of
//! cc65's `test/asm/opcodes/<cpu>-opcodes.s`, in
//! `fixtures/ca65-cpu-opcodes.json`.
//!
//! The M740 and SWEET 16 have no cc65 opcode test to read line by line, so
//! theirs come from their primary sources: every code of the 740 family
//! manual's opcode map, and every SWEET 16 instruction with every register,
//! in `fixtures/ca65-manual-forms.json`. Where ca65 contradicts the M740
//! manual, the line names the divergence and the verdict records ca65's
//! bytes beside it; where ca65 does not know an instruction the manual has,
//! the verdict records its refusal.
//!
//! The live tests run ca65 V2.19 on each line under `.setcpu` and `.org 0`
//! and records what it made. `verdict_replay` then checks every one through
//! the walk with no reference tools present.

use std::fs;
use std::process::Command;

mod support;

use verdict_corpus::{Outcome, Suite};

fn have(bin: &str) -> bool {
    Command::new(bin).output().is_ok()
}

/// The source each verdict records: the CPU, an address, and the line.
fn source(cpu: &str, line: &str) -> String {
    format!(".setcpu \"{cpu}\"\n.org 0\n {line}\n")
}

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-cpu-opcodes.json")).expect("fixture")
}

#[test]
fn every_cpu_has_its_lines() {
    let facts = fixture();
    let cpus = facts["cpus"].as_object().expect("cpus");
    let counts: Vec<(&str, usize)> = cpus
        .iter()
        .map(|(cpu, lines)| (cpu.as_str(), lines.as_array().map_or(0, Vec::len)))
        .collect();
    assert_eq!(
        counts,
        [
            ("4510", 261),
            ("45GS02", 336),
            ("6502DTV", 187),
            ("6502X", 219),
            ("65C02", 210),
            ("65CE02", 261),
            ("65SC02", 178),
            ("W65C02", 212),
        ]
    );
}

#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the verdicts"]
fn cc65s_opcode_tests_record_their_verdicts() {
    assert!(have("ca65") && have("ld65"), "ca65 and ld65 are required");
    let tmp = support::scratch::dir("ca65-cpu-opcodes");
    let cfg = tmp.join("flat.cfg");
    fs::write(&cfg, support::verdicts::CA65_FLAT_CFG).expect("config");
    let mut recorder = support::verdicts::Recorder::new();
    let facts = fixture();
    for (cpu, lines) in facts["cpus"].as_object().expect("cpus") {
        for line in lines.as_array().expect("lines") {
            let line = line.as_str().expect("a line");
            let text = source(cpu, line);
            let src = tmp.join("line.s");
            let obj = tmp.join("line.o");
            let bin = tmp.join("line.bin");
            fs::write(&src, &text).expect("source");
            let assembled = Command::new("ca65")
                .arg(&src)
                .arg("-o")
                .arg(&obj)
                .output()
                .expect("ca65");
            assert!(assembled.status.success(), "{cpu}: ca65 refused `{line}`");
            let linked = Command::new("ld65")
                .arg("-C")
                .arg(&cfg)
                .arg(&obj)
                .arg("-o")
                .arg(&bin)
                .output()
                .expect("ld65");
            assert!(linked.status.success(), "{cpu}: ld65 refused `{line}`");
            let bytes = fs::read(&bin).expect("image");
            recorder.record(
                support::verdicts::CaseRef {
                    suite: Suite::Probe,
                    cpu,
                    tool: "ca65",
                    dialect: "ca65",
                    case: format!("cc65 opcode test: {line}"),
                    source: &text,
                },
                Outcome::Bytes {
                    hex: verdict_corpus::encode_hex(&bytes),
                },
            );
        }
    }
    recorder.flush().expect("record the verdicts");
}

fn manual_forms() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ca65-manual-forms.json")).expect("fixture")
}

#[test]
fn the_manual_forms_cover_each_cpu() {
    let facts = manual_forms();
    let count = |cpu: &str| facts["cpus"][cpu].as_array().map_or(0, Vec::len);
    // The 740 manual's map assigns 231 codes; SWEET 16 has 15 register
    // operations for each of 16 registers, and 13 others.
    assert_eq!(count("M740"), 231);
    assert_eq!(count("SWEET16"), 15 * 16 + 13);
}

#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to record the verdicts"]
fn the_manual_forms_record_their_verdicts() {
    assert!(have("ca65") && have("ld65"), "ca65 and ld65 are required");
    let tmp = support::scratch::dir("ca65-manual-forms");
    let cfg = tmp.join("flat.cfg");
    fs::write(&cfg, support::verdicts::CA65_FLAT_CFG).expect("config");
    let mut recorder = support::verdicts::Recorder::new();
    let facts = manual_forms();
    for (cpu, lines) in facts["cpus"].as_object().expect("cpus") {
        for entry in lines.as_array().expect("lines") {
            let line = entry["line"].as_str().expect("a line");
            let text = source(cpu, line);
            let src = tmp.join("line.s");
            let obj = tmp.join("line.o");
            let bin = tmp.join("line.bin");
            fs::write(&src, &text).expect("source");
            let assembled = Command::new("ca65")
                .arg(&src)
                .arg("-o")
                .arg(&obj)
                .output()
                .expect("ca65");
            let case = support::verdicts::CaseRef {
                suite: Suite::Probe,
                cpu,
                tool: "ca65",
                dialect: "ca65",
                case: format!("manual form: {line}"),
                source: &text,
            };
            if !assembled.status.success() {
                let diagnostic = String::from_utf8_lossy(&assembled.stderr)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .replace(tmp.to_string_lossy().as_ref(), "<scratch>");
                recorder.record(case, Outcome::Rejected { diagnostic });
                continue;
            }
            let linked = Command::new("ld65")
                .arg("-C")
                .arg(&cfg)
                .arg(&obj)
                .arg("-o")
                .arg(&bin)
                .output()
                .expect("ld65");
            assert!(linked.status.success(), "{cpu}: ld65 refused `{line}`");
            let hex = verdict_corpus::encode_hex(&fs::read(&bin).expect("image"));
            let outcome = match entry["divergence"].as_str() {
                Some(divergence) => Outcome::Divergence {
                    divergence: divergence.to_owned(),
                    hex,
                },
                None => Outcome::Bytes { hex },
            };
            recorder.record(case, outcome);
        }
    }
    recorder.flush().expect("record the verdicts");
}
