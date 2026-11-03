//! ca65 reads source as bytes and copies a string's bytes as they are, so a
//! file need not be UTF-8: `"é"` is $E9 in a Latin-1 file and $C3 $A9 in a
//! UTF-8 one. A byte past ASCII outside a string or a comment is an invalid
//! input character. Each program here was measured on ca65 V2.19 and ld65
//! under a flat link at 0.

use std::process::Command;

mod support;

const FLAT: &str = "MEMORY { ROM: start = $0000, size = $1000, fill = no, file = %O; }\n\
                    SEGMENTS { CODE: load = ROM, type = ro; }\n";

/// Each program's bytes, with what ca65 and ld65 made, or `None` if ca65
/// refused it.
const MEASURED: &[(&[u8], Option<&str>)] = &[
    (b".byte \"\xe9\"\n", Some("E9")),
    (b".byte \"\xc3\xa9\"\n", Some("C3A9")),
    (b".byte '\xe9'\n", Some("E9")),
    (b".byte '\xc3\xa9'\n", None),
    (b".byte 1 ; \xe9\xff\n", Some("01")),
    (b".byte 1 \xe9\n", None),
    (b"l\xe9: nop\n", None),
    (b".byte \"\x80\"\n", Some("80")),
    (b".charmap $e9, $41\n.byte \"\xe9\", '\xe9'\n", Some("4141")),
    (b".byte .strlen(\"\xe9\xe8\")\n", Some("02")),
    (b".byte .strlen(\"\xc3\xa9\")\n", Some("02")),
    (b".byte .strat(\"\xe9\", 0)\n", Some("E9")),
    (
        b".feature string_escapes\n.byte \"\xe9\\x41\"\n",
        Some("E941"),
    ),
    (b".define s \"\xe9\"\n.byte s\n", Some("E9")),
    (b".macro m v\n.byte v\n.endmacro\nm \"\xe9\"\n", Some("E9")),
    (b".byte .sprintf(\"%s\xe9\", \"\xe8\")\n", Some("E8E9")),
    (b".byte .concat(\"\xe9\", \"\xe8\")\n", Some("E9E8")),
    (b".feature loose_string_term\n.byte '\xe9'\n", Some("E9")),
    (b"nop\r\n.byte \"\xe9\"\r\n", Some("EAE9")),
    (b"\xef\xbb\xbf.byte 1\n", None),
];

/// What the walk makes of `program`, read from bytes, as hex.
fn ours(program: &[u8], loader: &asm198x::source::MemoryLoader) -> Option<String> {
    asm198x::assemble_ca65_bytes_on(program, "form.s", loader, "6502", Some(FLAT))
        .ok()
        .map(|(result, _)| verdict_corpus::encode_hex(&result.bytes))
}

#[test]
fn source_bytes_match_ca65() {
    let loader = asm198x::source::MemoryLoader::new();
    for &(program, hex) in MEASURED {
        assert_eq!(
            ours(program, &loader).as_deref(),
            hex,
            "{}",
            String::from_utf8_lossy(program)
        );
    }
}

/// ca65 V2.19 reads $FF as the end of the file, so it refuses a string
/// holding one with `Newline in string constant`. Here it is a byte like any
/// other: a named divergence in Lang198x.
#[test]
fn ff_is_a_byte() {
    let loader = asm198x::source::MemoryLoader::new();
    assert_eq!(
        ours(b".byte \"a\xffb\", 1\n", &loader).as_deref(),
        Some("61FF6201")
    );
}

/// Included files are read as bytes too, whatever the file that includes
/// them. A loader that has only text gives its UTF-8 bytes, which is how
/// ca65 reads a UTF-8 file.
#[test]
fn included_files_are_read_as_bytes() {
    let loader = asm198x::source::MemoryLoader::new()
        .source_bytes("latin1.inc", b".byte \"\xe9\"\n".to_vec())
        .text("utf8.inc", ".byte \"é\"\n");
    assert_eq!(
        ours(b".include \"latin1.inc\"\n.byte '\xe8'\n", &loader).as_deref(),
        Some("E9E8")
    );
    assert_eq!(
        ours(".include \"latin1.inc\"\n.byte \"é\"\n".as_bytes(), &loader).as_deref(),
        Some("E9C3A9")
    );
    assert_eq!(
        ours(b".include \"utf8.inc\"\n", &loader).as_deref(),
        Some("C3A9")
    );
}

/// Runs ca65 and ld65 on each program, when they are installed, to check
/// the table against them.
#[test]
#[ignore = "needs ca65 and ld65; run with --ignored to check the table"]
fn the_table_matches_ca65_today() {
    let dir = support::scratch::dir("ca65-source-bytes");
    let cfg = dir.join("flat.cfg");
    std::fs::write(&cfg, FLAT).expect("config");
    for &(program, hex) in MEASURED {
        let (src, obj, bin) = (dir.join("t.s"), dir.join("t.o"), dir.join("t.bin"));
        std::fs::write(&src, program).expect("source");
        let assembled = Command::new("ca65").arg(&src).arg("-o").arg(&obj).status();
        let native = match assembled {
            Ok(status) if status.success() => {
                let linked = Command::new("ld65")
                    .arg("-C")
                    .arg(&cfg)
                    .arg(&obj)
                    .arg("-o")
                    .arg(&bin)
                    .status()
                    .expect("ld65");
                linked
                    .success()
                    .then(|| verdict_corpus::encode_hex(&std::fs::read(&bin).expect("image")))
            }
            Ok(_) => None,
            Err(error) => panic!("ca65 is required: {error}"),
        };
        assert_eq!(
            native.as_deref(),
            hex,
            "{}",
            String::from_utf8_lossy(program)
        );
    }
}

/// The command line reads a ca65 file as bytes, and still refuses a file
/// that is not UTF-8 for a dialect that reads text.
#[test]
fn the_command_line_reads_ca65_files_as_bytes() {
    let dir = support::scratch::dir("ca65-source-bytes-cli");
    let cfg = dir.join("flat.cfg");
    std::fs::write(&cfg, FLAT).expect("config");
    let source = dir.join("latin1.s");
    std::fs::write(&source, b".byte \"caf\xe9\"\n").expect("source");
    let out = dir.join("latin1.bin");
    let run = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .args(["--dialect", "ca65", "-C"])
        .arg(&cfg)
        .arg(&source)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("CLI");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(std::fs::read(&out).expect("image"), b"caf\xe9");

    let refused = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .args(["--dialect", "acme"])
        .arg(&source)
        .arg("-o")
        .arg(dir.join("acme.bin"))
        .output()
        .expect("CLI");
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("valid UTF-8"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
}

/// Messages and listings show a file that is not UTF-8 as Lang198x's shared
/// decoder reads it, so the two cannot disagree about a byte.
#[test]
fn shown_text_matches_the_shared_decoder() {
    use lang198x_ca65::syntax::{CharSet, decode};
    let every_byte: Vec<u8> = (0..=255).collect();
    let decoded = decode(&every_byte, CharSet::Latin1).expect("decodes");
    assert_eq!(asm198x::source::shown(&every_byte), decoded.text());
}
