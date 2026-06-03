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
                            fails.push(format!("6502 {} {}: ours {:02X?} vs acme {:02X?}", insn.mnemonic, form.mode, bytes, &r[2..]));
                        }
                    }
                    _ => fails.push(format!("6502 {} {}: acme rejected", insn.mnemonic, form.mode)),
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
                            fails.push(format!("Z80 {} {}: ours {:02X?} vs pasmo {:02X?}", insn.mnemonic, form.mode, bytes, r));
                        }
                    }
                    None => fails.push(format!("Z80 {} {}: pasmo rejected `{}`", insn.mnemonic, form.mode, text.lines().nth(1).unwrap_or("").trim())),
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
                                fails.push(format!("65816 {} {}: ours {:02X?} vs ca65 {:02X?}", insn.mnemonic, form.mode, bytes, r));
                            }
                        }
                        None => fails.push(format!("65816 {} {}: ca65 rejected `{}`", insn.mnemonic, form.mode, text.lines().last().unwrap_or("").trim())),
                    }
                }
            }
        }
    } else {
        eprintln!("SKIP: `ca65`/`ld65` not on PATH (65816)");
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

/// A tiny deterministic LCG, so the fuzz corpus is reproducible.
struct Rng(u64);
impl Rng {
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
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

/// Differential fuzz: random multi-instruction programs, disassembled then
/// reassembled by **both** our assembler and the reference. Both must reproduce
/// the original bytes — self-consistency *and* a ground-truth cross-check, over
/// random operand values and instruction sequences the curated corpus misses.
///
/// Stateless CPUs only (6502, Z80): the 65816's `m`/`x` width makes a random
/// instruction stream ambiguous to decode, so it is covered by the per-form
/// audit and the curriculum round-trip instead.
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
    let cpus = [Cpu { name: "6502", tool: "acme" }, Cpu { name: "Z80", tool: "pasmo" }];

    for cpu in cpus {
        if !have(cpu.tool) {
            eprintln!("SKIP fuzz: `{}` not on PATH", cpu.tool);
            continue;
        }
        let forms: Vec<&isa::Form> = match cpu.name {
            "6502" => isa::mos6502::SET.instructions.iter().flat_map(|i| i.forms).collect(),
            _ => isa::z80::SET.instructions.iter().flat_map(|i| i.forms).collect(),
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
                Ok(o) => fails.push(format!("{} prog {p}: our reasm differs ({} vs {} bytes)", cpu.name, o.len(), bytes.len())),
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
            let reference = reference.map(|r| if cpu.name == "6502" && r.len() >= 2 { r[2..].to_vec() } else { r });
            match reference {
                Some(r) if r == bytes => checked += 1,
                Some(r) => fails.push(format!("{} prog {p}: reference reasm differs ({} vs {} bytes)", cpu.name, r.len(), bytes.len())),
                None => fails.push(format!("{} prog {p}: reference rejected disassembly", cpu.name)),
            }
        }
    }

    eprintln!("fuzzed {checked} random programs (both assemblers vs the bytes)");
    assert!(
        fails.is_empty(),
        "{} fuzz mismatch(es):\n  {}",
        fails.len(),
        fails.iter().take(20).cloned().collect::<Vec<_>>().join("\n  ")
    );
    assert!(checked > 0, "no fuzzing ran — no tools present?");
}
