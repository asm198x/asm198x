//! Register/condition words as labels, measured against RGBDS 1.0.4 (#1063).

use asm198x::{assemble_rgbasm, assemble_rgbasm_files, source::MemoryLoader};

#[path = "support/scratch.rs"]
mod scratch;
#[path = "support/tool_identity.rs"]
mod tool_identity;

const SECTION: &str = "SECTION \"code\", ROM0[0]\n";
const KEYWORDS: &[&str] = &[
    "A", "B", "C", "D", "E", "H", "L", "AF", "BC", "DE", "HL", "SP", "HLD", "HLI", "NZ", "Z", "NC",
];
const OTHER_CASES: &[(&str, &str)] = &[("hli", ":"), ("nC", "::")];
const SCOPED: &str = "Scope:\n.l: nop\n.HLI\n nop\n jp Scope.l\n jr nz, Scope.HLI\n";
const NON_KEYWORDS: &str = "Scoped_HLI: nop\nSPIN: nop\nL1:: nop\nPC: nop\nF: nop\n";
const INACTIVE: &str = "IF 0\nL: nop\nENDC\n nop\n";
const LIVE_DS: &str = "HLI:: ds $1 - @\n";

fn rejected_labels() -> impl Iterator<Item = (&'static str, &'static str)> {
    KEYWORDS
        .iter()
        .flat_map(|&word| [(word, ":"), (word, "::")])
        .chain(OTHER_CASES.iter().copied())
}

#[test]
fn register_and_condition_words_cannot_be_plain_labels() {
    for (word, colon) in rejected_labels() {
        let source = format!("{SECTION}{word}{colon} nop\n");
        let error = assemble_rgbasm(&source).expect_err(&source);
        assert_eq!(error.line, 2, "{source}");
        assert!(error.message.contains(word), "{error}");
        assert!(error.message.contains("reserved"), "{error}");
    }
}

#[test]
fn reserved_label_check_applies_inside_included_source() {
    let source = format!("{SECTION}INCLUDE \"defs.inc\"\n");
    let loader = MemoryLoader::new().text("defs.inc", "; included comment\nnC:: nop\n");
    let error = assemble_rgbasm_files(&source, "main.asm", &loader).expect_err("included label");
    let span = error.error.span.expect("source position");
    assert_eq!(span.line, 2);
    assert_eq!(span.file.0, 1, "error is in the included file");
    assert!(
        error.error.message.contains("nC"),
        "{}",
        error.error.message
    );
}

#[test]
fn local_labels_and_whole_word_matches_leave_operands_alone() {
    let source = format!("{SECTION}{SCOPED}{NON_KEYWORDS}");
    let bytes = [0x00, 0x00, 0xC3, 0, 0, 0x20, 0xFA, 0, 0, 0, 0, 0];
    assert_eq!(assemble_rgbasm(&source).expect("valid labels").bytes, bytes);

    let loader = MemoryLoader::new().text("defs.inc", format!("{SCOPED}{NON_KEYWORDS}"));
    let source = format!("{SECTION}INCLUDE \"defs.inc\"\n");
    let included = assemble_rgbasm_files(&source, "main.asm", &loader).expect("valid include");
    assert_eq!(included.bytes, bytes);
}

#[test]
fn the_check_runs_for_live_labels_only() {
    let source = format!("{SECTION}{INACTIVE}");
    assert_eq!(assemble_rgbasm(&source).expect("untaken label").bytes, [0]);
    let loader = MemoryLoader::new().text("defs.inc", INACTIVE);
    let source = format!("{SECTION}INCLUDE \"defs.inc\"\n");
    let included = assemble_rgbasm_files(&source, "main.asm", &loader).expect("untaken include");
    assert_eq!(included.bytes, [0]);

    for source in [INACTIVE.replace("IF 0", "IF 1"), LIVE_DS.to_string()] {
        let source = format!("{SECTION}{source}");
        let error = assemble_rgbasm(&source).expect_err("live reserved label");
        assert!(error.message.contains("reserved"), "{error}");
    }
}

#[test]
#[ignore = "requires the RGBDS 1.0.4 reference executable"]
fn rgbds_1_0_4_label_verdicts() {
    let rgbasm = tool_identity::identify("rgbasm").expect("reference identity");
    assert_eq!(rgbasm.identity, "rgbasm v1.0.4");
    eprintln!("{rgbasm:?}");
    let dir = scratch::dir("rgbasm-labels");
    let cases = rejected_labels()
        .map(|(word, colon)| (format!("{word}{colon} nop\n"), false))
        .chain([
            (INACTIVE.replace("IF 0", "IF 1"), false),
            (LIVE_DS.to_owned(), false),
        ])
        .chain(
            [SCOPED, NON_KEYWORDS, INACTIVE, "#L: nop\n", "#NC:: nop\n"]
                .into_iter()
                .map(|source| (source.to_owned(), true)),
        );
    for (index, (source, accepted)) in cases.enumerate() {
        let name = format!("{index}.asm");
        std::fs::write(dir.join(&name), format!("{SECTION}{source}")).expect("write probe");
        let output = std::process::Command::new(&rgbasm.path)
            .args(["-o", "input.o", &name])
            .current_dir(&dir)
            .output()
            .expect("reference execution");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), accepted, "{source}: {stderr}");
        if !accepted {
            assert!(
                stderr.contains("syntax error, unexpected"),
                "{source}: {stderr}"
            );
        }
    }
}
