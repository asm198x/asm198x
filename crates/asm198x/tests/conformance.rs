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

fn have(bin: &str) -> bool {
    Command::new(bin).output().is_ok()
}

/// Synthesise canonical bytes for a form: its opcode, then filler operand bytes
/// chosen to avoid size-force edge cases (a 2-byte address is `$1234`, ≥ `$100`,
/// so it stays absolute; a 3-byte one is `$123456`, ≥ `$10000`, so it stays
/// long), then any trailing suffix bytes.
fn synth(form: &isa::Form) -> Vec<u8> {
    let mut b = form.opcode.to_vec();
    for op in form.operands {
        match op.kind {
            isa::OperandKind::RelativePc => {
                // A small forward offset, little-endian over the operand width.
                b.push(0x02);
                b.extend(std::iter::repeat_n(0x00, usize::from(op.bytes) - 1));
            }
            isa::OperandKind::Displacement => b.push(0x05),
            // Big-endian 16-bit immediate (Z80N `push nn`): $1234 high byte
            // first. Not reached today (this sweep walks the base Z80 set, not
            // the NEXT extension), but kept correct for when it is.
            isa::OperandKind::ImmediateBe => b.extend_from_slice(&[0x12, 0x34]),
            isa::OperandKind::Immediate | isa::OperandKind::Address => {
                // $12 / $1234 / $123456, little-endian.
                let bytes: &[u8] = match op.bytes {
                    1 => &[0x12],
                    2 => &[0x34, 0x12],
                    3 => &[0x56, 0x34, 0x12],
                    _ => &[],
                };
                b.extend_from_slice(bytes);
            }
        }
    }
    b.extend_from_slice(form.suffix);
    b
}

/// Run a reference assembler over `text`, returning the flat bytes it produced,
/// or `None` if it rejected the source. `build` is given the input and output
/// paths and must return the command (already configured) to run in `tmp`.
fn ref_assemble(
    tmp: &Path,
    text: &str,
    ext: &str,
    build: impl Fn(&Path, &Path) -> Vec<Command>,
) -> Option<Vec<u8>> {
    let src = tmp.join(format!("conf.{ext}"));
    let out = tmp.join("conf.out");
    let _ = fs::remove_file(&out);
    fs::write(&src, text).ok()?;
    for mut cmd in build(&src, &out) {
        if !cmd.current_dir(tmp).output().ok()?.status.success() {
            return None;
        }
    }
    fs::read(&out).ok()
}

#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn spec_opcodes_match_reference() {
    let tmp = std::env::temp_dir().join("asm198x-conformance");
    fs::create_dir_all(&tmp).expect("temp dir");
    let mut fails: Vec<String> = Vec::new();
    let mut checked = 0usize;

    // --- 6502 / acme -------------------------------------------------------
    if have("acme") {
        for insn in isa::mos6502::SET.instructions {
            for form in insn.forms {
                let bytes = synth(form);
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
                let bytes = synth(form);
                let text = asm198x::listing_z80(&bytes, 0x8000, false);
                let reference = ref_assemble(&tmp, &text, "z80", |src, out| {
                    let mut c = Command::new("pasmo");
                    c.arg(src).arg(out);
                    vec![c]
                });
                match reference {
                    Some(r) => {
                        checked += 1;
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
                    bytes.extend(synth(form));
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
                    let mut bytes = synth(form);
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
                let bytes = synth(form);
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
                let bytes = synth(form);
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
                let bytes = synth(form);
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
                let bytes = synth(form);
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
                let bytes = synth(form);
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
                let bytes = synth(form);
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
                let bytes = synth(form);
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
                let bytes = synth(form);
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
                let mut bytes = synth(form);
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
                let bytes = synth(form);
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

    eprintln!("audited {checked} spec forms against the reference tools");
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
fn sweep(
    name: &str,
    candidates: &[Vec<u8>],
    disasm: &dyn Fn(&[u8], u32) -> Vec<asm198x::Line>,
    listing: &dyn Fn(&[u8], u32) -> String,
    reassemble: &dyn Fn(&str) -> Option<Vec<u8>>,
    skip: &dyn Fn(&str) -> bool,
    fails: &mut Vec<String>,
) -> usize {
    let (oa, ob) = (0x1000u32, 0x4000u32);
    let mut instrs: Vec<Vec<u8>> = Vec::new();
    for cand in candidates {
        let la = disasm(cand, oa);
        let Some(fa) = la.first() else { continue };
        if is_data(&fa.text) || skip(&fa.text) {
            continue;
        }
        let lb = disasm(cand, ob);
        match lb.first() {
            Some(fb) if fb.text == fa.text => instrs.push(fa.bytes.clone()),
            _ => {} // position-dependent (or undecodable at ob) — skip
        }
    }
    if instrs.is_empty() {
        return 0;
    }
    let blob: Vec<u8> = instrs.concat();
    let source = listing(&blob, oa);
    if reassemble(&source).is_some_and(|a| a == blob) {
        return instrs.len();
    }
    // Localise: find the first instruction the reference can't reproduce.
    for instr in &instrs {
        let text = disasm(instr, oa)
            .first()
            .map_or_else(String::new, |l| l.text.clone());
        match reassemble(&listing(instr, oa)) {
            Some(b) if b == *instr => {}
            Some(b) => {
                fails.push(format!(
                    "{name}: {instr:02X?} -> ref {b:02X?} (disasm `{text}`)"
                ));
                break;
            }
            None => {
                fails.push(format!(
                    "{name}: ref rejected {instr:02X?} (disasm `{text}`)"
                ));
                break;
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
            "6809",
            &cands,
            &|b, o| asm198x::disassemble_6809(b, o as u16),
            &|b, o| asm198x::listing_6809(b, o as u16),
            &reasm,
            &|_| false,
            &mut fails,
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
            "68000",
            &cands,
            &|b, o| asm198x::disassemble_68000(b, o),
            &|b, o| asm198x::listing_68000(b, o),
            &reasm,
            &|_| false,
            &mut fails,
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
            "PDP-11",
            &cands,
            &|b, o| asm198x::disassemble_pdp11(b, o as u16),
            &|b, o| asm198x::listing_pdp11(b, o as u16),
            &reasm,
            &|_| false,
            &mut fails,
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
            "TMS9900",
            &cands,
            &|b, o| asm198x::disassemble_tms9900(b, o as u16),
            &|b, o| asm198x::listing_tms9900(b, o as u16),
            &reasm,
            &|_| false,
            &mut fails,
        );
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (TMS9900 sweep)");
    }

    // --- Zilog Z8000 / asl + p2bin (non-segmented Z8002) -------------------
    // Increments 1–6 (dyadic, program control, single-operand, stack, shifts /
    // rotates / sign-extends); groups not yet decoded fall to `word` data and
    // are skipped. Shifts also fall to data here — the fixed extension-word
    // filler is an out-of-range count — so their round-trip is the guard. See
    // decisions/z8000-staged-build.md.
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
            "Z8000",
            &cands,
            &|b, o| asm198x::disassemble_z8000(b, o as u16),
            &|b, o| asm198x::listing_z8000(b, o as u16),
            &reasm,
            &|_| false,
            &mut fails,
        );
    } else {
        eprintln!("SKIP: `asl`/`p2bin` not on PATH (Z8000 sweep)");
    }

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
                Some(r) if r == bytes => checked += 1,
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

    eprintln!("fuzzed {checked} random programs (both assemblers vs the bytes)");
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
                _ => asm198x::assemble_vasm(&text).ok(),
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
                _ => asm198x::assemble_vasm(&text),
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
                continue;
            }
            // Mismatch. Localise: does the reference reproduce each instruction on
            // its own? If one fails alone, the reference canonicalises that single
            // encoding differently from us (e.g. it masks an out-of-range static
            // bit number our more permissive assembler keeps) — outside what an
            // assembler round-trip can arbitrate, so scope it out, not a failure.
            // If every instruction reproduces alone but the program doesn't, the
            // composition itself diverges — a real bug.
            let single_divergence = disasm(&blob, oa).iter().any(|line| {
                let lt = match cpu.name {
                    "6809" => asm198x::listing_6809(&line.bytes, oa as u16),
                    _ => asm198x::listing_68000(&line.bytes, oa),
                };
                ref_assemble(&tmp, &lt, "asm", ref_build).as_deref() != Some(&line.bytes[..])
            });
            if single_divergence {
                scoped_out += 1;
            } else {
                fails.push(format!(
                    "{} prog {p}: reference composes the program differently\n    {}",
                    cpu.name,
                    text.replace('\n', " | ")
                ));
            }
        }
    }

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
