//! `--gb-rom` links a Game Boy ROM as plain `rgblink` does and finishes it as
//! `rgbfix -v -p 0xFF` does (#970). Plain RGBLINK keeps ROM0 to
//! `$0000-$3FFF`, so a ROM0 section that runs past it is refused, in
//! RGBLINK's words, rather than linked into the next bank.

use std::path::Path;
use std::process::{Command, Output};

#[path = "support/scratch.rs"]
mod scratch;
#[path = "support/tool_identity.rs"]
mod tool_identity;

/// The curriculum's shape: a header, then one ROM0 section padded to the end
/// of a 32 KiB cartridge. Plain RGBLINK refuses it twice over.
const PAST_THE_BANK: &str = "SECTION \"Header\", ROM0[$100]\n nop\n jp $150\n\
                             SECTION \"Program\", ROM0[$150]\n ld a, 1\n ds $8000-@, 0\n";

fn asm198x(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run asm198x")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn gb_rom_refuses_rom0_past_the_first_bank_in_rgblinks_words() {
    let dir = scratch::dir("rgblink-layouts-refusal");
    std::fs::write(dir.join("program.asm"), PAST_THE_BANK).expect("source");
    for extra in [&["--gb-rom"][..], &[][..]] {
        let output = asm198x(
            &dir,
            &[
                &["--dialect", "rgbasm", "program.asm", "-o", "out.gb"],
                extra,
            ]
            .concat(),
        );
        let message = stderr(&output);
        assert!(!output.status.success(), "{extra:?}: {message}");
        assert!(
            message.contains(
                "Section \"Program\" is bigger than the max size for that type: $7eb0 > $4000"
            ),
            "{extra:?}: {message}"
        );
        assert!(
            message.contains(
                "Section \"Program\"'s end address $8000 is greater than last address $4000"
            ),
            "{extra:?}: {message}"
        );
        assert!(!dir.join("out.gb").exists(), "a refused link wrote a ROM");
    }
}

#[test]
fn gb_rom_pads_the_second_bank_with_ff_after_a_full_rom0() {
    let dir = scratch::dir("rgblink-layouts-plain");
    std::fs::write(
        dir.join("program.asm"),
        "SECTION \"Program\", ROM0[$150]\n ds $4000-@, $11\n",
    )
    .expect("source");
    let output = asm198x(
        &dir,
        &[
            "--dialect",
            "rgbasm",
            "--gb-rom",
            "program.asm",
            "-o",
            "out.gb",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let rom = std::fs::read(dir.join("out.gb")).expect("ROM");
    assert_eq!(rom.len(), 0x8000);
    assert!(rom[0x150..0x4000].iter().all(|&byte| byte == 0x11));
    assert!(rom[0x4000..].iter().all(|&byte| byte == 0xFF));
}

/// RGBLINK writes whole banks, so `rgbfix` always has a header to finish,
/// even for a program that stops short of it.
#[test]
fn gb_rom_finishes_a_program_that_ends_before_its_header() {
    let dir = scratch::dir("rgblink-layouts-short");
    std::fs::write(dir.join("program.asm"), "SECTION \"a\", ROM0[$100]\n nop\n").expect("source");
    let output = asm198x(
        &dir,
        &[
            "--dialect",
            "rgbasm",
            "--gb-rom",
            "program.asm",
            "-o",
            "out.gb",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let rom = std::fs::read(dir.join("out.gb")).expect("ROM");
    assert_eq!(rom.len(), 0x8000);
    assert_eq!(rom[0x104..0x108], [0xCE, 0xED, 0x66, 0x66], "logo");
}

/// Each probe exercises one RGBLINK rule. Every probe assembles under
/// `rgbasm`; what RGBLINK does with it is the arbiter's answer.
const PROBES: &[(&str, &str)] = &[
    (
        "a program that ends before the cartridge header",
        "SECTION \"a\", ROM0[$100]\n nop\n",
    ),
    (
        "ROM0 fills its bank exactly",
        "SECTION \"a\", ROM0[$150]\n ds $3eb0, 1\n",
    ),
    (
        "pinned ROM0 runs one byte past $4000",
        "SECTION \"a\", ROM0[$150]\n ds $3eb1, 1\n",
    ),
    ("the curriculum's 32 KiB ROM0", PAST_THE_BANK),
    (
        "pinned ROM0 above the first bank",
        "SECTION \"a\", ROM0[$4100]\n db 1\n",
    ),
    (
        "pinned ROM0 at the top of the address space",
        "SECTION \"a\", ROM0[$7fff]\n db 1, 2\n",
    ),
    (
        "floating ROM0 bigger than a bank",
        "SECTION \"a\", ROM0\n ds $4001, 1\n",
    ),
    (
        "floating ROM0 sections that overflow together",
        "SECTION \"a\", ROM0\n ds $3000, 1\nSECTION \"b\", ROM0\n ds $2000, 2\n",
    ),
    (
        "floating ROM0 sections that overflow 32 KiB together",
        "SECTION \"a\", ROM0\n ds $5000, 1\nSECTION \"b\", ROM0\n ds $4000, 2\n",
    ),
    (
        "floating ROM0 around a pinned section",
        "SECTION \"p\", ROM0[$100]\n ds $10, 3\nSECTION \"a\", ROM0\n ds $3f00, 1\n",
    ),
    (
        "floating ROM0 split around a pinned section",
        "SECTION \"p\", ROM0[$2000]\n ds $10, 3\nSECTION \"a\", ROM0\n ds $2000, 1\n\
         SECTION \"b\", ROM0\n ds $1ff0, 2\n",
    ),
    (
        "every refusal at once, in declaration order",
        "SECTION \"x\", ROM0[$4100]\n db 1\nSECTION \"y\", ROM0\n ds $4001, 1\n\
         SECTION \"z\", ROM0[$3fff]\n db 1, 2\n",
    ),
    (
        "equal-sized floating sections",
        "SECTION \"r0\", ROM0\n ds 16, 1\nSECTION \"rx\", ROMX\n ds 16, 2\n\
         SECTION \"r1\", ROM0\n ds 16, 3\n",
    ),
    (
        "floating ROMX",
        "SECTION \"a\", ROM0[0]\n db BANK(\"b\")\n dw Lb\nSECTION \"b\", ROMX\nLb: db $bb\n",
    ),
    (
        "pinned ROMX in bank 1",
        "SECTION \"a\", ROM0[0]\n db BANK(\"b\")\nSECTION \"b\", ROMX[$4000], BANK[1]\nLb: db $bb\n",
    ),
    (
        "pinned ROMX with no bank",
        "SECTION \"a\", ROM0[0]\n db BANK(\"b\")\nSECTION \"b\", ROMX[$5000]\nLb: db $bb\n",
    ),
    (
        "ROMX in bank 2",
        "SECTION \"a\", ROM0[0]\n db BANK(\"b\")\nSECTION \"b\", ROMX[$4000], BANK[2]\nLb: db $bb\n",
    ),
    (
        "pinned ROMX past the end of its bank",
        "SECTION \"b\", ROMX[$7fff]\n db 1, 2\n",
    ),
];

/// What one side did with a probe: the finished ROM, or the messages it
/// refused it with.
enum Outcome {
    Rom(Vec<u8>),
    Refused(Vec<String>),
}

/// A ROM by its size, so a mismatch reads as a sentence rather than 32 KiB of
/// numbers.
impl std::fmt::Debug for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rom(rom) => write!(f, "a {}-byte ROM", rom.len()),
            Self::Refused(reasons) => write!(f, "refused: {reasons:?}"),
        }
    }
}

/// `rgbasm`, then `rgblink` with `link_flags`, then `rgbfix -v -p 0xFF`.
fn reference(dir: &Path, link_flags: &[&str]) -> Outcome {
    let _ = std::fs::remove_file(dir.join("ref.gb"));
    let run = |tool: &str, args: &[&str]| {
        Command::new(tool)
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("run {tool}: {e}"))
    };
    let assembled = run("rgbasm", &["-o", "probe.o", "probe.asm"]);
    assert!(
        assembled.status.success(),
        "every probe must assemble under rgbasm: {}",
        stderr(&assembled)
    );
    let linked = run(
        "rgblink",
        &[link_flags, &["-o", "ref.gb", "probe.o"][..]].concat(),
    );
    if !linked.status.success() {
        // `error: …` and `FATAL: …` lines; the summary line is not a reason.
        let reasons = stderr(&linked)
            .lines()
            .filter_map(|line| {
                line.strip_prefix("error: ")
                    .or_else(|| line.strip_prefix("FATAL: "))
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert!(!reasons.is_empty(), "rgblink failed silently");
        return Outcome::Refused(reasons);
    }
    let fixed = run("rgbfix", &["-v", "-p", "0xFF", "ref.gb"]);
    assert!(fixed.status.success(), "rgbfix: {}", stderr(&fixed));
    Outcome::Rom(std::fs::read(dir.join("ref.gb")).expect("reference ROM"))
}

fn ours(dir: &Path, flags: &[&str]) -> Outcome {
    let _ = std::fs::remove_file(dir.join("ours.gb"));
    let output = asm198x(
        dir,
        &[
            &["--dialect", "rgbasm", "--gb-rom"][..],
            flags,
            &["probe.asm", "-o", "ours.gb"],
        ]
        .concat(),
    );
    if output.status.success() {
        Outcome::Rom(std::fs::read(dir.join("ours.gb")).expect("our ROM"))
    } else {
        Outcome::Refused(vec![stderr(&output)])
    }
}

/// Byte identity where RGBLINK links, and a refusal carrying each of its
/// reasons where it does not.
fn agree(reference: &Outcome, ours: &Outcome) -> Result<(), String> {
    match (reference, ours) {
        (Outcome::Rom(theirs), Outcome::Rom(mine)) if theirs == mine => Ok(()),
        (Outcome::Rom(theirs), Outcome::Rom(mine)) => {
            let first = theirs.iter().zip(mine).position(|(a, b)| a != b);
            Err(format!(
                "ROMs differ: {} vs {} bytes, first difference at {first:X?}",
                theirs.len(),
                mine.len()
            ))
        }
        (Outcome::Refused(reasons), Outcome::Refused(said)) => {
            let missing: Vec<&String> = reasons
                .iter()
                .filter(|reason| !said[0].contains(reason.as_str()))
                .collect();
            if missing.is_empty() {
                Ok(())
            } else {
                Err(format!("refused without {missing:?}; said {said:?}"))
            }
        }
        (reference, ours) => Err(format!("reference {reference:?}, ours {ours:?}")),
    }
}

#[test]
#[ignore = "needs installed RGBDS references (rgbasm, rgblink, rgbfix)"]
fn gb_rom_matches_rgblink_then_rgbfix() {
    for tool in ["rgbasm", "rgblink"] {
        eprintln!(
            "{:?}",
            tool_identity::identify(tool).expect("reference identity")
        );
    }
    let dir = scratch::dir("rgblink-layouts-reference");
    let mut failures = Vec::new();
    let mut refusals = 0;
    for (name, source) in PROBES {
        std::fs::write(dir.join("probe.asm"), source).expect("probe");
        let reference = reference(&dir, &[]);
        refusals += usize::from(matches!(reference, Outcome::Refused(_)));
        if let Err(why) = agree(&reference, &ours(&dir, &[])) {
            failures.push(format!("{name}: {why}"));
        }
    }
    // The probes must test both answers, or agreement proves nothing.
    assert!(
        refusals > 0 && refusals < PROBES.len(),
        "{refusals} refusals"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
