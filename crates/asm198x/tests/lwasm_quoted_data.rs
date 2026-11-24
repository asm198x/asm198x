//! lwasm 4.25 reference cases for punctuation inside data literals (#983).

use asm198x::{assemble_lwasm, format_lwasm};

#[path = "support/scratch.rs"]
mod scratch;
#[path = "support/tool_identity.rs"]
mod tool_identity;

const PUNCTUATION: &str = "        org $4000\n        fcb '8','9',':',';',',','-','.',$2F\n";
const PUNCTUATION_BYTES: &[u8] = &[0x38, 0x39, 0x3A, 0x3B, 0x2C, 0x2D, 0x2E, 0x2F];
const EXPRESSIONS: &str = "        org $4000\n        fcb (';'+1),(','-$01),$12 ; comment , ' \"\n";
const STRINGS: &str =
    "        fcc /A;B,C/ ; comment after FCC\n        fcc \"D;E,F's\" ; another comment\n";
const STRINGS_BYTES: &[u8] = b"A;B,CD;E,F's";
const INDEXED: &str = "; fcc /this is; a comment/\n        * whole-line comment: , ;\n        lda 2,x ; indexed operand\n";
const CASES: &[(&str, &[u8])] = &[
    (PUNCTUATION, PUNCTUATION_BYTES),
    (EXPRESSIONS, &[0x3C, 0x2B, 0x12]),
    ("        fcb '(',')','\"',',',';' ; inline\n", b"()\",;"),
    (STRINGS, STRINGS_BYTES),
    ("        fcc 'A;B'\n        fcc ;CD;\n", b"A;BCD"),
    ("        fcn /x;y/\n        fcs /a;b/\n", b"x;y\0a;\xE2"),
    (INDEXED, &[0xA6, 0x02]),
];

#[test]
fn accepts_basic_punctuation_in_fcb() {
    let bytes = assemble_lwasm(PUNCTUATION)
        .expect("punctuation bytes")
        .bytes;
    assert_eq!(bytes, PUNCTUATION_BYTES);
}

#[test]
fn quotes_do_not_hide_external_separators_or_change_operands() {
    for (source, expected) in CASES.iter().skip(1) {
        assert_eq!(
            assemble_lwasm(source).expect(source).bytes,
            *expected,
            "{source}"
        );
    }
}

#[test]
fn formatter_keeps_delimiters_and_real_comments() {
    let source = format!("{EXPRESSIONS}{STRINGS}");
    let formatted = format_lwasm(&source).expect("formats quoted data");
    assert!(formatted.contains("; comment after FCC"), "{formatted}");
    assert!(formatted.contains("; another comment"), "{formatted}");
    assert!(formatted.contains("; comment , ' \""), "{formatted}");
    assert_eq!(
        assemble_lwasm(&formatted).expect("reassembles").bytes,
        [&[0x3C, 0x2B, 0x12], STRINGS_BYTES].concat()
    );
}

#[test]
#[ignore = "requires the lwasm 4.25 reference executable"]
fn lwasm_4_25_quoted_data_bytes() {
    let lwasm = tool_identity::identify("lwasm").expect("reference identity");
    assert_eq!(lwasm.identity, "lwasm from lwtools 4.25");
    eprintln!("{lwasm:?}");
    let dir = scratch::dir("lwasm-quoted-data");
    for (source, expected) in CASES {
        std::fs::write(dir.join("input.asm"), source).expect("write probe");
        let result = std::process::Command::new(&lwasm.path)
            .args(["--raw", "-o", "output.bin", "input.asm"])
            .current_dir(&dir)
            .output()
            .expect("reference execution");
        assert!(
            result.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            std::fs::read(dir.join("output.bin")).expect("object bytes"),
            *expected,
            "{source}"
        );
        assert_eq!(
            assemble_lwasm(source).expect(source).bytes,
            *expected,
            "{source}"
        );
    }
}
