//! Lua/pass integration, pinned against SjASMPlus 1.21.0.

#[cfg(feature = "lua")]
#[path = "support/tool_identity.rs"]
mod tool_identity;

#[cfg(feature = "lua")]
const SINE_TABLE: &str = include_str!("fixtures/lua/lua_sin_table.asm");

#[cfg(feature = "lua")]
fn corpus_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lua/verdicts.ndjson")
}

#[cfg(feature = "lua")]
#[test]
fn committed_lua_verdicts_replay_without_reference_tools() {
    let corpus = verdict_corpus::Corpus::read(&corpus_path()).expect("Lua verdict corpus");
    assert!(
        corpus.verdicts().any(|v| v.source == SINE_TABLE),
        "missing upstream example evidence"
    );
    for verdict in corpus.verdicts() {
        let expected = verdict.outcome.bytes().expect("byte verdict");
        let result = asm198x::assemble_sjasmplus(&verdict.source)
            .unwrap_or_else(|e| panic!("{}: {e}", verdict.case));
        assert_eq!(result.bytes, expected, "{}", verdict.case);
        let formatted = asm198x::format_sjasmplus(&verdict.source).expect("format");
        assert_eq!(
            asm198x::format_sjasmplus(&formatted).expect("format twice"),
            formatted
        );
        assert_eq!(
            asm198x::assemble_sjasmplus(&formatted)
                .expect("formatted source")
                .bytes,
            expected
        );
    }
    for (_, source, _) in CASES {
        assert!(
            corpus.verdicts().any(|v| v.source == *source),
            "unrecorded probe"
        );
    }
}

#[cfg(feature = "lua")]
const CASES: &[(&str, &str, &[u8])] = &[
    (
        "macro substitution does not rewrite Lua identifiers",
        " MACRO emit FOO\n LUA ALLPASS\n local FOO = 7\n sj.add_byte(FOO)\n ENDLUA\n ENDM\n emit 3\n",
        &[7],
    ),
    (
        "macro argument lookup and restoration",
        " MACRO inner FOO,BAR\n LUA ALLPASS\n assert(sj.get_define('FOO', true) == 'arg1')\n assert(sj.get_define('FOO', false) == 'abcd')\n assert(sj.get_define('FOO') == 'abcd')\n assert(sj.get_define('BAR', true) == 'arg2')\n assert(sj.get_define('BAR', false) == nil)\n assert(sj.get_define('BAZ', true) == 'efgh')\n sj.add_byte(7)\n ENDLUA\n ENDM\n MACRO outer FOO\n inner arg1,arg2\n LUA ALLPASS\n assert(sj.get_define('FOO', true) == 'outerarg')\n sj.add_byte(9)\n ENDLUA\n ENDM\n DEFINE FOO abcd\n DEFINE BAZ efgh\n outer outerarg\n LUA ALLPASS\n assert(sj.get_define('FOO', true) == 'abcd')\n assert(sj.get_define('BAR', true) == nil)\n ENDLUA\n",
        &[7, 9],
    ),
    (
        "pass1 labels persist",
        " LUA PASS1\n sj.insert_label('value', 23)\n ENDLUA\n db value\n",
        &[23],
    ),
    (
        "module symbols",
        " MODULE one\n LUA ALLPASS\n assert(sj.get_modules() == 'one')\n assert(sj.insert_label('start', 3))\n assert(sj.insert_label('.local', 4))\n assert(sj.get_label('.local') == 4)\n sj.add_byte(_c('start + .local'))\n ENDLUA\n ENDMODULE\n db one.start,one.start.local\n",
        &[7, 3, 4],
    ),
    (
        "bank switching",
        " device zxspectrum128\n org $c000\n LUA ALLPASS\n assert(sj.set_slot(0xc000))\n assert(sj.set_page(6))\n sj.add_byte(42)\n assert(sj.set_page(7))\n _pc('org $c000')\n sj.add_byte(99)\n ENDLUA\n LUA\n assert(sj.get_byte(0xc000) == 99)\n assert(sj.set_page(6))\n sj.add_byte(sj.get_byte(0xc000))\n ENDLUA\n",
        &[42, 99, 42],
    ),
    (
        "lua in repetition",
        " DUP 2\n LUA ALLPASS\n sj.add_byte(7)\n ENDLUA\n EDUP\n",
        &[7, 7],
    ),
    (
        "lua in macro",
        " MACRO emit\n LUA ALLPASS\n sj.add_byte(9)\n ENDLUA\n ENDM\n emit\n emit\n",
        &[9, 9],
    ),
    (
        "globals and immediate reads",
        " device zxspectrum48\n org $8000\n LUA PASS1\n count = 1; saved = sj.add_byte\n ENDLUA\n LUA PASS2\n count = count + 1\n ENDLUA\n LUA ALLPASS\n _pl('start: ld a,7')\n assert(sj.current_address == 0x8002)\n sj.add_byte(sj.get_label('start') % 256)\n ENDLUA\n LUA PASS3\n saved(count)\n sj.add_byte(sj.get_byte(0x8001))\n ENDLUA\n",
        &[0x3e, 7, 0, 2, 7],
    ),
    (
        "all passes count",
        " LUA ALLPASS\n n = (n or 0) + 1\n sj.add_byte(n)\n ENDLUA\n",
        &[3],
    ),
    (
        "skipped lua",
        " IF 0\n LUA\n error('unreachable')\n ENDLUA\n ENDIF\n db 42\n",
        &[42],
    ),
    (
        "lua punctuation",
        " LUA ALLPASS\n local s = 'a;b:c//d'\n sj.add_byte(#s)\n sj.add_byte(('abc'):byte(2))\n ENDLUA\n",
        &[8, 98],
    ),
    (
        "defines and labels",
        " LUA ALLPASS\n assert(sj.insert_define('V', '7'))\n assert(not sj.insert_define('V', '8'))\n assert(sj.get_define('V') == '8')\n assert(sj.insert_label('foo', 12))\n sj.add_word(_c('foo + V'))\n ENDLUA\n db foo,V\n",
        &[20, 0, 12, 8],
    ),
    (
        "forward calculation",
        " LUA ALLPASS\n sj.add_byte(_c('later'))\n ENDLUA\nlater: db 42\n",
        &[1, 42],
    ),
    (
        "live device read",
        " device zxspectrum48\n org $8000\n db 23\n LUA\n assert(sj.get_byte(0x8000) == 23)\n sj.add_byte(sj.get_byte(0x8000))\n _pc('org $8000')\n sj.add_byte(99)\n assert(sj.get_byte(0x8000) == 99)\n ENDLUA\n",
        &[23, 23, 99],
    ),
];

#[cfg(feature = "lua")]
#[test]
fn lua_matches_pinned_reference_bytes() {
    for (name, source, bytes) in CASES {
        let result = asm198x::assemble_sjasmplus(source).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(&result.bytes, bytes, "{name}");
    }
}

#[cfg(feature = "lua")]
#[test]
fn includes_share_the_interpreter_and_load_lazily() {
    use asm198x::source::MemoryLoader;
    let loader = MemoryLoader::new().text("lib.lua", "function emit() sj.add_byte(7) end");
    let source = " INCLUDELUA \"lib.lua\"\n LUA ALLPASS\n emit()\n ENDLUA\n IF 0\n INCLUDELUA \"missing.lua\"\n ENDIF\n";
    let result =
        asm198x::assemble_sjasmplus_files(source, "main.asm", &loader).expect("included Lua");
    assert_eq!(result.bytes, [7]);
}

#[cfg(feature = "lua")]
#[test]
fn catching_a_host_error_does_not_make_the_assembly_succeed() {
    for call in [
        "sj.error('stop')",
        "sj.exit('stop')",
        "_pc('not_an_instruction')",
        "sj.get_label('missing')",
        "sj.shellexec('anything')",
    ] {
        let source = format!(" LUA\n pcall(function() {call} end)\n ENDLUA\n");
        let error = asm198x::assemble_sjasmplus(&source).expect_err("host error remains fatal");
        assert!(error.to_string().contains("[LUA]"), "{error}");
    }
}

#[cfg(feature = "lua")]
#[test]
fn host_generated_work_is_bounded() {
    for code in ["ds 1000000000", "DUP 1000000000\\n EDUP"] {
        let source = format!(" LUA\n _pc('{code}')\n ENDLUA\n");
        let error = asm198x::assemble_sjasmplus(&source).expect_err("host work is bounded");
        assert!(error.to_string().contains("budget"), "{error}");
    }
}

#[cfg(feature = "lua")]
#[test]
fn machine_code_outside_allpass_has_an_acknowledgeable_warning() {
    for (mode, comment, expected) in [
        ("", "", true),
        ("ALLPASS", "", false),
        ("PASS3", "; luamc-ok", false),
    ] {
        let source = format!(" LUA {mode} {comment}\n sj.add_byte(7)\n ENDLUA\n");
        let result = asm198x::assemble_sjasmplus(&source).expect("Lua emits");
        assert_eq!(
            result.warnings.iter().any(|w| w.message.contains("luamc")),
            expected
        );
    }
}

#[cfg(feature = "lua")]
#[test]
fn lua_formatting_preserves_foreign_syntax() {
    for (name, source, _) in CASES {
        let formatted = asm198x::format_sjasmplus(source).expect("format");
        assert_eq!(
            asm198x::format_sjasmplus(&formatted).expect("format twice"),
            formatted,
            "{name}"
        );
        assert_eq!(
            asm198x::assemble_sjasmplus(source).expect("original").bytes,
            asm198x::assemble_sjasmplus(&formatted)
                .expect("formatted")
                .bytes,
            "{name}"
        );
    }
}

#[cfg(feature = "lua")]
#[test]
#[ignore = "requires installed SjASMPlus 1.21.0"]
fn lua_reference_probes() {
    use std::{fs, process::Command};
    use verdict_corpus::{Arbiter, Corpus, Outcome, Record, Suite, Verdict, encode_hex};
    let identity = tool_identity::identify("sjasmplus").expect("reference identity");
    assert!(
        identity.identity.contains("v1.21.0"),
        "unexpected reference version"
    );
    let path = corpus_path();
    let previous = if path.exists() {
        Some(Corpus::read(&path).expect("existing corpus"))
    } else {
        None
    };
    let mut records = Vec::new();
    let dir = std::env::temp_dir().join(format!("asm198x-lua-reference-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("probe directory");
    let cases = CASES
        .iter()
        .map(|&(name, source, bytes)| (name, source, Some(bytes)))
        .chain(std::iter::once(("upstream sine table", SINE_TABLE, None)));
    for (index, (name, source, expected)) in cases.enumerate() {
        let input = dir.join(format!("{index}.asm"));
        let output = dir.join(format!("{index}.bin"));
        fs::write(&input, source).expect("source");
        let result = Command::new("sjasmplus")
            .arg(format!("--raw={}", output.display()))
            .arg(&input)
            .output()
            .expect("sjasmplus");
        assert!(
            result.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let bytes = fs::read(output).expect("binary");
        if let Some(expected) = expected {
            assert_eq!(&bytes, expected, "{name}");
        }
        assert_eq!(
            asm198x::assemble_sjasmplus(source)
                .expect("assemble reference source")
                .bytes,
            bytes,
            "{name}"
        );
        let verdict = Verdict {
            suite: Suite::Probe,
            cpu: "z80".into(),
            dialect: "sjasmplus".into(),
            case: name.into(),
            source: source.into(),
            arbiter: Arbiter {
                tool: identity.tool.clone(),
                identity: identity.identity.clone(),
                digest: identity.digest.clone(),
            },
            outcome: Outcome::Bytes {
                hex: encode_hex(&bytes),
            },
        };
        if !previous
            .as_ref()
            .is_some_and(|c| c.verdicts().any(|v| v == &verdict))
        {
            records.push(Record::Verdict(Box::new(verdict)));
        }
    }
    verdict_corpus::append(&path, &records).expect("record observed reference bytes");
    fs::remove_dir_all(dir).expect("remove own probe directory");
}

#[cfg(not(feature = "lua"))]
#[test]
fn lua_without_the_feature_is_refused_by_name() {
    let error = asm198x::assemble_sjasmplus(" LUA\n sj.add_byte(1)\n ENDLUA\n")
        .expect_err("feature disabled");
    assert!(error.to_string().contains("not implemented"), "{error}");
}
