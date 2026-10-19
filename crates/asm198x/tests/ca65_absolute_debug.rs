use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

#[path = "support/scratch.rs"]
mod scratch;

fn run(root: &Path, args: &[&str]) -> Output {
    let stdout = root.join("cli.stdout");
    let stderr = root.join("cli.stderr");
    let mut child = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(root)
        .args(args)
        .stdout(Stdio::from(
            std::fs::File::create(&stdout).expect("stdout log"),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(&stderr).expect("stderr log"),
        ))
        .spawn()
        .expect("spawn CLI");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().expect("poll CLI").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill timed-out CLI");
            child.wait().expect("reap timed-out CLI");
            panic!("CLI failed to complete within five seconds: {args:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Output {
        status: child.wait().expect("completed CLI"),
        stdout: std::fs::read(stdout).expect("stdout"),
        stderr: std::fs::read(stderr).expect("stderr"),
    }
}

fn relative(path: &str) -> String {
    let path = path.replace('\\', "/");
    if let Some((_, tail)) = path.rsplit_once("/sub/") {
        format!("sub/{tail}")
    } else {
        path.rsplit('/').next().expect("filename").to_string()
    }
}

#[test]
fn native_labels_and_physical_source_ranges_agree() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-absolute-debug.json"))
            .expect("native fixture");
    let cases = facts["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 39);
    let root = scratch::dir("ca65-absolute-debug");
    for (name, text) in facts["files"].as_object().expect("includes") {
        std::fs::create_dir_all(root.join(name).parent().expect("parent")).expect("directories");
        std::fs::write(root.join(name), text.as_str().expect("include")).expect("include");
    }
    let mut labels = 0;
    let mut lines = 0;
    for fact in cases {
        let name = fact["name"].as_str().expect("case name");
        std::fs::write(
            root.join("input.s"),
            fact["source"].as_str().expect("source"),
        )
        .expect("source");
        std::fs::write(
            root.join("input.cfg"),
            fact["config"].as_str().expect("config"),
        )
        .expect("config");
        let output = run(
            &root,
            &[
                "--dialect",
                "ca65",
                "-C",
                "input.cfg",
                "input.s",
                "-o",
                "out.bin",
                "--debug=out.debug198x",
                "--listing-json=out.lst.json",
            ],
        );
        let accepted = fact["native"]
            .as_array()
            .expect("native stages")
            .last()
            .expect("stage")["exit"]
            == 0;
        assert_eq!(
            output.status.code(),
            Some(if accepted { 0 } else { 1 }),
            "{name}: {output:?}"
        );
        if !accepted {
            continue;
        }
        let bytes = std::fs::read(root.join("out.bin")).expect("image");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            fact["native_hex"].as_str().expect("native bytes"),
            "{name}"
        );
        let text = std::fs::read_to_string(root.join("out.debug198x")).expect("sidecar");
        let debug = debug198x::DebugInfo::read(&text).expect("official reader");
        for (symbol, value) in fact["symbol_values"].as_object().expect("symbols") {
            labels += 1;
            let expected = if fact["non_cpu_symbols"].get(symbol).is_some() {
                None
            } else {
                value.as_u64()
            };
            assert_eq!(debug.addr_of(symbol, None), expected, "{name}: {symbol}");
            if let Some(address) = expected {
                let found = debug
                    .symbol_at(address, None)
                    .expect("official address lookup");
                assert_eq!(debug.addr_of(&found.name, None), Some(address));
            }
        }
        let listing: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("out.lst.json")).expect("listing"),
        )
        .expect("listing JSON");
        let mut actual: Vec<_> = listing["lines"].as_array().expect("lines").iter().map(|line| {
            lines += 1;
            serde_json::json!({"file": relative(line["file"].as_str().expect("file")), "line": line["line"], "file_offset": line["file_offset"], "length": line["bytes"], "cycles": line.get("cycles")})
        }).collect();
        let mut expected = fact["expected_lines"]
            .as_array()
            .expect("expected lines")
            .clone();
        actual.sort_by_key(serde_json::Value::to_string);
        expected.sort_by_key(serde_json::Value::to_string);
        assert_eq!(
            actual, expected,
            "{name}: physical source ranges and cycles"
        );
        let mut records: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).expect("record"))
            .collect();
        for record in &mut records {
            if record["t"] == "header" {
                for path in record["sources"].as_array_mut().expect("source paths") {
                    *path = relative(path.as_str().expect("path")).into();
                }
            }
            if record["t"] == "line" {
                record["file"] = relative(record["file"].as_str().expect("file")).into();
            }
        }
        if name == "no_origin" {
            let mut baseline = facts["no_origin_records"].clone();
            baseline[0]["tool_version"] = env!("CARGO_PKG_VERSION").into();
            assert_eq!(serde_json::json!(records), baseline);
        }
        let symbols: Vec<_> = records
            .iter()
            .filter(|r| r["t"] == "symbol" && r["kind"] == "label")
            .collect();
        assert_eq!(
            symbols.len(),
            fact["symbol_values"].as_object().expect("symbols").len(),
            "{name}: label kinds"
        );
        for section in &debug.sections {
            assert!(section.space.is_none(), "{name}: fabricated space");
            if section.id < 7 {
                assert_eq!(fact_config_id(&facts, &section.name), section.id);
            } else {
                assert!(
                    facts["config_ids"].get(&section.name).is_some(),
                    "{name}: source section identity"
                );
            }
        }
        if name == "empty_absolute" {
            assert!(debug.sections.iter().all(|s| s.id < 7));
        }
        for symbol in symbols {
            let symbol_name = symbol["name"].as_str().expect("symbol name");
            let id = symbol["section"].as_u64().expect("section");
            let section = listing["sections"]
                .as_array()
                .expect("sections")
                .iter()
                .find(|s| s["id"] == id)
                .expect("symbol section");
            assert!(symbol.get("space").is_none());
            if fact["non_cpu_symbols"].get(symbol_name).is_some() {
                assert!(section["base"].is_null());
                let bases = debug198x::BaseMap::from([(id as u32, 0x8000)]);
                assert_eq!(
                    debug.addr_of(symbol_name, Some(&bases)),
                    Some(0x8000 + symbol["offset"].as_u64().expect("offset"))
                );
            }
            if fact["no_file_symbols"]
                .as_array()
                .expect("unmapped")
                .iter()
                .any(|s| s == symbol_name)
            {
                assert!(section["file_offset"].is_null());
            }
            if fact["no_file_symbols"]
                .as_array()
                .expect("unmapped")
                .iter()
                .chain(
                    fact["metadata_only_symbols"]
                        .as_array()
                        .expect("metadata only"),
                )
                .any(|s| s == symbol_name)
            {
                assert!(
                    listing["lines"]
                        .as_array()
                        .expect("lines")
                        .iter()
                        .all(|l| l["section"] != id)
                );
            }
            if !fact["non_cpu_symbols"]
                .as_object()
                .expect("non CPU")
                .contains_key(symbol_name)
            {
                let value = fact["symbol_values"][symbol_name]
                    .as_u64()
                    .expect("native value");
                let definitions = fact["symbol_definitions"][symbol_name]
                    .as_array()
                    .expect("definitions");
                let associated: Vec<_> = listing["lines"]
                    .as_array()
                    .expect("lines")
                    .iter()
                    .filter(|line| {
                        definitions.iter().any(|definition| {
                            definition["file"] == relative(line["file"].as_str().expect("file"))
                                && definition["line"] == line["line"]
                        })
                    })
                    .collect();
                if !associated.is_empty() {
                    assert!(
                        associated.iter().any(|l| l["address"] == value),
                        "{name}: {symbol_name} source co-location"
                    );
                }
            }
        }
        for line in listing["lines"].as_array().expect("lines") {
            let file = relative(line["file"].as_str().expect("file"));
            let at = line["file_offset"].as_u64().expect("physical offset");
            let length = line["bytes"].as_u64().expect("length");
            assert!(
                fact["physical_lines"]
                    .as_array()
                    .expect("native spans")
                    .iter()
                    .any(|span| {
                        span["file"] == file
                            && span["line"] == line["line"]
                            && span["file_offset"].as_u64().is_some_and(|start| {
                                start <= at
                                    && start + span["length"].as_u64().expect("native length")
                                        >= at + length
                            })
                    }),
                "{name}: native physical span: {line}"
            );
            if name == "pc_data_lines" {
                let at = at as usize;
                assert_eq!(
                    line["address"],
                    u16::from_le_bytes([bytes[at], bytes[at + 1]])
                );
                assert!(
                    debug
                        .line_at(line["address"].as_u64().expect("address"), None)
                        .is_some()
                );
            }
        }
        let actual_constants: std::collections::BTreeMap<_, _> = records
            .iter()
            .filter(|r| r["t"] == "symbol" && r["kind"] == "const")
            .map(|r| {
                (
                    r["name"].as_str().expect("constant").to_string(),
                    r["value"].clone(),
                )
            })
            .collect();
        assert_eq!(
            serde_json::json!(actual_constants),
            fact["constant_values"],
            "{name}: constants retain kind"
        );
        // Missing section lookup must stay unresolved; zero is a valid base.
        let mut missing = debug.clone();
        missing.sections.clear();
        for symbol in fact["symbol_values"].as_object().expect("symbols").keys() {
            assert_eq!(missing.addr_of(symbol, None), None);
        }
        let costs = listing["labels"].as_array().expect("costs");
        for (symbol, expected) in fact["cycle_costs"].as_object().expect("expected costs") {
            let cost = costs
                .iter()
                .find(|c| c["name"] == symbol.as_str())
                .expect("routine cost");
            assert_eq!(cost["cycles"]["min"], expected["min"], "{name}: {symbol}");
            assert_eq!(cost["cycles"]["max"], expected["max"], "{name}: {symbol}");
            assert_eq!(cost["bytes"], expected["bytes"], "{name}: {symbol}");
        }
        let formatted = run(&root, &["fmt", "--dialect", "ca65", "input.s"]);
        assert!(formatted.status.success(), "{name}: {formatted:?}");
        std::fs::write(root.join("formatted.s"), &formatted.stdout).expect("formatted source");
        let again = run(&root, &["fmt", "--dialect", "ca65", "formatted.s"]);
        assert_eq!(again.status.code(), Some(0));
        assert_eq!(formatted.stdout, again.stdout, "{name}: idempotence");
        let assembled = run(
            &root,
            &[
                "--dialect",
                "ca65",
                "-C",
                "input.cfg",
                "formatted.s",
                "-o",
                "out.bin",
                "--debug=out.debug198x",
            ],
        );
        assert!(assembled.status.success(), "{name}: {assembled:?}");
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("formatted image"),
            bytes
        );
        let formatted_debug = debug198x::DebugInfo::read(
            &std::fs::read_to_string(root.join("out.debug198x")).expect("formatted debug"),
        )
        .expect("official reader");
        assert_eq!(
            formatted_debug.sections, debug.sections,
            "{name}: formatted sections"
        );
        assert_eq!(
            formatted_debug.symbols, debug.symbols,
            "{name}: formatted symbols"
        );
    }
    assert_eq!(labels, 121);
    assert_eq!(lines, 213);
}

fn fact_config_id(facts: &serde_json::Value, name: &str) -> u32 {
    facts["config_ids"][name].as_u64().expect("configured ID") as u32
}

#[test]
fn routine_budgets_cross_logical_spans_and_preserve_outputs_on_failure() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-absolute-debug.json")).expect("fixture");
    let root = scratch::dir("ca65-logical-budget");
    let mut checked = 0;
    for fact in facts["cases"].as_array().expect("cases") {
        let name = fact["name"].as_str().expect("name");
        let costs = fact["cycle_costs"].as_object().expect("costs");
        if costs.is_empty() {
            continue;
        }
        for (path, text) in facts["files"].as_object().expect("includes") {
            std::fs::create_dir_all(root.join(path).parent().expect("parent"))
                .expect("directories");
            std::fs::write(root.join(path), text.as_str().expect("include")).expect("include");
        }
        std::fs::write(
            root.join("input.cfg"),
            fact["config"].as_str().expect("config"),
        )
        .expect("config");
        let source = fact["source"].as_str().expect("source");
        for (symbol, cost) in costs {
            // Qualified names are already rejected by the shared budget parser;
            // their full totals remain covered by the listing assertions above.
            if symbol.contains("::") {
                continue;
            }
            checked += 1;
            let limit = cost["max"].as_u64().expect("finite cost");
            let execute = || {
                run(
                    &root,
                    &[
                        "--dialect",
                        "ca65",
                        "-C",
                        "input.cfg",
                        "input.s",
                        "-o",
                        "out.bin",
                        "--sym=out.sym",
                        "--debug=out.debug198x",
                        "--listing=out.lst",
                        "--listing-json=out.lst.json",
                    ],
                )
            };
            let outputs = [
                "out.bin",
                "out.sym",
                "out.debug198x",
                "out.lst",
                "out.lst.json",
            ];
            let write_budget = |limit| {
                let (path, contents) = if name == "included_cycle" {
                    std::fs::write(root.join("input.s"), source).expect("root source");
                    (
                        "sub/cycles.inc",
                        facts["files"]["sub/cycles.inc"].as_str().expect("include"),
                    )
                } else {
                    ("input.s", source)
                };
                std::fs::write(
                    root.join(path),
                    format!("{contents}; asm198x: cycles({symbol}) <= {limit}\n"),
                )
                .expect("budget source");
            };
            write_budget(limit);
            let pass = execute();
            assert!(pass.status.success(), "{name}/{symbol}: {pass:?}");
            let listing = std::fs::read_to_string(root.join("out.lst")).expect("text listing");
            assert!(
                listing
                    .lines()
                    .any(|line| line.starts_with(&format!("  {symbol} (+$"))
                        && line.ends_with(&format!(
                            ": {} bytes, {limit} cycles (straight-line)",
                            cost["bytes"]
                        ))),
                "{name}: {listing}"
            );
            let previous: Vec<_> = outputs
                .iter()
                .map(|path| std::fs::read(root.join(path)).expect("output"))
                .collect();
            write_budget(limit - 1);
            let fail = execute();
            assert_eq!(fail.status.code(), Some(1), "{name}/{symbol}: {fail:?}");
            assert!(
                String::from_utf8_lossy(&fail.stderr).contains("exceeds its cycle budget"),
                "{fail:?}"
            );
            if name == "included_cycle" {
                assert!(
                    String::from_utf8_lossy(&fail.stderr).contains("sub/cycles.inc:6:"),
                    "{fail:?}"
                );
                assert_eq!(
                    std::fs::read_to_string(root.join("input.s")).expect("root source"),
                    source
                );
            }
            for (path, before) in outputs.iter().zip(previous) {
                assert_eq!(
                    std::fs::read(root.join(path)).expect("preserved output"),
                    before,
                    "{name}: {path}"
                );
            }
        }
    }
    assert_eq!(checked, 9);
    // Unmapped reservations have no measured instruction cost.
    let fact = facts["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["name"] == "no_file")
        .expect("unmapped case");
    std::fs::write(
        root.join("input.s"),
        format!(
            "{}; asm198x: cycles(buffer) <= 0\n",
            fact["source"].as_str().expect("source")
        ),
    )
    .expect("unknown budget");
    let fail = run(
        &root,
        &[
            "--dialect",
            "ca65",
            "-C",
            "input.cfg",
            "input.s",
            "-o",
            "out.bin",
        ],
    );
    assert_eq!(fail.status.code(), Some(1), "{fail:?}");
}
#[test]
fn logical_debug_failure_preserves_outputs_and_include_location() {
    let root = scratch::dir("ca65-absolute-debug-preserve");
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ca65-absolute-debug.json"))
            .expect("native captures");
    let config = facts["cases"][0]["config"].as_str().expect("config");
    std::fs::write(root.join("input.cfg"), config).expect("config");
    let source = ".include \"definitions.inc\"\n";
    let invalid = ".org $2000\n.word 65536\n";
    std::fs::write(root.join("input.s"), source).expect("source");
    let outputs = [
        "out.bin",
        "out.sym",
        "out.debug198x",
        "out.lst",
        "out.lst.json",
    ];
    for mode in ["human", "json"] {
        let execute = || {
            run(
                &root,
                &[
                    "--dialect",
                    "ca65",
                    "-C",
                    "input.cfg",
                    "--message-format",
                    mode,
                    "input.s",
                    "-o",
                    "out.bin",
                    "--sym=out.sym",
                    "--debug=out.debug198x",
                    "--listing=out.lst",
                    "--listing-json=out.lst.json",
                ],
            )
        };
        std::fs::write(
            root.join("definitions.inc"),
            ".org $2000\n.segment \"RODATA\"\n.byte 1\n.segment \"CODE\"\n.word *\n",
        )
        .expect("valid include");
        let success = execute();
        assert!(success.status.success(), "{success:?}");
        assert_eq!(
            std::fs::read(root.join("out.bin")).expect("image"),
            [1, 0x20, 1]
        );
        let previous: Vec<_> = outputs
            .iter()
            .map(|name| std::fs::read(root.join(name)).expect("output"))
            .collect();
        std::fs::write(root.join("definitions.inc"), invalid).expect("invalid include");
        let failure = execute();
        assert_eq!(failure.status.code(), Some(1), "{failure:?}");
        if mode == "json" {
            let errors: serde_json::Value =
                serde_json::from_slice(&failure.stdout).expect("diagnostics");
            assert_eq!(errors[0]["span"]["line"], 2);
            assert!(
                errors[0]["span"]["path"]
                    .as_str()
                    .expect("path")
                    .ends_with("definitions.inc")
            );
        } else {
            assert!(
                String::from_utf8_lossy(&failure.stderr).contains("definitions.inc:2:"),
                "{failure:?}"
            );
        }
        for (name, bytes) in outputs.iter().zip(previous) {
            assert_eq!(
                std::fs::read(root.join(name)).expect("preserved output"),
                bytes,
                "{name}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(root.join("input.s")).expect("source"),
            source
        );
        assert_eq!(
            std::fs::read_to_string(root.join("definitions.inc")).expect("include"),
            invalid
        );
    }
}
