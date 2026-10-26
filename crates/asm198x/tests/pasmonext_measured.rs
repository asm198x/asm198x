//! PasmoNext 0.1.3 measured on whole programs: the curriculum's programs with
//! the files they read and the options they were run with, and the behaviour
//! a verdict cannot record (messages, warnings, SAVEBIN, three passes).
//!
//! The verdicts themselves replay in `verdict_replay.rs`.

mod support;

/// Measured programs Asm198x does not match yet, each with what it
/// waits for. Three-pass programs are not listed: they are found by the
/// fixture's `--- passes 3` and must be reported as such.
const PROGRAMS_NOT_YET: &[(&str, &str)] = &[];

/// Measured programs where PasmoNext 0.1.3 has an obvious bug, which the new
/// path fixes rather than copies, under 198x's
/// `reference-bugs-are-fixed-not-copied.md`: each name, why, and what the new
/// path must do instead, its bytes or `None` for a rejection. One that stops
/// deviating fails the test.
const PROGRAMS_DEVIATING: &[(&str, &str, Option<&str>)] = &[
    (
        "org_ignores_rest",
        "PasmoNext never reads past ORG's operand, so `org 5 junk` drops `junk`",
        None,
    ),
    // The manual: the `$` in a name "are ignored". PasmoNext's `stripdollar`
    // removes the wrong characters, so it reads `ab$c` as `ac`, `ab$` as `a`.
    (
        "name_dollar_trailing",
        "PasmoNext reads `ab$` as `a`, so `jp ab` finds nothing",
        Some("00C30000"),
    ),
    (
        "name_dollar_inside_read_as_manual",
        "PasmoNext reads `ab$c` as `ac`, so `jp abc` finds nothing",
        Some("00C30000"),
    ),
    (
        "name_dollar_inside_pasmonext_reading",
        "PasmoNext reads `ab$c` as `ac`, so `jp ac` reaches it",
        None,
    ),
    // The reserve counter `RB` and `RW` take from.
    (
        "rb_without_rsset",
        "PasmoNext never resets the counter between passes, so pass 2 gives \
         `aa` a new value and ends in a phase error",
        Some("0001"),
    ),
    (
        "rsset_symbol",
        "PasmoNext reads a name after RSSET as 0",
        None,
    ),
    (
        "rsset_expression",
        "PasmoNext reads `2+3` after RSSET as 2",
        None,
    ),
    (
        "rsset_junk_after",
        "PasmoNext drops whatever follows RSSET's number",
        None,
    ),
    (
        "stack_junk_after",
        "PasmoNext drops whatever follows STACK's value",
        None,
    ),
    // Structures.
    (
        "struct_ends_junk",
        "PasmoNext drops whatever follows ENDS",
        None,
    ),
    (
        "struct_name_junk",
        "PasmoNext drops whatever follows STRUCT's name",
        None,
    ),
    (
        "struct_db_trailing_comma",
        "PasmoNext counts the missing item after `db 1,` as a byte",
        None,
    ),
    (
        "struct_db_no_items",
        "PasmoNext counts `fa db`, with no item, as a byte",
        None,
    ),
    // HEX.
    (
        "hex_non_hex",
        "PasmoNext writes a garbage byte for `ZZ`",
        None,
    ),
    (
        "hex_with_h",
        "PasmoNext reads `0ABh` as the pairs `0A` and `Bh`",
        None,
    ),
    (
        "hex_two_items",
        "PasmoNext drops everything after HEX's first word",
        None,
    ),
    (
        "hex_junk",
        "PasmoNext drops everything after HEX's first word",
        None,
    ),
    (
        "hex_string",
        "PasmoNext writes nothing for a string and says nothing",
        None,
    ),
    // SAVEBIN.
    (
        "savebin_paren_size",
        "PasmoNext saves nothing when the size does not start with a number or a name",
        None,
    ),
    (
        "savebin_junk",
        "PasmoNext drops whatever follows SAVEBIN's size",
        None,
    ),
    // PasmoNext keeps one memory image across its passes, so bytes an earlier
    // pass wrote where the last writes nothing stay in the file.
    (
        "equ_forward_then_org",
        "PasmoNext keeps the byte pass 1 wrote at 0, before `org x` moved to 5",
        Some("01"),
    ),
    (
        "three_passes_settle",
        "PasmoNext keeps the byte pass 1 wrote at 0, before `org x` moved to 5",
        Some("01"),
    ),
    (
        "pass3_settles",
        "PasmoNext keeps the byte pass 1 wrote at 0, before `org x` moved to 5",
        Some("01"),
    ),
    (
        "stale_label_defined_check",
        "PasmoNext keeps the byte pass 1 wrote at 1, where the last pass, skipping the IF, writes nothing",
        Some("00"),
    ),
    (
        "pass_three_defined_forward",
        "PasmoNext keeps the byte pass 1 wrote at 0, before `org x` moved to 5",
        Some("FF"),
    ),
    (
        "stale_label_from_pass_one",
        "PasmoNext reads `lab`, which only pass 1 defined, as the value pass 1 gave it",
        None,
    ),
    // PasmoNext reads nothing after an `ELSE` or `ENDIF` it reaches while
    // skipping lines, nor after any `ENDM`, so it drops the text there.
    (
        "else_skipped_junk",
        "PasmoNext drops the text after `ELSE`",
        None,
    ),
    (
        "else_skipped_instruction",
        "PasmoNext drops the text after `ELSE`",
        None,
    ),
    (
        "endif_skipped_junk",
        "PasmoNext drops the text after `ENDIF`",
        None,
    ),
    (
        "endif_nested_skipped_junk",
        "PasmoNext drops the text after `ENDIF`",
        None,
    ),
    (
        "endm_rept_junk",
        "PasmoNext drops the text after `ENDM`",
        None,
    ),
    (
        "endm_rept_zero_junk",
        "PasmoNext drops the text after `ENDM`",
        None,
    ),
    (
        "endm_irp_junk",
        "PasmoNext drops the text after `ENDM`",
        None,
    ),
    (
        "endm_macro_junk",
        "PasmoNext drops the text after `ENDM`",
        None,
    ),
];

/// One measured program: its source, the files it reads, its options, and
/// what PasmoNext did with it.
struct Program {
    name: String,
    source: String,
    files: asm198x::source::MemoryLoader,
    options: Vec<String>,
    bytes: Option<String>,
    three_passes: bool,
}

fn programs() -> Vec<Program> {
    let text = include_str!("fixtures/pasmonext-0.1.3-programs.txt");
    text.split("\n=== ")
        .skip(1)
        .map(|case| {
            let mut lines = case.lines();
            let mut program = Program {
                name: lines.next().unwrap_or_default().to_owned(),
                source: String::new(),
                files: asm198x::source::MemoryLoader::new(),
                options: Vec::new(),
                bytes: None,
                three_passes: false,
            };
            let mut outcome = false;
            for line in lines {
                if let Some(file) = line.strip_prefix("+++ ") {
                    let (name, hex) = file.split_once(' ').unwrap_or((file, ""));
                    let bytes = verdict_corpus::decode_hex(hex).expect("hex");
                    // A file may be read by INCBIN or INCLUDE, so it is
                    // offered both ways.
                    let mut files = std::mem::take(&mut program.files);
                    if let Ok(text) = String::from_utf8(bytes.clone()) {
                        files = files.text(name, text);
                    }
                    program.files = files.binary(name, bytes);
                } else if let Some(options) = line.strip_prefix("!!! ") {
                    program.options = options.split(' ').map(str::to_owned).collect();
                } else if let Some(hex) = line.strip_prefix("--- bytes ") {
                    program.bytes = Some(hex.to_ascii_uppercase());
                    outcome = true;
                } else if line == "--- passes 3" {
                    program.three_passes = true;
                } else if line.starts_with("---") {
                    outcome = true;
                } else if !outcome {
                    program.source.push_str(line);
                    program.source.push('\n');
                }
            }
            program
        })
        .collect()
}

/// PasmoNext's command-line `options` as Asm198x takes them, or `None`
/// if one is not taken yet.
fn options(options: &[String]) -> Option<asm198x::PasmoNextOptions> {
    let mut taken = asm198x::PasmoNextOptions::default();
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "--nocase" => taken.names_in_any_case = true,
            "--alocal" => taken.automatic_locals = true,
            "--bracket" => taken.brackets_only = true,
            "--w8080" => taken.warns_8080 = true,
            "--pass3" => taken.three_passes = true,
            "--equ" | "-E" => taken.predefined.push(options.next()?.clone()),
            _ => return None,
        }
    }
    Some(taken)
}

#[test]
fn measured_programs_match() {
    let programs = programs();
    assert!(programs.len() >= 88, "the fixture has {}", programs.len());
    let mut failures = Vec::new();
    for program in &programs {
        // Programs run with options Asm198x does not take yet, such as
        // `--pass3`, wait for the step that takes them.
        let Some(options) = options(&program.options) else {
            continue;
        };
        let result = asm198x::assemble_pasmo_files_with(
            &program.source,
            "t.asm",
            &program.files,
            true,
            &options,
        );
        let named = PROGRAMS_NOT_YET
            .iter()
            .any(|(name, _)| *name == program.name);
        if let Some((_, why, expected)) = PROGRAMS_DEVIATING
            .iter()
            .find(|(name, _, _)| *name == program.name)
        {
            let ours = result.as_ref().ok().map(|r| {
                r.bytes
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<String>()
            });
            if ours.as_deref() != *expected {
                failures.push(format!(
                    "{}: gives {ours:?}, but should deviate to {expected:?}: {why}",
                    program.name
                ));
            }
            continue;
        }
        let ok = {
            let ours = result.as_ref().ok().map(|r| {
                r.bytes
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<String>()
            });
            // PasmoNext says when a value moving in pass 2 makes it run a
            // third; the walk says so too.
            let switched = result.as_ref().is_ok_and(|r| {
                r.warnings
                    .iter()
                    .any(|w| w.message == "Switching to 3 pass mode")
            });
            ours == program.bytes && (result.is_err() || switched == program.three_passes)
        };
        if ok == named {
            failures.push(format!(
                "{}: {}",
                program.name,
                if named {
                    "matches; take it off PROGRAMS_NOT_YET".to_owned()
                } else {
                    match &result {
                        Ok(r) => format!(
                            "differs: gives {}, PasmoNext {:?}",
                            r.bytes
                                .iter()
                                .map(|b| format!("{b:02X}"))
                                .collect::<String>(),
                            program.bytes
                        ),
                        Err(e) => format!(
                            "differs: rejects it ({}), PasmoNext {:?}",
                            e.error.message, program.bytes
                        ),
                    }
                }
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn includes_resolve_from_the_folder_pasmonext_runs_in() {
    // Laid out like the pattern library's `mistakes/` programs, which
    // include `caller.inc` from the pattern folder they are run from.
    // PasmoNext 0.1.3, run there on `mistakes/broken.asm`, writes 3E 07 00.
    let dir = support::scratch::dir("pasmonext-run-folder");
    std::fs::create_dir_all(dir.join("mistakes")).expect("mistakes folder");
    std::fs::write(dir.join("caller.inc"), " ld a,7\n").expect("caller.inc");
    std::fs::write(dir.join("data.bin"), [0]).expect("data.bin");
    let source = " org 0\n include \"caller.inc\"\n incbin \"data.bin\"\n";
    std::fs::write(dir.join("mistakes/broken.asm"), source).expect("broken.asm");
    let loader = asm198x::source::FsLoader::new(&*dir, Vec::new());
    let result =
        asm198x::assemble_pasmo_files(source, "mistakes/broken.asm", &loader).expect("assembles");
    assert_eq!(result.bytes, [0x3E, 0x07, 0x00]);
}

#[test]
fn messages_and_warnings_say_what_pasmonext_printed() {
    // Measured on PasmoNext 0.1.3: what each program printed. PasmoNext
    // gives a `.WARNING` on both passes; Asm198x gives it once.
    let said = |source: &str| -> Vec<(String, bool)> {
        let result = asm198x::assemble_pasmonext(source).expect("assembles");
        result
            .warnings
            .iter()
            .map(|w| (w.message.clone(), !w.kind.is_advisory()))
            .collect()
    };
    let note = |text: &str| vec![(text.to_owned(), true)];
    assert_eq!(said(" MESSAGE \"hello\"\n db 1\n"), note("hello"));
    assert_eq!(said("vv equ 1234h\n MESSAGE \"v=\",vv\n"), note("v=1234"));
    assert_eq!(said(" org 8000h\n nop\n MESSAGE $\n"), note("8001"));
    assert_eq!(said(" org 10h\n MESSAGE PC\n"), note("0010"));
    assert_eq!(said(" MESSAGE 10\n"), note("000A"));
    assert_eq!(said(" MESSAGE later\nlater equ 7\n"), note("0007"));
    assert_eq!(said("vv equ 100\n MESSAGE %d vv\n"), note("100"));
    assert_eq!(
        said("vv equ 100\nww equ 255\n MESSAGE %d vv,ww\n"),
        note("10000FF")
    );
    assert_eq!(said(" MESSAGE a\n"), note("A"));
    assert_eq!(said(" MESSAGE 'q'\n"), note("q"));
    assert_eq!(said(" MESSAGE\n"), note(""));
    assert_eq!(
        said("mm MACRO vv\n MESSAGE \"v\",vv\n ENDM\n mm 3\n"),
        note("v0003")
    );
    assert_eq!(
        said(" .WARNING careful now\n db 1\n"),
        vec![("careful now".to_owned(), false)]
    );
    // PasmoNext reads `ld b,(12h)` as `ld b,12h`, as its manual says, and
    // warns about it, in its second pass only.
    assert_eq!(
        said(" ld b,(12h)\n ld b,(1)+1\n"),
        vec![("looks like a non existent instruction".to_owned(), false)]
    );
    let error = |source: &str| {
        asm198x::assemble_pasmonext(source)
            .expect_err("rejected")
            .message
    };
    assert_eq!(
        error(" nop\n .ERROR something went wrong\n"),
        ".ERROR directive: something went wrong"
    );
    assert_eq!(
        error(" .ERROR bad ; here\n"),
        ".ERROR directive: bad ; here"
    );
    assert_eq!(error(" MESSAGE nope\n"), "Symbol 'nope' is undefined");
    assert_eq!(error(" MESSAGE \"a\" \"b\"\n"), "Error in Message format");
}

#[test]
fn savebin_writes_what_pasmonext_wrote() {
    // Measured on PasmoNext 0.1.3: the file each program saved. Memory
    // stands as it is when the last pass reaches the line, over what the
    // first pass left: bytes written further on are there, but a later
    // overwrite is not.
    for (source, saved) in [
        (
            " org 8000h\n db 1,2,3,4\n SAVEBIN \"out.bin\",8001h,2\n",
            "0203",
        ),
        (
            " SAVEBIN \"out.bin\",8000h,2\n org 8000h\n db 7,8\n",
            "0708",
        ),
        (
            " org 8000h\n db 1\n SAVEBIN \"out.bin\",8000h,3\n",
            "010000",
        ),
        (" org 0\n db 5\n SAVEBIN \"out.bin\",0,0\n", ""),
        (
            " SAVEBIN \"out.bin\",start,2\nstart equ 0\n db 9,9\n",
            "0909",
        ),
        (
            " org 0\n db 1,2\n SAVEBIN \"out.bin\",0,2\n org 0\n db 3,4\n",
            "0102",
        ),
        ("here SAVEBIN \"out.bin\",0,1\n", "00"),
    ] {
        let result = asm198x::assemble_pasmonext(source).expect("assembles");
        let files: Vec<(String, String)> = result
            .artifacts
            .iter()
            .map(|a| {
                (
                    a.name.clone(),
                    a.bytes.iter().map(|b| format!("{b:02x}")).collect(),
                )
            })
            .collect();
        assert_eq!(
            files,
            [("out.bin".to_owned(), saved.to_owned())],
            "{source}"
        );
    }
    let range =
        asm198x::assemble_pasmonext(" SAVEBIN \"out.bin\",0FFFFh,2\n").expect_err("out of range");
    assert_eq!(range.message, "Error SAVEBIN out of range");
}

#[test]
fn w8080_warns_once_about_each_z80_instruction() {
    // Measured on PasmoNext 0.1.3 with `--w8080`: it warned on lines 3, 4
    // and 5, on both passes. Line 5, `out (12h),a`, is the 8080's own `OUT`,
    // and line 6, the Next's `jp (c)`, is not an 8080 instruction; the new
    // path follows the rule for both, under 198x's decision to fix
    // reference assemblers' bugs rather than copy them.
    let source = " org 0\n ld a,b\n djnz $\n exx\n out (12h),a\n jp (c)\n db 1\n";
    let options = asm198x::PasmoNextOptions {
        warns_8080: true,
        ..Default::default()
    };
    let result = asm198x::assemble_pasmo_files_with(
        source,
        "t.asm",
        &asm198x::source::MemoryLoader::new(),
        true,
        &options,
    )
    .expect("assembles");
    let warned: Vec<(usize, &str)> = result
        .warnings
        .iter()
        .map(|w| (w.line, w.message.as_str()))
        .collect();
    let not_8080 = "not a 8080 instruction";
    assert_eq!(warned, [(3, not_8080), (4, not_8080), (6, not_8080)]);
    let quiet = asm198x::assemble_pasmonext(source).expect("assembles");
    assert!(quiet.warnings.is_empty());
}

#[test]
fn three_passes_say_why_and_refuse_what_only_an_earlier_pass_defined() {
    // Measured on PasmoNext 0.1.3: `x` moves from 0 to 5 in pass 2, so it
    // says "Switching to 3 pass mode" there, on the line that moved it, and
    // runs a third pass.
    let result = asm198x::assemble_pasmonext("x equ later\n org x\n db 1\nlater equ 5\n")
        .expect("assembles");
    let said: Vec<(usize, &str)> = result
        .warnings
        .iter()
        .map(|w| (w.line, w.message.as_str()))
        .collect();
    assert_eq!(said, [(1, "Switching to 3 pass mode")]);
    assert_eq!(result.bytes, [0x01]);
    // Only pass 1, where `x` was 0, defines `lab`; PasmoNext writes `dw lab`
    // with the value pass 1 gave it.
    let source = " org 0\nx equ later\n if x = 0\nlab: db 1\n endif\n dw lab\nlater equ 5\n";
    let error = asm198x::assemble_pasmonext(source).expect_err("refused");
    assert_eq!(error.line, 6);
    assert!(
        error
            .message
            .contains("'lab' is defined only in an earlier pass"),
        "{}",
        error.message
    );
}
