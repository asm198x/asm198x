//! Spec-conformance audit: every `isa` form, cross-checked against the real tool.
//!
//! The byte-identity harness ([`tests/curriculum`]) proves *curated programs*
//! match the reference assembler. This audit proves the **spec data itself** —
//! every `(mnemonic, mode) → opcode` in `isa` — against ground truth, so a
//! hand-authoring slip (a wrong opcode for a mode no curated program happens to
//! use) is caught.
//!
//! The trick reuses the disassemblers. For each form we synthesise its canonical
//! bytes (its opcode + filler operands), disassemble them with **our**
//! disassembler, then reassemble that text with the **reference** assembler and
//! require the bytes to come back identical. The existing round-trip reassembles
//! with *our* assembler (self-consistency); swapping in the reference makes the
//! reference the arbiter, so a wrong spec opcode shows up as a mismatch.
//!
//! Covers the three **form-based** specs (`mos6502`, `z80`, `mos65816`), which
//! is where opcode tables are largest and hand-authoring risk highest — the
//! 65816 set was authored this cycle. 6809 (`Kind`-based) and 68000
//! (field-based) use different spec shapes and need their own synthesis; their
//! round-trip is covered by the curriculum harness until a sweep-based audit is
//! added for them.
//!
//! `#[ignore]`d like the curriculum harness — it needs the reference tools. Run:
//!
//! ```text
//! cargo test --test conformance -- --ignored --nocapture
//! ```

use std::fs;
use std::path::Path;
use std::process::Command;

mod support;

use verdict_corpus::Suite;

fn have(bin: &str) -> bool {
    Command::new(bin).output().is_ok()
}

/// What a reference assembler did with the source it was handed.
///
/// The distinction matters because a corpus records *facts about source text*
/// (#61, R1). A tool that deliberately refused the source told us something
/// about that source, and the refusal is as much a fact as any byte string. A
/// tool that was missing, crashed, or could not be read from told us nothing
/// about the source at all — recording that would put a fiction in the corpus
/// and, worse, one that looks like a rejection.
///
/// Collapsing both into `None`, as this helper used to, makes the two
/// indistinguishable. Every call site that only asks "bytes or not" is
/// unaffected; recording is what needs them separated.
#[derive(Debug)]
enum RefOutcome {
    /// It assembled, producing these bytes.
    Bytes(Vec<u8>),
    /// It refused the source, with a diagnostic attributable to the text.
    Rejected {
        /// What the tool said, for the record and for the human reading it.
        diagnostic: String,
    },
    /// Nothing was learned about the source. **Never recordable.**
    NonVerdict {
        /// Why this run says nothing — a missing tool, a crash, an I/O failure.
        reason: String,
    },
}

impl RefOutcome {
    /// The bytes-or-nothing view. Exactly the old return value, so an assertion
    /// written against it cannot change meaning.
    fn bytes(self) -> Option<Vec<u8>> {
        match self {
            Self::Bytes(b) => Some(b),
            Self::Rejected { .. } | Self::NonVerdict { .. } => None,
        }
    }
}

/// Run a reference assembler over `text` and classify what happened. `build` is
/// given the input and output paths and must return the commands (already
/// configured) to run in `tmp`.
///
/// A non-zero exit is only a rejection if the tool **said** something: a silent
/// failure, or one killed by a signal, is a non-verdict. That is deliberately
/// conservative — misreading a crash as "the reference rejects this source"
/// would record a fact that is not true and then enforce it forever.
fn ref_outcome(
    tmp: &Path,
    text: &str,
    ext: &str,
    build: impl Fn(&Path, &Path) -> Vec<Command>,
) -> RefOutcome {
    let src = tmp.join(format!("conf.{ext}"));
    let out = tmp.join("conf.out");
    let _ = fs::remove_file(&out);
    if let Err(e) = fs::write(&src, text) {
        return RefOutcome::NonVerdict {
            reason: format!("could not write {}: {e}", src.display()),
        };
    }
    for mut cmd in build(&src, &out) {
        let finished = match cmd.current_dir(tmp).output() {
            Ok(o) => o,
            Err(e) => {
                return RefOutcome::NonVerdict {
                    reason: format!("could not run the tool: {e}"),
                };
            }
        };
        if finished.status.success() {
            continue;
        }
        // No exit code means a signal killed it — a crash never judges source.
        let Some(code) = finished.status.code() else {
            return RefOutcome::NonVerdict {
                reason: "the tool was killed by a signal".to_string(),
            };
        };
        let diagnostic = diagnostic_of(&finished);
        if diagnostic.is_empty() {
            return RefOutcome::NonVerdict {
                reason: format!("the tool exited {code} without saying why"),
            };
        }
        return RefOutcome::Rejected { diagnostic };
    }
    match fs::read(&out) {
        Ok(bytes) => RefOutcome::Bytes(bytes),
        Err(e) => RefOutcome::NonVerdict {
            reason: format!("could not read {}: {e}", out.display()),
        },
    }
}

/// What the tool said, preferring stderr and falling back to stdout — some
/// reference assemblers report errors on one, some on the other.
fn diagnostic_of(finished: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&finished.stderr).trim().to_string();
    if stderr.is_empty() {
        String::from_utf8_lossy(&finished.stdout).trim().to_string()
    } else {
        stderr
    }
}

/// The bytes-or-nothing view of [`ref_outcome`], for the assertions that only
/// ask whether the reference produced our bytes. U4 replaces these call sites
/// one suite at a time as it teaches each to record, and this goes with the
/// last of them.
fn ref_assemble(
    tmp: &Path,
    text: &str,
    ext: &str,
    build: impl Fn(&Path, &Path) -> Vec<Command>,
) -> Option<Vec<u8>> {
    ref_outcome(tmp, text, ext, build).bytes()
}

#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn spec_opcodes_match_reference() {
    let tmp = std::env::temp_dir().join("asm198x-conformance");
    fs::create_dir_all(&tmp).expect("temp dir");
    let mut fails: Vec<String> = Vec::new();
    let mut checked = 0usize;
    // Live mode records what each reference did, so the tool-free replay can
    // check the same facts on a machine that has none of them.
    let mut recorder = support::verdicts::Recorder::new();

    // --- 6502 / acme -------------------------------------------------------
    if have("acme") {
        for insn in isa::mos6502::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_6502(&bytes, 0x0800);
                let reference = ref_assemble(&tmp, &text, "a", |src, out| {
                    let mut c = Command::new("acme");
                    c.args(["-f", "cbm", "-o"]).arg(out).arg(src);
                    vec![c]
                });
                match reference {
                    // acme `cbm` output is a 2-byte load address then data.
                    Some(r) if r.len() >= 2 => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "6502",
                                tool: "acme",
                                dialect: "acme",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r[2..],
                        );
                        if r[2..] != bytes[..] {
                            fails.push(format!(
                                "6502 {} {}: ours {:02X?} vs acme {:02X?}",
                                insn.mnemonic,
                                form.mode,
                                bytes,
                                &r[2..]
                            ));
                        }
                    }
                    _ => fails.push(format!(
                        "6502 {} {}: acme rejected",
                        insn.mnemonic, form.mode
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `acme` not on PATH");
    }

    // --- Z80 / pasmo -------------------------------------------------------
    if have("pasmo") {
        for insn in isa::z80::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_z80(&bytes, 0x8000, false);
                let reference = ref_assemble(&tmp, &text, "z80", |src, out| {
                    let mut c = Command::new("pasmo");
                    c.arg(src).arg(out);
                    vec![c]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "Z80",
                                tool: "pasmo",
                                dialect: "pasmo",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "Z80 {} {}: ours {:02X?} vs pasmo {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "Z80 {} {}: pasmo rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(1).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `pasmo` not on PATH");
    }

    // --- 65816 / ca65 (6502 base + the extension) --------------------------
    if have("ca65") && have("ld65") {
        let cfg = tmp.join("flat816.cfg");
        fs::write(
            &cfg,
            "MEMORY { MAIN: start=$0000, size=$10000, fill=no, file=%O; }\n\
             SEGMENTS { CODE: load=MAIN, type=ro; }\n",
        )
        .expect("config");
        let sets: [&isa::InstructionSet; 2] = [&isa::mos6502::SET, &isa::mos65816::SET];
        for set in sets {
            for insn in set.instructions {
                for form in insn.forms {
                    // A 16-bit immediate needs the disassembler in 16-bit mode;
                    // prefix `rep #$30` so it tracks the width.
                    let mut bytes = if form.mode == "immediate16" {
                        vec![0xC2, 0x30]
                    } else {
                        Vec::new()
                    };
                    bytes.extend(form.exemplar());
                    let text = asm198x::listing_65816(&bytes, 0x0000);
                    let reference = ref_assemble(&tmp, &text, "s", |src, out| {
                        let obj = src.with_extension("o");
                        let mut a = Command::new("ca65");
                        a.args(["--cpu", "65816"]).arg(src).arg("-o").arg(&obj);
                        let mut l = Command::new("ld65");
                        l.arg("-C").arg(&cfg).arg(&obj).arg("-o").arg(out);
                        vec![a, l]
                    });
                    match reference {
                        Some(r) => {
                            checked += 1;
                            recorder.record_bytes(
                                support::verdicts::CaseRef {
                                    suite: Suite::Form,
                                    cpu: "65816",
                                    tool: "ca65",
                                    dialect: "ca65",
                                    case: format!("{} {}", insn.mnemonic, form.mode),
                                    source: &text,
                                },
                                &r,
                            );
                            if r != bytes {
                                fails.push(format!(
                                    "65816 {} {}: ours {:02X?} vs ca65 {:02X?}",
                                    insn.mnemonic, form.mode, bytes, r
                                ));
                            }
                        }
                        None => fails.push(format!(
                            "65816 {} {}: ca65 rejected `{}`",
                            insn.mnemonic,
                            form.mode,
                            text.lines().last().unwrap_or("").trim()
                        )),
                    }
                }
            }
        }
    } else {
        eprintln!("SKIP: `ca65`/`ld65` not on PATH (65816)");
    }

    // --- HuC6280 / ca65 (6502 base + the extension) ------------------------
    if have("ca65") && have("ld65") {
        let cfg = tmp.join("flatpce.cfg");
        fs::write(
            &cfg,
            "MEMORY { MAIN: start=$0000, size=$10000, fill=no, file=%O; }\n\
             SEGMENTS { CODE: load=MAIN, type=ro; }\n",
        )
        .expect("config");
        let sets: [&isa::InstructionSet; 2] = [&isa::mos6502::SET, &isa::huc6280::SET];
        for set in sets {
            for insn in set.instructions {
                for form in insn.forms {
                    let mut bytes: Vec<u8> = form.exemplar().collect();
                    // `tma` reads one MMU register, so ca65 requires a
                    // single-bit operand; the generic `$12` filler (two bits)
                    // is rejected. Use `$02` — the opcode is still verified.
                    // (`tam` may set several at once, so multi-bit is fine.)
                    if insn.mnemonic == "TMA" {
                        bytes[1] = 0x02;
                    }
                    let text = asm198x::listing_huc6280(&bytes, 0x0000);
                    let reference = ref_assemble(&tmp, &text, "s", |src, out| {
                        let obj = src.with_extension("o");
                        let mut a = Command::new("ca65");
                        a.args(["--cpu", "huc6280"]).arg(src).arg("-o").arg(&obj);
                        let mut l = Command::new("ld65");
                        l.arg("-C").arg(&cfg).arg(&obj).arg("-o").arg(out);
                        vec![a, l]
                    });
                    match reference {
                        Some(r) => {
                            checked += 1;
                            recorder.record_bytes(
                                support::verdicts::CaseRef {
                                    suite: Suite::Form,
                                    cpu: "huc6280",
                                    tool: "ca65",
                                    dialect: "ca65",
                                    case: format!("{} {}", insn.mnemonic, form.mode),
                                    source: &text,
                                },
                                &r,
                            );
                            if r != bytes {
                                fails.push(format!(
                                    "huc6280 {} {}: ours {:02X?} vs ca65 {:02X?}",
                                    insn.mnemonic, form.mode, bytes, r
                                ));
                            }
                        }
                        None => fails.push(format!(
                            "huc6280 {} {}: ca65 rejected `{}`",
                            insn.mnemonic,
                            form.mode,
                            text.lines().last().unwrap_or("").trim()
                        )),
                    }
                }
            }
        }
    } else {
        eprintln!("SKIP: `ca65`/`ld65` not on PATH (huc6280)");
    }

    // --- SM83 / rgbasm + rgblink (Game Boy) --------------------------------
    if have("rgbasm") && have("rgblink") {
        for insn in isa::sm83::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_sm83(&bytes, 0x0000);
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("o");
                    let mut a = Command::new("rgbasm");
                    a.arg("-o").arg(&obj).arg(src);
                    let mut l = Command::new("rgblink");
                    l.arg("-o").arg(out).arg(&obj);
                    vec![a, l]
                });
                match reference {
                    // rgblink pads the ROM, so compare only the emitted prefix.
                    Some(r) if r.len() >= bytes.len() => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "sm83",
                                tool: "rgbasm",
                                dialect: "rgbasm",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r[..bytes.len()],
                        );
                        if r[..bytes.len()] != bytes[..] {
                            fails.push(format!(
                                "sm83 {} {}: ours {:02X?} vs rgbasm {:02X?}",
                                insn.mnemonic,
                                form.mode,
                                bytes,
                                &r[..bytes.len()]
                            ));
                        }
                    }
                    _ => fails.push(format!(
                        "sm83 {} {}: rgbasm rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().last().unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `rgbasm`/`rgblink` not on PATH (sm83)");
    }

    // --- Intel 8080 / asl + p2bin ------------------------------------------
    if have("asl") && have("p2bin") {
        for insn in isa::i8080::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_i8080(&bytes, 0x0000);
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "8080",
                                tool: "asl",
                                dialect: "asl",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "8080 {} {}: ours {:02X?} vs asl {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "8080 {} {}: asl rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(2).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (8080)");
    }

    // --- Motorola 6800 / asl + p2bin ---------------------------------------
    if have("asl") && have("p2bin") {
        for insn in isa::m6800::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_m6800(&bytes, 0x0000);
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "6800",
                                tool: "asl",
                                dialect: "asl",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "6800 {} {}: ours {:02X?} vs asl {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "6800 {} {}: asl rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(2).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (6800)");
    }

    // --- RCA CDP1802 / asl + p2bin -----------------------------------------
    if have("asl") && have("p2bin") {
        for insn in isa::cdp1802::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_1802(&bytes, 0x0000);
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "1802",
                                tool: "asl",
                                dialect: "asl",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "1802 {} {}: ours {:02X?} vs asl {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "1802 {} {}: asl rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(2).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (1802)");
    }

    // --- Intel 8048 (MCS-48) / asl + p2bin ---------------------------------
    if have("asl") && have("p2bin") {
        for insn in isa::i8048::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_8048(&bytes, 0x0000);
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "8048",
                                tool: "asl",
                                dialect: "asl",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "8048 {} {}: ours {:02X?} vs asl {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "8048 {} {}: asl rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(2).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (8048)");
    }

    // --- ROM-less MCS-48 (8035/8039/8040) / asl + p2bin --------------------
    // The ROM-less parts share the 8048 encoding; the arbiter (`cpu 8039`)
    // agrees form-for-form, except the four BUS-port ops it forbids (the bus is
    // committed to external program fetch) — those we skip, matching the
    // dialect's own rejection (see `dialects::i8048`).
    if have("asl") && have("p2bin") {
        let bus_op = |mn: &str, mode: &str| {
            matches!(
                (mn, mode),
                ("ORL", "bus,#N") | ("ANL", "bus,#N") | ("OUTL", "bus,a") | ("INS", "a,bus")
            )
        };
        for insn in isa::i8048::SET.instructions {
            for form in insn.forms {
                if bus_op(insn.mnemonic, form.mode) {
                    continue;
                }
                let bytes: Vec<u8> = form.exemplar().collect();
                // Retarget the listing header at the ROM-less part.
                let text = asm198x::listing_8048(&bytes, 0x0000).replace("cpu 8048", "cpu 8039");
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "8039",
                                tool: "asl",
                                dialect: "asl",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "8039 {} {}: ours {:02X?} vs asl {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "8039 {} {}: asl rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(2).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (8039)");
    }

    // --- National SC/MP (INS8060) / asl + p2bin ----------------------------
    if have("asl") && have("p2bin") {
        for insn in isa::scmp::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_scmp(&bytes, 0x0000);
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "SC/MP",
                                tool: "asl",
                                dialect: "asl",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "SC/MP {} {}: ours {:02X?} vs asl {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "SC/MP {} {}: asl rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(2).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (SC/MP)");
    }

    // --- Fairchild F8 (3850) / asl + p2bin --------------------------------
    if have("asl") && have("p2bin") {
        for insn in isa::f8::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_f8(&bytes, 0x0000);
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "F8",
                                tool: "asl",
                                dialect: "asl",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "F8 {} {}: ours {:02X?} vs asl {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "F8 {} {}: asl rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(2).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (F8)");
    }

    // --- Signetics 2650 / asl + p2bin --------------------------------------
    if have("asl") && have("p2bin") {
        for insn in isa::s2650::SET.instructions {
            for form in insn.forms {
                let mut bytes: Vec<u8> = form.exemplar().collect();
                // The 2650 is big-endian; `synth` fills little-endian, so swap
                // the 2-byte absolute operand. (Big-endian also keeps the address
                // in the memory-reference ops' 13-bit direct range.)
                if form.operands.first().map(|o| o.kind) == Some(isa::OperandKind::Address) {
                    let p = form.opcode.len();
                    bytes.swap(p, p + 1);
                }
                let text = asm198x::listing_2650(&bytes, 0x0000);
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "2650",
                                tool: "asl",
                                dialect: "asl",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "2650 {} {}: ours {:02X?} vs asl {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "2650 {} {}: asl rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(2).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (2650)");
    }

    // --- TI TMS7000 / asl + p2bin ------------------------------------------
    if have("asl") && have("p2bin") {
        for insn in isa::tms7000::SET.instructions {
            for form in insn.forms {
                let bytes: Vec<u8> = form.exemplar().collect();
                let text = asm198x::listing_tms7000(&bytes, 0x0000);
                let reference = ref_assemble(&tmp, &text, "asm", |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
                        recorder.record_bytes(
                            support::verdicts::CaseRef {
                                suite: Suite::Form,
                                cpu: "TMS7000",
                                tool: "asl",
                                dialect: "asl",
                                case: format!("{} {}", insn.mnemonic, form.mode),
                                source: &text,
                            },
                            &r,
                        );
                        if r != bytes {
                            fails.push(format!(
                                "TMS7000 {} {}: ours {:02X?} vs asl {:02X?}",
                                insn.mnemonic, form.mode, bytes, r
                            ));
                        }
                    }
                    None => fails.push(format!(
                        "TMS7000 {} {}: asl rejected `{}`",
                        insn.mnemonic,
                        form.mode,
                        text.lines().nth(2).unwrap_or("").trim()
                    )),
                }
            }
        }
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (TMS7000)");
    }

    let recorded = recorder.flush().expect("write the verdict corpus");
    eprintln!("audited {checked} spec forms against the reference tools");
    eprintln!("recorded {recorded} new verdict(s)");
    assert!(
        fails.is_empty(),
        "{} spec mismatch(es):\n  {}",
        fails.len(),
        fails.join("\n  ")
    );
    assert!(checked > 0, "no audits ran — no tools present?");
}

/// Whether a disassembled line is a data fallback (not a decoded instruction).
fn is_data(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("fcb")
        || t.starts_with("dc.")
        || t.starts_with(".byte")
        || t.starts_with("defb")
        || t.starts_with("word ")
        || t.starts_with("byte ")
}

/// Sweep-based audit for the specs that are not form-based (`mos6809` is
/// `Kind`-based, `m68k` field-based): rather than iterate spec forms, feed
/// candidate byte sequences through **our** disassembler, keep the ones that
/// decode to a position-independent instruction (verified by disassembling at
/// two origins — this drops PC-relative branches, which can't be batched), then
/// concatenate them and reassemble the whole blob with the **reference** tool in
/// one call. The reference is the arbiter; a wrong opcode in the spec or bad
/// disassembler output shows up as a mismatch. On failure it localises by
/// reassembling each instruction alone.
/// Run a reference assembler over source, returning its bytes or nothing.
type Reassemble<'a> = dyn Fn(&str) -> Option<Vec<u8>> + 'a;

/// One CPU's sweep: what to walk, and how to render, arbitrate and record it.
///
/// Grouped because the sweep now needs the arbiter's identity as well as its
/// behaviour, and eight loose parameters is a call nobody can read.
struct SweepSpec<'a> {
    /// The CPU label, which is also its corpus file.
    name: &'a str,
    /// The issue tracking a known difference between our output and the
    /// arbiter's, where one exists. `None` where we expect to match everywhere.
    divergence: Option<u32>,
    /// The executable whose identity signs the verdicts.
    tool: &'a str,
    /// The syntax the listing is written in.
    dialect: &'a str,
    disasm: &'a dyn Fn(&[u8], u32) -> Vec<asm198x::Line>,
    listing: &'a dyn Fn(&[u8], u32) -> String,
    reassemble: &'a Reassemble<'a>,
    /// The invocation whose output is *recorded*, when it differs from the one
    /// that arbitrates.
    ///
    /// Usually the same invocation that arbitrates.
    ///
    /// Where our own output differs from it, the chunk is recorded as a
    /// **divergence** tagged with the issue tracking that difference, rather
    /// than as a plain fact. That is the only honest way to record the 68000:
    /// its sweep runs vasm with `-no-opt` so opcodes compare literally, and we
    /// sit between vasm's two configurations (#110) —
    ///
    /// | source | ours | vasm default | vasm `-no-opt` |
    /// |---|---|---|---|
    /// | `lea (a0),a0` | `41D0` | *deleted* | `41D0` |
    /// | `asl.w #1,d0` | `E340` | `D040` | `E340` |
    /// | `adda.w #$10,a0` | `41E8…` | `41E8…` | `D0FC…` |
    ///
    /// — so no invocation reproduces us. Recording the matching chunks as facts
    /// and the differing ones as tracked divergences keeps the CPU covered,
    /// pins the difference so it cannot drift unnoticed either way, and fails
    /// the moment #110 is fixed, which is when the marker should go.
    record_with: Option<&'a Reassemble<'a>>,
    skip: &'a dyn Fn(&str) -> bool,
}

/// Sweep an opcode space: disassemble every candidate, keep the
/// position-independent instructions, and require the reference to reproduce
/// them.
///
/// The instructions are grouped into **per-mnemonic chunks** rather than one
/// blob per CPU. One blob answers only "does this whole CPU round-trip", so a
/// single bad encoding fails the lot, and the corpus would hold one enormous
/// fact that changes whenever any instruction changes. A chunk fails alone,
/// localises to a mnemonic, and re-records only when that mnemonic's encodings
/// move.
///
/// Chunks key on (CPU, mnemonic, chunk source text) — never a positional index
/// (KTD5), so a disassembler change that reshuffles which instructions land in
/// a chunk re-keys it honestly instead of silently rewriting an unrelated fact.
/// Each chunk's listing is self-contained, carrying whatever header the CPU
/// needs (the Z8000's `supmode on`), so it reassembles on its own.
fn sweep(
    spec: SweepSpec<'_>,
    candidates: &[Vec<u8>],
    fails: &mut Vec<String>,
    recorder: &mut support::verdicts::Recorder,
) -> usize {
    let (oa, ob) = (0x1000u32, 0x4000u32);
    let mut instrs: Vec<Vec<u8>> = Vec::new();
    for cand in candidates {
        let la = (spec.disasm)(cand, oa);
        let Some(fa) = la.first() else { continue };
        if is_data(&fa.text) || (spec.skip)(&fa.text) {
            continue;
        }
        let lb = (spec.disasm)(cand, ob);
        match lb.first() {
            Some(fb) if fb.text == fa.text => instrs.push(fa.bytes.clone()),
            _ => {} // position-dependent (or undecodable at ob) — skip
        }
    }
    if instrs.is_empty() {
        return 0;
    }

    // Group by mnemonic — the first token of the disassembly. `BTreeMap` so the
    // chunk order is deterministic, which is what lets a replay map a byte
    // offset back to a case.
    let mut chunks: std::collections::BTreeMap<String, Vec<Vec<u8>>> =
        std::collections::BTreeMap::new();
    for instr in &instrs {
        let text = (spec.disasm)(instr, oa)
            .first()
            .map_or_else(String::new, |l| l.text.clone());
        let mnemonic = text
            .split_whitespace()
            .next()
            .unwrap_or("?")
            .to_ascii_lowercase();
        chunks.entry(mnemonic).or_default().push(instr.clone());
    }

    for (mnemonic, group) in &chunks {
        let blob: Vec<u8> = group.concat();
        let source = (spec.listing)(&blob, oa);
        if let Some(reference) = (spec.reassemble)(&source)
            && reference == blob
        {
            // Record what the invocation we claim parity with produces. `None`
            // means there is no such invocation for this CPU, so nothing is
            // recorded — the chunk is still arbitrated, just not replayable.
            let recorded = match spec.record_with {
                Some(record_with) => record_with(&source),
                None => None,
            };
            if let Some(recorded) = recorded {
                let case = support::verdicts::CaseRef {
                    suite: Suite::SweepChunk,
                    cpu: spec.name,
                    tool: spec.tool,
                    dialect: spec.dialect,
                    case: format!("sweep chunk `{mnemonic}` ({} instructions)", group.len()),
                    source: &source,
                };
                // Ask our own assembler the same question the tool-free replay
                // will. Where we already differ knowingly, record a tracked
                // divergence rather than a fact our next run would fail on.
                let ours = support::verdicts::assemble_form(spec.name, spec.dialect, &source)
                    .and_then(Result::ok);
                match spec.divergence {
                    Some(issue) if ours.as_deref() != Some(recorded.as_slice()) => recorder.record(
                        case,
                        verdict_corpus::Outcome::Divergence {
                            divergence: format!("issue-{issue}"),
                            hex: verdict_corpus::encode_hex(&recorded),
                        },
                    ),
                    _ => recorder.record_bytes(case, &recorded),
                }
            }
            continue;
        }
        // Localise inside the chunk: which instruction can the reference not
        // reproduce on its own?
        for instr in group {
            let text = (spec.disasm)(instr, oa)
                .first()
                .map_or_else(String::new, |l| l.text.clone());
            match (spec.reassemble)(&(spec.listing)(instr, oa)) {
                Some(b) if b == *instr => {}
                Some(b) => {
                    fails.push(format!(
                        "{}: {instr:02X?} -> ref {b:02X?} (disasm `{text}`, chunk `{mnemonic}`)",
                        spec.name
                    ));
                    break;
                }
                None => {
                    fails.push(format!(
                        "{}: ref rejected {instr:02X?} (disasm `{text}`, chunk `{mnemonic}`)",
                        spec.name
                    ));
                    break;
                }
            }
        }
    }
    instrs.len()
}

#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn spec_sweep_matches_reference() {
    let tmp = std::env::temp_dir().join("asm198x-sweep");
    fs::create_dir_all(&tmp).expect("temp dir");
    let mut fails: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut recorder = support::verdicts::Recorder::new();

    // --- 6809 / lwasm ------------------------------------------------------
    if have("lwasm") {
        let mut cands: Vec<Vec<u8>> = Vec::new();
        // Every primary opcode (and the $10/$11-prefixed pages); the byte after
        // the opcode doubles as a canonical postbyte ($84 = `,x`) for indexed.
        for prefix in [&[][..], &[0x10][..], &[0x11][..]] {
            for b in 0u16..256 {
                let mut v = prefix.to_vec();
                v.push(b as u8);
                v.extend_from_slice(&[0x84, 0x12, 0x34, 0x12, 0x56]);
                cands.push(v);
            }
        }
        // Every indexed postbyte for `lda ,r` (opcode $A6) — the postbyte space.
        for pb in 0u16..256 {
            cands.push(vec![0xA6, pb as u8, 0x12, 0x34, 0x56]);
        }
        let reasm = |src: &str| {
            ref_assemble(&tmp, src, "asm", |s, o| {
                let mut c = Command::new("lwasm");
                c.args(["--6809", "--raw", "-o"]).arg(o).arg(s);
                vec![c]
            })
        };
        checked += sweep(
            SweepSpec {
                name: "6809",
                divergence: None,
                tool: "lwasm",
                dialect: "lwasm",
                disasm: &|b, o| asm198x::disassemble_6809(b, o as u16),
                listing: &|b, o| asm198x::listing_6809(b, o as u16),
                reassemble: &reasm,
                record_with: Some(&reasm),
                skip: &|_| false,
            },
            &cands,
            &mut fails,
            &mut recorder,
        );
    } else {
        eprintln!("SKIP: `lwasm` not on PATH (6809 sweep)");
    }

    // --- 68000 / vasm ------------------------------------------------------
    if have("vasmm68k_mot") {
        // Every opcode word; canonical extension-word fillers follow.
        let cands: Vec<Vec<u8>> = (0u32..=0xFFFF)
            .map(|w| {
                vec![
                    (w >> 8) as u8,
                    w as u8,
                    0x00,
                    0x10,
                    0x00,
                    0x20,
                    0x00,
                    0x30,
                    0x00,
                    0x40,
                ]
            })
            .collect();
        let reasm = |src: &str| {
            ref_assemble(&tmp, src, "s", |s, o| {
                let mut c = Command::new("vasmm68k_mot");
                // `-no-opt`: the audit compares opcodes literally, so vasm must
                // not transform or delete instructions (e.g. its optimizer drops
                // `lea (a0),a0` as a redundant no-op).
                c.args(["-Fbin", "-no-opt", "-quiet", "-o"]).arg(o).arg(s);
                vec![c]
            })
        };
        checked += sweep(
            SweepSpec {
                name: "68000",
                divergence: Some(110),
                tool: "vasmm68k_mot",
                dialect: "vasm",
                disasm: &|b, o| asm198x::disassemble_68000(b, o),
                listing: &|b, o| asm198x::listing_68000(b, o),
                reassemble: &reasm,
                record_with: Some(&reasm),
                skip: &|_| false,
            },
            &cands,
            &mut fails,
            &mut recorder,
        );
    } else {
        eprintln!("SKIP: `vasmm68k_mot` not on PATH (68000 sweep)");
    }

    // --- DEC PDP-11 / asl + p2bin ------------------------------------------
    if have("asl") && have("p2bin") {
        // Every opcode word (little-endian), with canonical little-endian
        // extension-word fillers for the modes that carry them.
        let cands: Vec<Vec<u8>> = (0u32..=0xFFFF)
            .map(|w| vec![w as u8, (w >> 8) as u8, 0x10, 0x00, 0x20, 0x00, 0x30, 0x00])
            .collect();
        let reasm = |src: &str| {
            ref_assemble(&tmp, src, "asm", |s, o| {
                let obj = s.with_extension("p");
                let mut a = Command::new("asl");
                a.arg("-q").arg(s).arg("-o").arg(&obj);
                let mut b = Command::new("p2bin");
                b.arg(&obj).arg(o);
                vec![a, b]
            })
        };
        checked += sweep(
            SweepSpec {
                name: "PDP-11",
                divergence: None,
                tool: "asl",
                dialect: "asl",
                disasm: &|b, o| asm198x::disassemble_pdp11(b, o as u16),
                listing: &|b, o| asm198x::listing_pdp11(b, o as u16),
                reassemble: &reasm,
                record_with: Some(&reasm),
                skip: &|_| false,
            },
            &cands,
            &mut fails,
            &mut recorder,
        );
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (PDP-11 sweep)");
    }

    // --- TI TMS9900 / asl + p2bin ------------------------------------------
    if have("asl") && have("p2bin") {
        // Every opcode word (big-endian), with canonical big-endian
        // extension-word fillers for the symbolic-address modes.
        let cands: Vec<Vec<u8>> = (0u32..=0xFFFF)
            .map(|w| vec![(w >> 8) as u8, w as u8, 0x10, 0x00, 0x20, 0x00, 0x30, 0x00])
            .collect();
        let reasm = |src: &str| {
            ref_assemble(&tmp, src, "asm", |s, o| {
                let obj = s.with_extension("p");
                let mut a = Command::new("asl");
                a.arg("-q").arg(s).arg("-o").arg(&obj);
                let mut b = Command::new("p2bin");
                b.arg(&obj).arg(o);
                vec![a, b]
            })
        };
        checked += sweep(
            SweepSpec {
                name: "TMS9900",
                divergence: None,
                tool: "asl",
                dialect: "asl",
                disasm: &|b, o| asm198x::disassemble_tms9900(b, o as u16),
                listing: &|b, o| asm198x::listing_tms9900(b, o as u16),
                reassemble: &reasm,
                record_with: Some(&reasm),
                skip: &|_| false,
            },
            &cands,
            &mut fails,
            &mut recorder,
        );
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (TMS9900 sweep)");
    }

    // --- GI CP1610 / asl + p2bin (Intellivision) ---------------------------
    // Each 10-bit decle is a big-endian 16-bit word (top six bits zero), so the
    // candidate space is `0x000..=0x3FF`, each with a canonical big-endian filler
    // word for the direct-address and immediate modes' extension decle. The
    // single-decle register / shift groups ignore it; the memory modes consume it.
    // Branches (position-dependent) fall out of the two-origin check, so they are
    // covered by a round-trip test, not the sweep. See the crate `decisions/`.
    if have("asl") && have("p2bin") {
        let cands: Vec<Vec<u8>> = (0u32..=0x3FF)
            .map(|w| vec![(w >> 8) as u8, w as u8, 0x12, 0x34])
            .collect();
        let reasm = |src: &str| {
            ref_assemble(&tmp, src, "asm", |s, o| {
                let obj = s.with_extension("p");
                let mut a = Command::new("asl");
                a.arg("-q").arg(s).arg("-o").arg(&obj);
                let mut b = Command::new("p2bin");
                b.arg(&obj).arg(o);
                vec![a, b]
            })
        };
        checked += sweep(
            SweepSpec {
                name: "CP1610",
                divergence: None,
                tool: "asl",
                dialect: "asl",
                disasm: &|b, o| asm198x::disassemble_cp1610(b, o as u16),
                listing: &|b, o| asm198x::listing_cp1610(b, o as u16),
                reassemble: &reasm,
                record_with: Some(&reasm),
                skip: &|_| false,
            },
            &cands,
            &mut fails,
            &mut recorder,
        );
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (CP1610 sweep)");
    }

    // --- Zilog Z8000 / asl + p2bin (non-segmented Z8002) -------------------
    // The full non-segmented Z8002 instruction set (dyadic, program control,
    // single-operand, stack, shifts / rotates / sign-extends, bit ops, multiply
    // / divide, block / string, I/O, CPU control, and the TCC / LDK / RLDB /
    // RRDB / LDR cleanup);
    // groups not yet decoded fall to `word` data and are skipped. Shifts, the
    // dynamic bit form, the long mul/div immediates, and every block / string /
    // block-I/O op (its two-word form's second word has a zero top nibble the
    // fixed filler can't match) also fall to data here, so their round-trip is
    // the guard. Simple I/O is privileged, so the listing carries `supmode on`.
    // See decisions/z8000-staged-build.md.
    if have("asl") && have("p2bin") {
        // Every opcode word (big-endian), with a canonical big-endian
        // extension-word filler for the immediate / direct / indexed modes.
        let cands: Vec<Vec<u8>> = (0u32..=0xFFFF)
            .map(|w| vec![(w >> 8) as u8, w as u8, 0x12, 0x34])
            .collect();
        let reasm = |src: &str| {
            ref_assemble(&tmp, src, "asm", |s, o| {
                let obj = s.with_extension("p");
                let mut a = Command::new("asl");
                a.arg("-q").arg(s).arg("-o").arg(&obj);
                let mut b = Command::new("p2bin");
                b.arg(&obj).arg(o);
                vec![a, b]
            })
        };
        checked += sweep(
            SweepSpec {
                name: "Z8000",
                divergence: None,
                tool: "asl",
                dialect: "asl",
                disasm: &|b, o| asm198x::disassemble_z8000(b, o as u16),
                listing: &|b, o| asm198x::listing_z8000(b, o as u16),
                reassemble: &reasm,
                record_with: Some(&reasm),
                skip: &|_| false,
            },
            &cands,
            &mut fails,
            &mut recorder,
        );

        // --- Zilog Z8001 / asl (segmented) ---------------------------------
        // The same opcode space but with a canonical **long-form** segmented
        // address filler (`0x8000` + `0x1234`) after each word — so direct /
        // indexed operands decode as `<<0>>01234H` (and long immediates as the
        // 32-bit `0x80001234`); `asl` always emits long-form, so short-form
        // fillers could not round-trip. Verifies the widened memory operands
        // (`<<seg>>offset` addresses, `@RRn` pointers, `LDA` into a long pair,
        // block-I/O mixed pointers) across every instruction.
        let seg_cands: Vec<Vec<u8>> = (0u32..=0xFFFF)
            .map(|w| vec![(w >> 8) as u8, w as u8, 0x80, 0x00, 0x12, 0x34])
            .collect();
        checked += sweep(
            SweepSpec {
                name: "Z8001",
                divergence: None,
                tool: "asl",
                dialect: "asl",
                disasm: &|b, o| asm198x::disassemble_z8001(b, o as u16),
                listing: &|b, o| asm198x::listing_z8001(b, o as u16),
                reassemble: &reasm,
                record_with: Some(&reasm),
                skip: &|_| false,
            },
            &seg_cands,
            &mut fails,
            &mut recorder,
        );
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (Z8000 sweep)");
    }

    let recorded = recorder.flush().expect("write the verdict corpus");
    eprintln!("recorded {recorded} new sweep verdict(s)");
    eprintln!("swept {checked} decodable instructions against the reference tools");
    assert!(
        fails.is_empty(),
        "{} sweep mismatch(es):\n  {}",
        fails.len(),
        fails
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(checked > 0, "no sweeps ran — no tools present?");
}

/// A tiny deterministic LCG, so the fuzz corpus is reproducible.
struct Rng(u64);
impl Rng {
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u32() as usize) % n
    }
    fn byte(&mut self) -> u8 {
        self.next_u32() as u8
    }
}

/// Synthesise a form's bytes with a caller-supplied operand-byte source (random
/// for the fuzzer). Any byte is a valid immediate/displacement/offset, so the
/// result is always a decodable instruction.
fn synth_with(form: &isa::Form, fill: &mut impl FnMut() -> u8) -> Vec<u8> {
    let mut b = form.opcode.to_vec();
    for op in form.operands {
        for _ in 0..op.bytes {
            b.push(fill());
        }
    }
    b.extend_from_slice(form.suffix);
    b
}

/// Draw one random, decodable, position-independent instruction for the non-form
/// CPUs (6809, 68000), which have no `isa::Form` to synthesise from. Fill a
/// buffer with random bytes, disassemble it, and take the first line if it
/// decodes to a real instruction that reads the same at two origins — the same
/// filter the sweep uses to drop data bytes and position-dependent forms
/// (branches, PC-relative EA) that can't be freely concatenated. Returns `None`
/// if no decodable instruction turns up within the retry budget.
///
/// The random operand *values* are the point: where the sweep uses fixed filler
/// (`$1234`, `$84,…`), these exercise the size/sign boundaries that selection
/// logic turns on — 6809's 5/8/16-bit indexed offset, 68000 displacement
/// sign-extension — which fixed fillers never reach.
///
/// `canonical` gates the candidate to the byte-space an *assembler* reference can
/// actually arbitrate: an instruction only enters the corpus if our own
/// disasm→asm round-trip reproduces it. Random bytes routinely land on
/// *non-canonical* encodings (68000 brief-extension reserved/scale bits, `0(a0)`
/// vs `(a0)`) that decode fine but that no assembler emits — round-trip-to-bytes
/// is undefined there, so those bytes are out of scope for this method, not bugs.
/// (Testing the decoder *on* those patterns needs a decoder/emulator oracle, not
/// an assembler.)
fn random_insn(
    rng: &mut Rng,
    disasm: &dyn Fn(&[u8], u32) -> Vec<asm198x::Line>,
    canonical: &dyn Fn(&[u8]) -> bool,
) -> Option<Vec<u8>> {
    for _ in 0..64 {
        let buf: Vec<u8> = (0..8).map(|_| rng.byte()).collect();
        let la = disasm(&buf, 0x1000);
        let Some(fa) = la.first() else { continue };
        if is_data(&fa.text) {
            continue;
        }
        let lb = disasm(&buf, 0x4000);
        if lb.first().map(|l| l.text.as_str()) != Some(fa.text.as_str()) {
            continue; // position-dependent, or differs across origins
        }
        if !canonical(&fa.bytes) {
            continue; // non-canonical encoding — out of scope for round-trip
        }
        return Some(fa.bytes.clone());
    }
    None
}

/// Differential fuzz: random multi-instruction programs, disassembled then
/// reassembled by **both** our assembler and the reference. Both must reproduce
/// the original bytes — self-consistency *and* a ground-truth cross-check, over
/// random operand values and instruction sequences the curated corpus misses.
///
/// The form-based CPUs (6502, Z80): synthesised from `isa::Form`s. The non-form
/// CPUs (6809, 68000) are fuzzed by [`differential_fuzz_bytewise`] instead, which
/// synthesises instructions by disassembling random bytes. The 65816 is fuzzed by
/// neither: under `m`/`x` width a random instruction stream is genuinely
/// ambiguous to decode, so it is covered by the per-form audit and the curriculum
/// round-trip instead.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn differential_fuzz() {
    let tmp = std::env::temp_dir().join("asm198x-fuzz");
    fs::create_dir_all(&tmp).expect("temp dir");
    let mut fails: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut recorder = support::verdicts::Recorder::new();
    const PROGRAMS: usize = 100;
    const INSNS: usize = 6;

    // (label, forms, our-assemble, disassemble, reference-build)
    struct Cpu {
        name: &'static str,
        tool: &'static str,
    }
    let cpus = [
        Cpu {
            name: "6502",
            tool: "acme",
        },
        Cpu {
            name: "Z80",
            tool: "pasmo",
        },
    ];

    for cpu in cpus {
        if !have(cpu.tool) {
            eprintln!("SKIP fuzz: `{}` not on PATH", cpu.tool);
            continue;
        }
        let forms: Vec<&isa::Form> = match cpu.name {
            "6502" => isa::mos6502::SET
                .instructions
                .iter()
                .flat_map(|i| i.forms)
                .collect(),
            _ => isa::z80::SET
                .instructions
                .iter()
                .flat_map(|i| i.forms)
                .collect(),
        };
        let mut rng = Rng(0x1234_5678_9abc_def0);
        for p in 0..PROGRAMS {
            // Build a random program's bytes.
            let mut bytes = Vec::new();
            for _ in 0..INSNS {
                let form = forms[rng.below(forms.len())];
                bytes.extend(synth_with(form, &mut || rng.byte()));
            }
            // Disassemble, then require both assemblers to reproduce the bytes.
            let (text, ours) = match cpu.name {
                "6502" => {
                    let t = asm198x::listing_6502(&bytes, 0x0800);
                    let o = asm198x::assemble_acme(&t).map(|a| a.bytes);
                    (t, o)
                }
                _ => {
                    let t = asm198x::listing_z80(&bytes, 0x8000, false);
                    let o = asm198x::assemble_pasmo(&t).map(|a| a.bytes);
                    (t, o)
                }
            };
            match ours {
                Ok(o) if o == bytes => {}
                Ok(o) => fails.push(format!(
                    "{} prog {p}: our reasm differs ({} vs {} bytes)",
                    cpu.name,
                    o.len(),
                    bytes.len()
                )),
                Err(e) => fails.push(format!("{} prog {p}: our reasm error: {e}", cpu.name)),
            }
            let reference = ref_assemble(&tmp, &text, "src", |src, out| match cpu.name {
                "6502" => {
                    let mut c = Command::new("acme");
                    c.args(["-f", "cbm", "-o"]).arg(out).arg(src);
                    vec![c]
                }
                _ => {
                    let mut c = Command::new("pasmo");
                    c.arg(src).arg(out);
                    vec![c]
                }
            });
            // acme prepends a 2-byte load address.
            let reference = reference.map(|r| {
                if cpu.name == "6502" && r.len() >= 2 {
                    r[2..].to_vec()
                } else {
                    r
                }
            });
            match reference {
                Some(r) if r == bytes => {
                    checked += 1;
                    recorder.record_bytes(
                        support::verdicts::CaseRef {
                            suite: Suite::Fuzz,
                            cpu: cpu.name,
                            tool: cpu.tool,
                            dialect: cpu.tool,
                            case: format!("fuzz program {p}"),
                            source: &text,
                        },
                        &r,
                    );
                }
                Some(r) => fails.push(format!(
                    "{} prog {p}: reference reasm differs ({} vs {} bytes)",
                    cpu.name,
                    r.len(),
                    bytes.len()
                )),
                None => fails.push(format!(
                    "{} prog {p}: reference rejected disassembly",
                    cpu.name
                )),
            }
        }
    }

    let recorded = recorder.flush().expect("write the verdict corpus");
    eprintln!("fuzzed {checked} random programs (both assemblers vs the bytes)");
    eprintln!("recorded {recorded} new verdict(s)");
    assert!(
        fails.is_empty(),
        "{} fuzz mismatch(es):\n  {}",
        fails.len(),
        fails
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(checked > 0, "no fuzzing ran — no tools present?");
}

/// Differential fuzz for the **non-form** specs (6809 computed-operand, 68000
/// field-packed). The form-based fuzzer above can't drive these — they have no
/// `isa::Form` to synthesise from — so [`random_insn`] builds each instruction by
/// disassembling random bytes and keeping the decodable, position-independent
/// ones (the sweep's two-origin filter). We concatenate `INSNS` of them into a
/// program and require **both** our assembler and the reference to reproduce the
/// original bytes.
///
/// This is the sibling of [`spec_sweep_matches_reference`], not a duplicate: the
/// sweep walks the opcode space once with *fixed* filler operands; this walks
/// random *operand values* through multi-instruction programs, exercising the
/// size/sign-selection paths (6809 indexed-offset width, 68000 displacement
/// sign-extension) that a fixed filler can't reach. It reuses the same
/// `listing`/reference-command pairs the sweep proved out, so any mismatch is a
/// real disagreement, not a harness artefact.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn differential_fuzz_bytewise() {
    let tmp = std::env::temp_dir().join("asm198x-fuzz-bw");
    fs::create_dir_all(&tmp).expect("temp dir");
    let mut fails: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut scoped_out = 0usize;
    let mut recorder = support::verdicts::Recorder::new();
    const PROGRAMS: usize = 100;
    const INSNS: usize = 6;
    let oa = 0x1000u32;

    struct Cpu {
        name: &'static str,
        tool: &'static str,
    }
    let cpus = [
        Cpu {
            name: "6809",
            tool: "lwasm",
        },
        Cpu {
            name: "68000",
            tool: "vasmm68k_mot",
        },
    ];

    for cpu in cpus {
        if !have(cpu.tool) {
            eprintln!("SKIP fuzz: `{}` not on PATH", cpu.tool);
            continue;
        }
        let disasm = |b: &[u8], o: u32| -> Vec<asm198x::Line> {
            match cpu.name {
                "6809" => asm198x::disassemble_6809(b, o as u16),
                _ => asm198x::disassemble_68000(b, o),
            }
        };
        // Canonical for *us*: our disasm→asm round-trips to the same bytes.
        let canonical = |bytes: &[u8]| -> bool {
            let text = match cpu.name {
                "6809" => asm198x::listing_6809(bytes, oa as u16),
                _ => asm198x::listing_68000(bytes, oa),
            };
            let ours = match cpu.name {
                "6809" => asm198x::assemble_lwasm(&text).map(|a| a.bytes).ok(),
                _ => asm198x::assemble_vasm(&text).map(|a| a.bytes).ok(),
            };
            ours.as_deref() == Some(bytes)
        };
        // The reference-assembler command, reused for the whole-program check and
        // for per-instruction localisation on a mismatch.
        let ref_build = |s: &Path, o: &Path| -> Vec<Command> {
            match cpu.name {
                "6809" => {
                    let mut c = Command::new("lwasm");
                    c.args(["--6809", "--raw", "-o"]).arg(o).arg(s);
                    vec![c]
                }
                _ => {
                    let mut c = Command::new("vasmm68k_mot");
                    // `-no-opt`: same reason as the sweep — vasm must not
                    // transform or delete instructions, or the bytes won't match.
                    c.args(["-Fbin", "-no-opt", "-quiet", "-o"]).arg(o).arg(s);
                    vec![c]
                }
            }
        };
        let mut rng = Rng(0x0bad_f00d_dead_cafe);
        for p in 0..PROGRAMS {
            // Build a random program from decodable, position-independent insns.
            let mut blob = Vec::new();
            for _ in 0..INSNS {
                if let Some(insn) = random_insn(&mut rng, &disasm, &canonical) {
                    blob.extend(insn);
                }
            }
            if blob.is_empty() {
                continue;
            }
            let text = match cpu.name {
                "6809" => asm198x::listing_6809(&blob, oa as u16),
                _ => asm198x::listing_68000(&blob, oa),
            };
            // Our assembler must reproduce the bytes (self-consistency).
            let ours = match cpu.name {
                "6809" => asm198x::assemble_lwasm(&text).map(|a| a.bytes),
                _ => asm198x::assemble_vasm(&text).map(|a| a.bytes),
            };
            match ours {
                Ok(o) if o == blob => {}
                Ok(o) => fails.push(format!(
                    "{} prog {p}: our reasm differs ({} vs {} bytes)\n    {}",
                    cpu.name,
                    o.len(),
                    blob.len(),
                    text.replace('\n', " | ")
                )),
                Err(e) => fails.push(format!("{} prog {p}: our reasm error: {e}", cpu.name)),
            }
            // The reference must reproduce the whole program too (ground truth).
            if ref_assemble(&tmp, &text, "asm", ref_build).as_deref() == Some(&blob[..]) {
                checked += 1;
                recorder.record_bytes(
                    support::verdicts::CaseRef {
                        suite: Suite::Fuzz,
                        cpu: cpu.name,
                        tool: cpu.tool,
                        dialect: if cpu.name == "6809" { "lwasm" } else { "vasm" },
                        case: format!("byte-wise fuzz program {p}"),
                        source: &text,
                    },
                    &blob,
                );
                continue;
            }
            // Mismatch. Localise: does the reference reproduce each instruction on
            // its own? If one fails alone, the reference canonicalises that single
            // encoding differently from us (e.g. it masks an out-of-range static
            // bit number our more permissive assembler keeps) — outside what an
            // assembler round-trip can arbitrate, so scope it out, not a failure.
            // If every instruction reproduces alone but the program doesn't, the
            // composition itself diverges — a real bug.
            let diverging = disasm(&blob, oa).into_iter().find_map(|line| {
                let lt = match cpu.name {
                    "6809" => asm198x::listing_6809(&line.bytes, oa as u16),
                    _ => asm198x::listing_68000(&line.bytes, oa),
                };
                let reference = ref_assemble(&tmp, &lt, "asm", ref_build);
                (reference.as_deref() != Some(&line.bytes[..]))
                    .then_some((lt, line.bytes, reference))
            });
            if let Some((lt, ours_bytes, reference)) = diverging {
                scoped_out += 1;
                // Record the *instruction*, not the program. The quirk belongs
                // to one encoding, so keying it by content means two programs
                // that trip the same canonicalisation record one divergence
                // rather than two — and the id survives any reshuffle of the
                // generated programs, which a positional index would not.
                if let Some(reference) = reference {
                    recorder.record(
                        support::verdicts::CaseRef {
                            suite: Suite::Fuzz,
                            cpu: cpu.name,
                            tool: cpu.tool,
                            dialect: if cpu.name == "6809" { "lwasm" } else { "vasm" },
                            case: format!(
                                "canonicalisation of {}",
                                verdict_corpus::encode_hex(&ours_bytes)
                            ),
                            source: &lt,
                        },
                        verdict_corpus::Outcome::Divergence {
                            divergence: format!(
                                "canonicalisation-{}-{}",
                                cpu.name,
                                verdict_corpus::encode_hex(&ours_bytes)
                            ),
                            hex: verdict_corpus::encode_hex(&reference),
                        },
                    );
                }
            } else {
                fails.push(format!(
                    "{} prog {p}: reference composes the program differently\n    {}",
                    cpu.name,
                    text.replace('\n', " | ")
                ));
            }
        }
    }

    let recorded = recorder.flush().expect("write the verdict corpus");
    eprintln!("recorded {recorded} new verdict(s)");
    eprintln!(
        "byte-wise fuzzed {checked} random programs (6809/68000); \
         {scoped_out} scoped out (reference canonicalises a single instruction differently)"
    );
    assert!(
        fails.is_empty(),
        "{} fuzz mismatch(es):\n  {}",
        fails.len(),
        fails
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(checked > 0, "no fuzzing ran — no tools present?");
}

/// #66 and #90 — unwritten space, arbitrated against the real pipeline for the
/// whole asl family rather than the one chip the bugs were filed against.
///
/// `asl` reserves without writing, and `p2bin` materialises from the lowest
/// written address to the highest. So the interior of the written range is
/// filled (`$FF`), and both ends fall away: a trailing reservation is simply
/// absent, and a leading one moves where the image starts. All three shapes are
/// one rule read from different places, which is why they are probed together.
///
/// #85's lesson is why this sweeps the family: the reserve behaviour was
/// *likely* uniform across the asl chips, and "likely" is what punished us.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn unwritten_space_matches_p2bin_across_the_asl_family() {
    if !(have("asl") && have("p2bin")) {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH");
        return;
    }
    let tmp = std::env::temp_dir().join("asm198x-gaps");
    let _ = fs::create_dir_all(&tmp);

    type Assemble = fn(&str) -> Result<asm198x::AssemblyResult, asm198x::AsmError>;
    // (our dialect, asl's CPU name, the reserve directive). Note asl spells the
    // TMS7000 `TMS70C00`; the bare part number is not a CPU it knows.
    let family: &[(&str, &str, &str, Assemble)] = &[
        ("8080", "8080", "ds", asm198x::assemble_i8080),
        ("6800", "6800", "rmb", asm198x::assemble_m6800),
        ("1802", "1802", "ds", asm198x::assemble_1802),
        ("8048", "8048", "ds", asm198x::assemble_8048),
        ("scmp", "SC/MP", "ds", asm198x::assemble_scmp),
        ("2650", "2650", "ds", asm198x::assemble_2650),
        ("tms7000", "TMS70C00", "ds", asm198x::assemble_tms7000),
    ];

    let mut checked = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for (dialect, cpu, res, assemble) in family {
        for (shape, body) in [
            ("leading", format!(" org 0\n {res} 3\n db 9\n")),
            ("interior", format!(" org 0\n db 1\n {res} 2\n db 9\n")),
            ("trailing", format!(" org 0\n db 9\n {res} 2\n")),
            ("leading org gap", " org 0\n org 5\n db 9\n".to_string()),
        ] {
            let reference = ref_assemble(
                &tmp,
                &format!(" cpu {cpu}\n{body} end\n"),
                "asm",
                |src, out| {
                    let obj = src.with_extension("p");
                    let mut a = Command::new("asl");
                    a.arg("-q").arg(src).arg("-o").arg(&obj);
                    let mut b = Command::new("p2bin");
                    b.arg(&obj).arg(out);
                    vec![a, b]
                },
            );
            let Some(reference) = reference else {
                mismatches.push(format!("{dialect} {shape}: asl/p2bin rejected the probe"));
                continue;
            };
            let ours = assemble(&body).expect("assemble").bytes;
            checked += 1;
            if ours != reference {
                mismatches.push(format!(
                    "{dialect} {shape}: ours {ours:02X?} vs asl+p2bin {reference:02X?}"
                ));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} of {checked} unwritten-space probes diverge:\n  {}",
        mismatches.len(),
        mismatches.join("\n  ")
    );
    assert!(checked > 0, "no probes ran — no tools present?");
}

// ---------------------------------------------------------------------------
// `ref_outcome` classification (#61 U2).
//
// These are **not** `#[ignore]`d, and that is the point: they need no reference
// assembler, only a shell, so the rule separating a fact from a non-fact is
// itself checked on every PR. The suites that use the rule still need the real
// tools; the rule does not.
// ---------------------------------------------------------------------------

/// A scratch directory of its own, so these never race the real suites.
fn outcome_tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("asm198x-refoutcome-{tag}"));
    let _ = fs::create_dir_all(&dir);
    dir
}

/// A clean run that produces an output file is bytes, and the bytes are what
/// the tool wrote.
#[test]
fn a_clean_run_yields_the_bytes() {
    let tmp = outcome_tmp("ok");
    let outcome = ref_outcome(&tmp, "source", "asm", |_src, out| {
        let mut c = Command::new("sh");
        c.arg("-c")
            .arg(format!("printf 'AB' > '{}'", out.display()));
        vec![c]
    });
    assert!(
        matches!(&outcome, RefOutcome::Bytes(b) if b == b"AB"),
        "{outcome:?}"
    );
}

/// A tool that exits non-zero *and says why* has judged the source. That is a
/// verdict, and the diagnostic travels with it — without the diagnostic there
/// is nothing tying the refusal to the text.
#[test]
fn a_refusal_with_a_diagnostic_is_a_verdict() {
    let tmp = outcome_tmp("rejected");
    let outcome = ref_outcome(&tmp, "source", "asm", |_src, _out| {
        let mut c = Command::new("sh");
        c.arg("-c")
            .arg("echo 'error: value out of range' >&2; exit 1");
        vec![c]
    });
    match &outcome {
        RefOutcome::Rejected { diagnostic } => {
            assert_eq!(diagnostic, "error: value out of range");
        }
        other => panic!("expected a rejection, got {other:?}"),
    }
}

/// Some reference assemblers report errors on stdout rather than stderr, so a
/// diagnostic there counts too — otherwise those tools' rejections would all be
/// misfiled as crashes.
#[test]
fn a_diagnostic_on_stdout_counts_as_well() {
    let tmp = outcome_tmp("stdout");
    let outcome = ref_outcome(&tmp, "source", "asm", |_src, _out| {
        let mut c = Command::new("sh");
        c.arg("-c").arg("echo 'line 1: bad operand'; exit 2");
        vec![c]
    });
    assert!(
        matches!(&outcome, RefOutcome::Rejected { diagnostic } if diagnostic == "line 1: bad operand"),
        "{outcome:?}"
    );
}

/// A tool that fails silently has not judged anything. Reading that as a
/// rejection would record "the reference refuses this source" on no evidence,
/// and then enforce it forever.
#[test]
fn a_silent_failure_is_not_a_verdict() {
    let tmp = outcome_tmp("silent");
    let outcome = ref_outcome(&tmp, "source", "asm", |_src, _out| {
        let mut c = Command::new("sh");
        c.arg("-c").arg("exit 1");
        vec![c]
    });
    assert!(
        matches!(&outcome, RefOutcome::NonVerdict { reason } if reason.contains("without saying why")),
        "{outcome:?}"
    );
}

/// A crash is about the tool, never about the source.
#[test]
fn a_tool_killed_by_a_signal_is_not_a_verdict() {
    let tmp = outcome_tmp("signal");
    let outcome = ref_outcome(&tmp, "source", "asm", |_src, _out| {
        let mut c = Command::new("sh");
        c.arg("-c").arg("kill -9 $$");
        vec![c]
    });
    assert!(
        matches!(&outcome, RefOutcome::NonVerdict { reason } if reason.contains("signal")),
        "{outcome:?}"
    );
}

/// An absent tool is the ordinary state on a machine without the references —
/// which is most machines, and the whole reason the corpus exists.
#[test]
fn an_absent_tool_is_not_a_verdict() {
    let tmp = outcome_tmp("absent");
    let outcome = ref_outcome(&tmp, "source", "asm", |_src, _out| {
        vec![Command::new("asm198x-no-such-reference-tool")]
    });
    assert!(
        matches!(&outcome, RefOutcome::NonVerdict { reason } if reason.contains("could not run")),
        "{outcome:?}"
    );
}

/// A run that succeeds but writes nothing is a non-verdict, not empty bytes.
/// "The reference assembled this to zero bytes" is a very different claim from
/// "the reference produced no output file", and only one of them is true.
#[test]
fn a_missing_output_file_is_not_empty_bytes() {
    let tmp = outcome_tmp("nooutput");
    let outcome = ref_outcome(&tmp, "source", "asm", |_src, _out| {
        vec![Command::new("true")]
    });
    assert!(
        matches!(&outcome, RefOutcome::NonVerdict { reason } if reason.contains("could not read")),
        "{outcome:?}"
    );
}

/// The bytes-only view is exactly the old return value: everything that is not
/// bytes collapses to `None`, so no existing assertion changes meaning.
#[test]
fn the_bytes_view_collapses_every_non_byte_outcome() {
    assert_eq!(RefOutcome::Bytes(vec![1, 2]).bytes(), Some(vec![1, 2]));
    assert_eq!(
        RefOutcome::Rejected {
            diagnostic: "nope".to_string()
        }
        .bytes(),
        None
    );
    assert_eq!(
        RefOutcome::NonVerdict {
            reason: "absent".to_string()
        }
        .bytes(),
        None
    );
}

/// lwasm's five object-target words, arbitrated against lwasm rather than
/// against the manual.
///
/// `export`, `extdep`, `extern`, `external` and `import` are declared
/// [`Category::RefusedByReference`], which is a claim about *lwasm*: that it
/// refuses them itself when the output is a binary. Everything else in the
/// declared surface is a claim about us, checkable without a tool. This one is
/// not, so it is checked with the tool — and it is the claim most likely to be
/// wrong, because reading the manual would tell you these are ordinary
/// directives.
///
/// If lwtools ever accepts them under `--raw`, they stop being refusals and
/// become a gap, and this is what says so.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn lwasm_refuses_its_object_target_words_for_a_binary() {
    if !have("lwasm") {
        eprintln!("SKIP: `lwasm` not on PATH");
        return;
    }
    let tmp = std::env::temp_dir().join("asm198x-lwasm-objwords");
    let _ = fs::create_dir_all(&tmp);

    let mut wrong: Vec<String> = Vec::new();
    for word in ["export", "extdep", "extern", "external", "import"] {
        // With an operand and without: the answer is the same either way, so
        // neither shape can be the one that happened to be probed.
        for body in [format!(" {word}\n"), format!(" {word} foo\n")] {
            let source = format!(" org 0\n{body}foo: fcb 1\n");
            let outcome = ref_outcome(&tmp, &source, "asm", |src, out| {
                let mut c = Command::new("lwasm");
                c.arg("--6809").arg("--raw").arg("-o").arg(out).arg(src);
                vec![c]
            });
            match &outcome {
                RefOutcome::Rejected { diagnostic }
                    if diagnostic.contains("Only supported for object target") => {}
                other => wrong.push(format!("`{word}` with `{}`: {other:?}", body.trim())),
            }
            // And we refuse it too, naming lwasm's rule rather than a gap here.
            let err = asm198x::assemble_lwasm(&source).expect_err("we refuse it as well");
            let message = err.to_string();
            if !message.contains("object target") || message.contains("does not implement") {
                wrong.push(format!("`{word}` — ours reads wrong: {message}"));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} object-target probe(s) disagree:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// vasm's three import-side words, arbitrated against vasm.
///
/// `xref`, `import` and `nref` are declared [`Category::RefusedByReference`],
/// and unlike lwasm's five, vasm never says so: it answers `error 86: external
/// symbol <foo> must not be defined` when the name is defined here and `error
/// 3007: undefined symbol` when it is not. The refusal is the *pair* — no
/// program satisfies both — so the pair is what is checked. A probe of either
/// shape alone would read as an ordinary undefined-symbol rule.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn vasm_refuses_its_import_side_words_for_a_binary() {
    if !have("vasmm68k_mot") {
        eprintln!("SKIP: `vasmm68k_mot` not on PATH");
        return;
    }
    let tmp = std::env::temp_dir().join("asm198x-vasm-importwords");
    let _ = fs::create_dir_all(&tmp);

    let run = |source: &str| {
        ref_outcome(&tmp, source, "s", |src, out| {
            let mut c = Command::new("vasmm68k_mot");
            c.arg("-Fbin").arg("-no-opt").arg("-o").arg(out).arg(src);
            vec![c]
        })
    };

    let mut wrong: Vec<String> = Vec::new();
    for word in ["xref", "import", "nref"] {
        let defined = format!("\tsection code,code\n\t{word} foo\nfoo:\tdc.b 1\n");
        match &run(&defined) {
            RefOutcome::Rejected { diagnostic } if diagnostic.contains("must not be defined") => {}
            other => wrong.push(format!("`{word}` with the name defined: {other:?}")),
        }
        let undefined = format!("\tsection code,code\n\t{word} foo\n\tdc.b 1\n");
        match &run(&undefined) {
            RefOutcome::Rejected { diagnostic } if diagnostic.contains("undefined symbol") => {}
            other => wrong.push(format!("`{word}` with the name undefined: {other:?}")),
        }
        // And the seven that *can* be satisfied still can, so this is a fact
        // about these three rather than about visibility words in general.
        let err = asm198x::assemble_vasm(&undefined).expect_err("we refuse it too");
        if !err.to_string().contains("object file") {
            wrong.push(format!("`{word}` — ours reads wrong: {err}"));
        }
    }
    for word in [
        "xdef", "public", "global", "export", "entry", "weak", "extrn",
    ] {
        let defined = format!("\tsection code,code\n\t{word} foo\nfoo:\tdc.b 1\n");
        match &run(&defined) {
            RefOutcome::Bytes(b) if b == &[1] => {}
            other => wrong.push(format!("`{word}` with the name defined: {other:?}")),
        }
    }
    assert!(
        wrong.is_empty(),
        "{} import-side probe(s) disagree:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// ca65's `.forceimport`, arbitrated against ca65 **and** ld65.
///
/// It is declared [`Category::RefusedByReference`] on a claim no single tool
/// makes: defining the name is ca65's `Symbol 'zz' is already an import`, and
/// leaving it undefined is an ld65 unresolved external — with nothing
/// referencing it, which is what separates `.forceimport` from `.import`. The
/// pair is the refusal, so the pair is checked, and plain `.import` is checked
/// beside it to prove the difference is real.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn ca65_cannot_satisfy_forceimport_for_a_binary() {
    if !(have("ca65") && have("ld65")) {
        eprintln!("SKIP: `ca65`/`ld65` not on PATH");
        return;
    }
    let tmp = std::env::temp_dir().join("asm198x-ca65-forceimport");
    let _ = fs::create_dir_all(&tmp);
    let cfg = tmp.join("flat.cfg");
    fs::write(
        &cfg,
        "MEMORY { RAM: start=$8000, size=$100, file=%O; }\n\
         SEGMENTS { CODE: load=RAM, type=rw; }\n",
    )
    .expect("write the config");

    let run = |source: &str| {
        ref_outcome(&tmp, source, "s", |src, out| {
            let obj = src.with_extension("o");
            let mut a = Command::new("ca65");
            a.arg("-o").arg(&obj).arg(src);
            let mut b = Command::new("ld65");
            b.arg("-C").arg(&cfg).arg("-o").arg(out).arg(&obj);
            vec![a, b]
        })
    };

    let mut wrong: Vec<String> = Vec::new();
    match &run(".segment \"CODE\"\n.forceimport zz\nzz: .byte 1\n") {
        RefOutcome::Rejected { diagnostic } if diagnostic.contains("already an import") => {}
        other => wrong.push(format!("forceimport with the name defined: {other:?}")),
    }
    match &run(".segment \"CODE\"\n.forceimport zz\n.byte 1\n") {
        RefOutcome::Rejected { diagnostic } if diagnostic.contains("nresolved external") => {}
        other => wrong.push(format!("forceimport with the name undefined: {other:?}")),
    }
    // Plain `.import`, unreferenced, links — so this is a fact about the
    // `force`, not about imports.
    match &run(".segment \"CODE\"\n.import zz\n.byte 1\n") {
        RefOutcome::Bytes(b) if b == &[1] => {}
        other => wrong.push(format!("plain import, unreferenced: {other:?}")),
    }
    // And `.export` of a name nothing defines is refused by ca65 itself, which
    // is the check the export words carry here.
    match &run(".segment \"CODE\"\n.export nope\n.byte 1\n") {
        RefOutcome::Rejected { diagnostic } if diagnostic.contains("never defined") => {}
        other => wrong.push(format!("export of an undefined name: {other:?}")),
    }

    let err = asm198x::assemble_ca65(".segment \"CODE\"\n.forceimport zz\n.byte 1\n")
        .expect_err("we refuse it too");
    if !err.to_string().contains("linker resolves it") {
        wrong.push(format!("ours reads wrong: {err}"));
    }

    assert!(
        wrong.is_empty(),
        "{} forceimport probe(s) disagree:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// Another entry sharing a base means the two mnemonics are the same
/// encoding, and a disassembler can only print one of them. Derived from the
/// spec rather than listed here, so it cannot go stale.
fn shares_base(base: u16, mnemonic: &str) -> Option<&'static str> {
    isa::pdp11::INSTRUCTIONS
        .iter()
        .find(|i| i.base == base && !i.mnemonic.eq_ignore_ascii_case(mnemonic))
        .map(|i| i.mnemonic)
}

/// Given a mnemonic and a mode, the entry's base and the exemplar word the
/// spec states for it.
type BaseLookup<'a> = &'a dyn Fn(&str, &str) -> Option<(u16, u16)>;

/// An exemplar word as bytes, with the extension-word fillers behind it.
fn exemplar_bytes(word: u16, big_endian: bool, filler: &[u8]) -> Vec<u8> {
    let mut b = if big_endian {
        vec![(word >> 8) as u8, word as u8]
    } else {
        vec![word as u8, (word >> 8) as u8]
    };
    b.extend_from_slice(filler);
    b
}

/// The disassembly up to and including the line that names `mnemonic`.
///
/// The exemplar carries filler bytes so a multi-word instruction has operands
/// to consume, and whatever the instruction leaves over disassembles to `word`
/// data directives. `asl` has no `word` directive for this CPU, so it rejects
/// the file over the leftovers rather than over the instruction under audit.
/// Cutting the tail hands the reference exactly the instruction we are asking
/// it about.
fn trim_after_instruction(source: &str, mnemonic: &str) -> Option<String> {
    let mut out = String::new();
    for line in source.lines() {
        let head = line.split_whitespace().next();
        // A data directive before the instruction means the exemplar word did
        // not decode at all: the mnemonic further down belongs to the filler,
        // not to this row. That is an unplaced row, not a reference rejection.
        if head.is_some_and(|w| w.eq_ignore_ascii_case("word")) {
            return None;
        }
        out.push_str(line);
        out.push('\n');
        if head.is_some_and(|w| w.eq_ignore_ascii_case(mnemonic)) {
            out.push_str("\tend\n");
            return Some(out);
        }
    }
    None
}

/// Does this disassembly name `mnemonic` as an opcode?
fn names_row(source: &str, mnemonic: &str) -> bool {
    source.lines().any(|l| {
        l.split_whitespace()
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case(mnemonic))
    })
}

/// Another entry sharing this base — the mnemonic is an alias, and the
/// disassembler can only print one of them.
fn alias_of(cpu: &str, base: u16, mnemonic: &str) -> Option<&'static str> {
    match cpu {
        "PDP-11" => shares_base(base, mnemonic),
        _ => None,
    }
}

/// One word CPU's form audit: every row the spec declares, put to `asl`.
///
/// A word CPU packs its operands into fields of a single opcode word, so the
/// representative bytes for a row are the entry's `base` — the opcode word with
/// its fields zeroed — followed by the canonical extension-word fillers the
/// sweep already uses. Zeroed fields are a valid encoding: register zero, a
/// zero displacement, a branch to itself.
///
/// **What this covers that the sweep does not.** The sweep disassembles each
/// candidate at two origins and keeps only what reads the same at both, so
/// every PC-relative instruction is dropped from it by construction — 17
/// branches and `SOB` on the PDP-11, 13 jumps on the TMS9900. Those rows are
/// in no sweep chunk, and until this they were in no verdict at all.
///
/// The disassembly is checked to name the row's own mnemonic before anything
/// is recorded. A row whose synthesised bytes decode to something else would
/// otherwise arbitrate a different instruction under this row's name, which is
/// worse than not arbitrating it.
#[allow(clippy::too_many_arguments)]
fn word_cpu_form_audit(
    cpu: &str,
    rows: impl Iterator<Item = isa::Row>,
    base_of: BaseLookup<'_>,
    big_endian: bool,
    filler: &[u8],
    listing: &dyn Fn(&[u8], u32) -> String,
    tmp: &std::path::Path,
    recorder: &mut support::verdicts::Recorder,
    fails: &mut Vec<String>,
) -> usize {
    let mut checked = 0usize;
    for row in rows {
        let Some((base, base_word)) = base_of(row.mnemonic, row.mode) else {
            fails.push(format!(
                "{cpu} {} {}: no base for the row",
                row.mnemonic, row.mode
            ));
            continue;
        };
        // The spec states the field value that makes this a legal encoding —
        // zero for almost everything, register-deferred for the two PDP-11
        // instructions that cannot take a register operand. Stated rather than
        // searched for, so a wrong value fails the `names_row` check below
        // instead of being hunted around.
        //
        // An alias is the exception the spec cannot state, because it is not
        // about encoding: `BHIS` shares its base with `BCC`, and a
        // disassembler can only print one of a pair. The encoding is still the
        // row's claim and is still arbitrated, under the spelling that prints.
        let want = if names_row(
            &listing(&exemplar_bytes(base_word, big_endian, filler), 0x1000),
            row.mnemonic,
        ) {
            row.mnemonic
        } else {
            alias_of(cpu, base, row.mnemonic).unwrap_or(row.mnemonic)
        };
        let bytes = exemplar_bytes(base_word, big_endian, filler);

        // `listing` writes a whole assembler source file — the `cpu` line, the
        // origin and the instruction — which is what the sweep hands to the
        // reference, so it is what this hands over too.
        let source = listing(&bytes, 0x1000);
        // The disassembly must name this row, or the audit would arbitrate
        // some other instruction under this row's name.
        if !names_row(&source, want) {
            fails.push(format!(
                "{cpu} {} {}: base {base:#06X} disassembles to something else:\n{source}",
                row.mnemonic, row.mode
            ));
            continue;
        }
        let reference = ref_assemble(tmp, &source, "asm", |src, out| {
            let obj = src.with_extension("p");
            let mut a = Command::new("asl");
            a.arg("-q").arg(src).arg("-o").arg(&obj);
            let mut b = Command::new("p2bin");
            b.arg(&obj).arg(out);
            vec![a, b]
        });
        let Some(reference) = reference else {
            fails.push(format!(
                "{cpu} {} {}: asl rejected our own disassembly:\n{source}",
                row.mnemonic, row.mode
            ));
            continue;
        };
        checked += 1;
        recorder.record_bytes(
            support::verdicts::CaseRef {
                suite: Suite::Form,
                cpu,
                tool: "asl",
                dialect: "asl",
                case: format!("{} {}", row.mnemonic, row.mode),
                source: &source,
            },
            &reference,
        );
        // Only the opcode word is this row's claim; the fillers are ours.
        let ours = &bytes[..2];
        if reference.len() < 2 || &reference[..2] != ours {
            fails.push(format!(
                "{cpu} {} {}: ours {ours:02X?} vs asl {:02X?}",
                row.mnemonic,
                row.mode,
                &reference[..reference.len().min(2)]
            ));
        }
    }
    checked
}

/// The 6809 form audit: every row the spec declares, put to `lwasm`.
///
/// The `Form` specs have had this since the beginning; the 6809 could not,
/// because both the audit and its denominator iterated `Form` and this spec
/// authors `Kind`. It enumerates rows now
/// (`decisions/every-spec-enumerates-its-forms.md`), so the audit follows the
/// same three steps the 6502 one does: synthesise representative bytes for the
/// row, let **our** disassembler write the source — which is what answers
/// "how is this operand written" without a second opinion — and hand that to
/// the real assembler.
///
/// What this catches is a declared row whose encoding disagrees with lwasm.
/// It cannot catch a row nobody declared: that is `xtask surface`'s job, and
/// the reason it rather than coverage found
/// [#225](https://github.com/asm198x/asm198x/issues/225).
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn spec_rows_match_reference_6809() {
    if !have("lwasm") {
        eprintln!("SKIP: `lwasm` not on PATH (6809 form audit)");
        return;
    }
    let tmp = std::env::temp_dir().join("asm198x-form-6809");
    let _ = fs::create_dir_all(&tmp);
    let mut recorder = support::verdicts::Recorder::new();
    let mut fails: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut unsynthesised: Vec<String> = Vec::new();
    let mut input_only: Vec<&str> = Vec::new();

    for insn in isa::mos6809::SET {
        // An undocumented opcode is input-only, so the disassembler never
        // writes it and this audit — which goes through the disassembler —
        // cannot arbitrate it. Reported by name rather than counted as a gap.
        if insn.undocumented {
            input_only.push(insn.mnemonic);
            continue;
        }
        for row in isa::mos6809::rows().filter(|r| r.mnemonic == insn.mnemonic) {
            let Some((buf, n)) = insn.exemplar(row.mode) else {
                unsynthesised.push(format!("{} {}", row.mnemonic, row.mode));
                continue;
            };
            let bytes = &buf[..n];
            let text = asm198x::listing_6809(bytes, 0x0000);
            let source = format!("\torg $0000\n{text}");
            let reference = ref_assemble(&tmp, &source, "asm", |src, out| {
                let mut c = Command::new("lwasm");
                c.args(["--6809", "--raw", "-o"]).arg(out).arg(src);
                vec![c]
            });
            let Some(reference) = reference else {
                fails.push(format!(
                    "{} {}: lwasm rejected our own disassembly:\n{source}",
                    row.mnemonic, row.mode
                ));
                continue;
            };
            checked += 1;
            recorder.record_bytes(
                support::verdicts::CaseRef {
                    suite: Suite::Form,
                    cpu: "6809",
                    tool: "lwasm",
                    dialect: "lwasm",
                    case: format!("{} {}", row.mnemonic, row.mode),
                    source: &source,
                },
                &reference,
            );
            if reference != bytes {
                fails.push(format!(
                    "{} {}: ours {:02X?} vs lwasm {:02X?}",
                    row.mnemonic, row.mode, bytes, reference
                ));
            }
        }
    }
    let added = recorder.flush().expect("write the corpus");
    if added > 0 {
        eprintln!("6809 form audit: {added} new verdict(s) recorded");
    }

    // A row the synthesiser cannot build is a row this audit silently skips,
    // which is the shape of hole the whole exercise exists to close.
    assert!(
        unsynthesised.is_empty(),
        "{} row(s) have no representative bytes, so nothing arbitrates them:\n  {}",
        unsynthesised.len(),
        unsynthesised.join("\n  ")
    );
    assert!(
        fails.is_empty(),
        "{} of {checked} 6809 rows diverge:\n  {}",
        fails.len(),
        fails.join("\n  ")
    );
    assert!(checked > 0, "no rows arbitrated");
    eprintln!(
        "6809 form audit: {checked} rows arbitrated against lwasm, {} input-only ({})",
        input_only.len(),
        input_only.join(", ")
    );
}

/// The word CPUs' form audits: PDP-11, TMS9900 and the CP-1610, against `asl`.
///
/// Their specs are identical in shape — `Insn { mnemonic, base, class }` — so
/// one implementation serves all three, and the only per-CPU facts are the
/// byte order, the extension-word filler and asl's name for the chip. All
/// three are copied from the sweeps rather than restated.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn spec_rows_match_reference_word_cpus() {
    if !(have("asl") && have("p2bin")) {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (word-CPU form audits)");
        return;
    }
    let tmp = std::env::temp_dir().join("asm198x-form-word");
    let _ = fs::create_dir_all(&tmp);
    let mut recorder = support::verdicts::Recorder::new();
    let mut fails: Vec<String> = Vec::new();
    let mut total = 0usize;

    total += word_cpu_form_audit(
        "PDP-11",
        isa::pdp11::rows(),
        &|m, _| {
            isa::pdp11::INSTRUCTIONS
                .iter()
                .find(|i| i.mnemonic == m)
                .map(|i| (i.base, i.exemplar()))
        },
        false,
        &[0x10, 0x00, 0x20, 0x00, 0x30, 0x00],
        &|b, o| asm198x::listing_pdp11(b, o as u16),
        &tmp,
        &mut recorder,
        &mut fails,
    );
    total += word_cpu_form_audit(
        "TMS9900",
        isa::tms9900::rows(),
        &|m, _| {
            isa::tms9900::INSTRUCTIONS
                .iter()
                .find(|i| i.mnemonic == m)
                .map(|i| (i.base, i.exemplar()))
        },
        true,
        &[0x10, 0x00, 0x20, 0x00, 0x30, 0x00],
        &|b, o| asm198x::listing_tms9900(b, o as u16),
        &tmp,
        &mut recorder,
        &mut fails,
    );
    total += word_cpu_form_audit(
        // The corpus label, not asl's chip name — the listing writes its own
        // `cpu` line, so this is only ever the label a verdict is filed under.
        "CP1610",
        isa::cp1610::rows(),
        &|m, _| {
            isa::cp1610::INSTRUCTIONS
                .iter()
                .find(|i| i.mnemonic == m)
                .map(|i| (i.base, i.exemplar()))
        },
        true,
        &[0x12, 0x34],
        &|b, o| asm198x::listing_cp1610(b, o as u16),
        &tmp,
        &mut recorder,
        &mut fails,
    );

    let added = recorder.flush().expect("write the corpus");
    if added > 0 {
        eprintln!("word-CPU form audits: {added} new verdict(s) recorded");
    }
    assert!(
        fails.is_empty(),
        "{} of {total} word-CPU rows diverge or cannot be arbitrated:\n  {}",
        fails.len(),
        fails.join("\n  ")
    );
    assert!(total > 0, "no rows arbitrated");
    eprintln!("word-CPU form audits: {total} rows arbitrated against asl");
}

/// The Z8000 form audit, against `asl`.
///
/// Thirteen tables that share no opcode-word formula, so the exemplar lives
/// with each family in `isa::z8000` rather than being guessed here — the
/// lesson of the attempt that failed, which searched and mis-encoded in equal
/// measure.
///
/// A row no family can yet exemplify is **skipped and reported**, not
/// arbitrated. Coverage then shows the true fraction: a partial audit that
/// claimed 100% would be the exact dishonesty this work exists to end.
/// The non-segmented Z8002.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn spec_rows_match_reference_z8000() {
    z8000_form_audit("Z8000", false);
}

/// The segmented Z8001, whose rows are the same and whose encodings are not:
/// an `@Rn` pointer becomes a register pair, and memory operands carry a
/// segment. Its own audit, because "the same spec" is a claim to check rather
/// than assume.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn spec_rows_match_reference_z8001() {
    z8000_form_audit("Z8001", true);
}

fn z8000_form_audit(cpu: &str, seg: bool) {
    if !(have("asl") && have("p2bin")) {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH ({cpu} form audit)");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("asm198x-form-{cpu}"));
    let _ = fs::create_dir_all(&tmp);
    let mut recorder = support::verdicts::Recorder::new();
    let mut fails: Vec<String> = Vec::new();
    let mut unplaced = 0usize;
    let mut excepted: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let filler = [0x10u8, 0x00, 0x20, 0x00, 0x30, 0x00];
    // A byte immediate travels as its byte *replicated* into a word, so filler
    // that is merely distinct decodes as no byte immediate at all. These rows
    // get operand words that are a legal instance of the form.
    let byte_filler = [0x05u8; 6];
    // A segmented memory operand carries a long-form segment word: bit 15 set,
    // the segment in bits 14-8, low byte zero. `asl` writes that form even for
    // an offset small enough for the short one, so it is what these rows get.
    let seg_filler = [0x80u8, 0x00, 0x20, 0x00, 0x30, 0x00];

    // Rows this method cannot arbitrate, each with the reason and the issue
    // that will retire the entry. Named rather than folded into the anonymous
    // unplaced count: a known exception is a finding, not a gap.
    const EXCEPTIONS: &[(&str, &str, &str)] = &[];

    for row in isa::z8000::rows() {
        let m = row.mnemonic;
        if let Some((.., why)) = EXCEPTIONS
            .iter()
            .find(|(mn, mode, _)| *mn == m && *mode == row.mode)
        {
            excepted.push(format!("{m} {}: {why}", row.mode));
            continue;
        }
        let dyadic = isa::z8000::INSTRUCTIONS.iter().find(|i| i.mnemonic == m);
        let filler = match dyadic {
            Some(i) if i.size == isa::z8000::Size::Byte && row.mode == "immediate" => &byte_filler,
            _ if seg && matches!(row.mode, "direct address" | "indexed") => &seg_filler,
            _ => &filler,
        };
        // The opcode word, and the operand word a shift carries with it. Only
        // that family has one; for everything else the operands come from the
        // filler.
        let placed: Option<(u16, Option<u16>)> = dyadic
            .and_then(|i| i.exemplar(row.mode, seg))
            .map(|w| (w, None))
            .or_else(|| {
                isa::z8000::MONO
                    .iter()
                    .find(|i| i.mnemonic == m)
                    .map(|i| (i.exemplar(), None))
            })
            .or_else(|| {
                isa::z8000::SHIFTS
                    .iter()
                    .find(|i| i.mnemonic == m)
                    .map(isa::z8000::Shift::exemplar)
            })
            .or_else(|| {
                isa::z8000::BLOCK.iter().find(|i| i.mnemonic == m).map(|i| {
                    let (a, b) = i.exemplar();
                    (a, Some(b))
                })
            })
            .or_else(|| {
                isa::z8000::BLOCK_IO
                    .iter()
                    .find(|i| i.mnemonic == m)
                    .map(|i| {
                        let (a, b) = i.exemplar();
                        (a, Some(b))
                    })
            })
            .or_else(|| {
                isa::z8000::EXTENDS
                    .iter()
                    .find(|i| i.mnemonic == m)
                    .map(|i| (i.exemplar(), None))
            })
            .or_else(|| {
                isa::z8000::MULDIV
                    .iter()
                    .find(|i| i.mnemonic == m)
                    .map(|i| (i.exemplar(), None))
            });
        let Some((word, operand)) = placed else {
            unplaced += 1;
            continue;
        };
        let bytes = match operand {
            Some(w) => {
                let mut b = exemplar_bytes(word, true, &w.to_be_bytes());
                b.extend_from_slice(filler);
                b
            }
            None => exemplar_bytes(word, true, filler),
        };
        let listing = if seg {
            asm198x::listing_z8001(&bytes, 0x1000)
        } else {
            asm198x::listing_z8000(&bytes, 0x1000)
        };
        let Some(source) = trim_after_instruction(&listing, m) else {
            // The exemplar is not an instance of this row: the family's rule
            // and the disassembler disagree, which is a real finding rather
            // than a row to skip quietly.
            fails.push(format!(
                "{cpu} {m} {}: exemplar {word:#06X} disassembles to something else:\n{listing}",
                row.mode
            ));
            continue;
        };
        let reference = ref_assemble(&tmp, &source, "asm", |src, out| {
            let obj = src.with_extension("p");
            let mut a = Command::new("asl");
            a.arg("-q").arg(src).arg("-o").arg(&obj);
            let mut b = Command::new("p2bin");
            b.arg(&obj).arg(out);
            vec![a, b]
        });
        let Some(reference) = reference else {
            fails.push(format!(
                "{cpu} {m} {}: asl rejected our own disassembly:\n{source}",
                row.mode
            ));
            continue;
        };
        checked += 1;
        recorder.record_bytes(
            support::verdicts::CaseRef {
                suite: Suite::Form,
                cpu,
                tool: "asl",
                dialect: "asl",
                case: format!("{m} {}", row.mode),
                source: &source,
            },
            &reference,
        );
        if reference.len() < 2 || reference[..2] != bytes[..2] {
            fails.push(format!(
                "{cpu} {m} {}: ours {:02X?} vs asl {:02X?}",
                row.mode,
                &bytes[..2],
                &reference[..reference.len().min(2)]
            ));
        }
    }

    let added = recorder.flush().expect("write the corpus");
    eprintln!(
        "{cpu} form audit: {checked} arbitrated ({added} new), {unplaced} rows have no \
         exemplar yet, {} excepted",
        excepted.len()
    );
    for e in &excepted {
        eprintln!("  excepted: {e}");
    }
    assert!(
        fails.is_empty(),
        "{} {cpu} row(s) diverge:\n  {}",
        fails.len(),
        fails.join("\n  ")
    );
    assert!(checked > 0, "no rows arbitrated");
}

/// Identify every reference tool this machine has, proving the probe table
/// against the real binaries rather than against synthetic output.
///
/// `#[ignore]`d because it needs the tools — but unlike the audits it asserts
/// nothing about *bytes*, only that a present tool can be identified. A tool
/// that runs but cannot be identified is a silent hole in the corpus's
/// provenance, so it fails loudly here rather than recording verdicts signed
/// by nobody.
#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn every_present_reference_tool_can_be_identified() {
    const TOOLS: &[&str] = &[
        "acme",
        "ca65",
        "ld65",
        "pasmo",
        "sjasmplus",
        "lwasm",
        "rgbasm",
        "rgblink",
        "asl",
        "p2bin",
        "vasmm68k_mot",
    ];
    let mut unidentified = Vec::new();
    let mut seen = 0usize;
    for tool in TOOLS {
        if !have(tool) {
            eprintln!("SKIP: {tool} not on PATH");
            continue;
        }
        seen += 1;
        match support::tool_identity::identify(tool) {
            Some(id) => eprintln!("{tool}: {} [{}]", id.identity, &id.digest[..12]),
            None => unidentified.push(*tool),
        }
    }
    assert!(
        unidentified.is_empty(),
        "present but unidentifiable, so their verdicts would be unsigned: {unidentified:?}"
    );
    assert!(seen > 0, "no reference tools present at all");
}
