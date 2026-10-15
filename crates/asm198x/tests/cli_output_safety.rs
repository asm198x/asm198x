//! Output failures must not replace a previous image or create a misleading one.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "asm198x-output-safety-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("isolated output directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assemble(dir: &Path, dialect: &str, mode: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(dir)
        .args(["--dialect", dialect, "--message-format", mode, "program.s"])
        .args(extra)
        .output()
        .expect("run assembler")
}

fn assert_preserved(path: &Path, previous: Option<&[u8]>) {
    match previous {
        Some(bytes) => assert_eq!(std::fs::read(path).expect("previous image remains"), bytes),
        None => assert!(
            !path.exists(),
            "failed invocation created {}",
            path.display()
        ),
    }
}

#[test]
fn invalid_sidecar_destinations_preserve_primary_images_in_every_driver() {
    for (dialect, source, flags) in [
        ("acme", "*=$1000\nstart: lda #$2a\n", &[][..]),
        ("acme", "*=$1000\nstart: lda #$2a\n", &["--prg"][..]),
        (
            "pasmo",
            " org $8000\nstart: nop\n end start\n",
            &["--tapbas"][..],
        ),
        ("ca65", ".segment \"CODE\"\nstart: nop\n", &[][..]),
        ("vasm", "start: moveq #42,d0\n rts\n", &[][..]),
    ] {
        for mode in ["human", "json"] {
            for option in ["--sym", "--debug", "--listing", "--listing-json", "--map"] {
                // vasm listings are explicitly unsupported.
                if dialect == "vasm" && option.starts_with("--listing") {
                    continue;
                }
                let dir = Scratch::new();
                std::fs::write(dir.0.join("program.s"), source).expect("source");
                let mut valid = vec!["-o", "image.bin"];
                valid.extend_from_slice(flags);
                let control = assemble(&dir.0, dialect, mode, &valid);
                assert!(control.status.success(), "{dialect}/{mode}: {control:?}");
                assert!(
                    !std::fs::read(dir.0.join("image.bin"))
                        .expect("control image")
                        .is_empty()
                );
                for previous in [None, Some(b"previous successful image".as_slice())] {
                    let image = dir.0.join("image.bin");
                    if image.exists() {
                        std::fs::remove_file(&image).expect("reset control image");
                    }
                    if let Some(bytes) = previous {
                        std::fs::write(&image, bytes).expect("previous image");
                    }
                    let bad = format!("{option}=missing/output");
                    let mut args = valid.clone();
                    args.push(&bad);
                    let failed = assemble(&dir.0, dialect, mode, &args);
                    assert!(!failed.status.success(), "{dialect}/{mode}/{option}");
                    assert_preserved(&image, previous);
                }
            }
        }
    }
}

#[test]
fn source_named_symbols_and_save_destinations_are_checked_before_the_image() {
    for (dialect, source) in [
        (
            "acme",
            "!to \"named.bin\",plain\n!symbollist \"missing/named.sym\"\n*=$1000\nstart: lda #$2a\n",
        ),
        (
            "sjasmplus",
            " DEVICE ZXSPECTRUM48\n ORG $8000\n db 42\n SAVEBIN \"missing/saved.bin\",$8000,1\n",
        ),
    ] {
        for mode in ["human", "json"] {
            for previous in [None, Some(b"previous successful image".as_slice())] {
                let dir = Scratch::new();
                std::fs::write(dir.0.join("program.s"), source).expect("source");
                let image = dir.0.join("named.bin");
                if let Some(bytes) = previous {
                    std::fs::write(&image, bytes).expect("previous image");
                }
                let args = if dialect == "acme" {
                    &[][..]
                } else {
                    &["-o", "named.bin"][..]
                };
                let failed = assemble(&dir.0, dialect, mode, args);
                assert!(!failed.status.success(), "{dialect}/{mode}: {failed:?}");
                assert_preserved(&image, previous);
                std::fs::create_dir(dir.0.join("missing")).expect("repair destination");
                let successful = assemble(&dir.0, dialect, mode, args);
                assert!(successful.status.success(), "{successful:?}");
                assert_ne!(
                    std::fs::read(&image).expect("new image").as_slice(),
                    b"previous successful image"
                );
                assert!(
                    dir.0
                        .join(if dialect == "acme" {
                            "missing/named.sym"
                        } else {
                            "missing/saved.bin"
                        })
                        .is_file()
                );
            }
        }
    }
}

#[test]
fn directory_and_readonly_sidecars_do_not_truncate_an_existing_image() {
    for mode in ["human", "json"] {
        for readonly in [false, true] {
            let dir = Scratch::new();
            std::fs::write(dir.0.join("program.s"), "*=$1000\nstart: lda #$2a\n").expect("source");
            let sidecar = dir.0.join("sidecar");
            if readonly {
                std::fs::write(&sidecar, "previous symbols").expect("symbol file");
                let mut permissions = sidecar.metadata().expect("metadata").permissions();
                permissions.set_readonly(true);
                std::fs::set_permissions(&sidecar, permissions).expect("readonly symbol file");
            } else {
                std::fs::create_dir(&sidecar).expect("directory at symbol path");
            }
            let image = dir.0.join("image.bin");
            std::fs::write(&image, b"previous successful image").expect("previous image");
            let failed = assemble(&dir.0, "acme", mode, &["-o", "image.bin", "--sym=sidecar"]);
            assert!(!failed.status.success(), "{failed:?}");
            assert_preserved(&image, Some(b"previous successful image"));
            if readonly {
                // Windows requires clearing readonly before removing the fixture.
                let mut permissions = sidecar.metadata().expect("metadata").permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                std::fs::set_permissions(&sidecar, permissions).expect("restore permissions");
            }
        }
    }
}

#[test]
fn memory_maps_match_between_reporting_modes_and_validate_before_writes() {
    for (dialect, source) in [
        ("acme", "*=$1000\nstart: lda #$2a\n rts\n"),
        ("pasmo", " org $8000\nstart: nop\n"),
        ("ca65", ".segment \"CODE\"\nstart: nop\n"),
        ("vasm", "start: moveq #42,d0\n rts\n"),
    ] {
        for flag in ["--map", "--map=memory.txt"] {
            let mut maps = Vec::new();
            for mode in ["human", "json"] {
                let dir = Scratch::new();
                std::fs::write(dir.0.join("program.s"), source).expect("source");
                let run = assemble(&dir.0, dialect, mode, &["-o", "image.bin", flag]);
                assert!(run.status.success(), "{dialect}/{mode}: {run:?}");
                let path = dir.0.join(if flag == "--map" {
                    "program.map"
                } else {
                    "memory.txt"
                });
                let map = std::fs::read(&path).expect("requested map exists");
                assert!(!map.is_empty());
                if mode == "json" {
                    assert!(run.stderr.is_empty());
                    let result: asm198x::AssemblyResult =
                        serde_json::from_slice(&run.stdout).expect("JSON result");
                    assert_eq!(map, asm198x::render_map(&result).as_bytes());
                    assert!(
                        result
                            .warnings
                            .iter()
                            .any(|warning| warning.message.contains("memory map"))
                    );
                }
                maps.push(map);
            }
            assert_eq!(maps[0], maps[1], "{dialect}/{flag}");
        }
    }
}

#[test]
fn sidecars_preserve_inputs_and_binary_images_through_aliases() {
    for (dialect, source, included) in [
        (
            "acme",
            "*=$1000\n!source \"body.inc\"\n!binary \"data.bin\"\n",
            "start: lda #42\n",
        ),
        (
            "pasmo",
            " org $8000\n include \"body.inc\"\n incbin \"data.bin\"\n",
            "start: nop\n",
        ),
        (
            "ca65",
            ".segment \"CODE\"\n.include \"body.inc\"\n.incbin \"data.bin\"\n",
            "start: nop\n",
        ),
        (
            "vasm",
            " include \"body.inc\"\n incbin \"data.bin\"\n",
            "start: moveq #42,d0\n",
        ),
    ] {
        for mode in ["human", "json"] {
            for option in ["--debug", "--sym", "--listing", "--listing-json", "--map"] {
                if dialect == "vasm" && option.starts_with("--listing") {
                    continue;
                }
                for target in ["program.s", "body.inc", "data.bin", "image.bin"] {
                    for alias in ["parent", "hard-link", "symlink", "windows-case"] {
                        #[cfg(not(unix))]
                        if alias == "symlink" {
                            continue;
                        }
                        #[cfg(not(windows))]
                        if alias == "windows-case" {
                            continue;
                        }
                        let dir = Scratch::new();
                        std::fs::create_dir(dir.0.join("nested")).expect("alias parent");
                        std::fs::write(dir.0.join("program.s"), source).expect("root");
                        std::fs::write(dir.0.join("body.inc"), included).expect("include");
                        std::fs::write(dir.0.join("data.bin"), [0xde, 0xad]).expect("binary input");
                        let valid = format!("{option}=separate.out");
                        let control = assemble(&dir.0, dialect, mode, &["-o", "image.bin", &valid]);
                        assert!(control.status.success(), "{dialect}/{mode}: {control:?}");
                        assert!(
                            !std::fs::read(dir.0.join("image.bin"))
                                .expect("control image")
                                .is_empty()
                        );
                        std::fs::write(dir.0.join("image.bin"), b"previous image")
                            .expect("previous image");
                        let destination = match alias {
                            "parent" => format!("nested/../{target}"),
                            "windows-case" => target.to_ascii_uppercase(),
                            "hard-link" => {
                                std::fs::hard_link(dir.0.join(target), dir.0.join("alias"))
                                    .expect("hard link");
                                "alias".to_owned()
                            }
                            _ => {
                                #[cfg(unix)]
                                std::os::unix::fs::symlink(target, dir.0.join("alias"))
                                    .expect("symlink");
                                "alias".to_owned()
                            }
                        };
                        let bad = format!("{option}={destination}");
                        let run = assemble(&dir.0, dialect, mode, &["-o", "image.bin", &bad]);
                        assert!(
                            !run.status.success(),
                            "{dialect}/{mode}/{option}/{target}/{alias}: {run:?}"
                        );
                        assert_eq!(
                            std::fs::read(dir.0.join("program.s")).expect("root survives"),
                            source.as_bytes()
                        );
                        assert_eq!(
                            std::fs::read(dir.0.join("body.inc")).expect("include survives"),
                            included.as_bytes()
                        );
                        assert_eq!(
                            std::fs::read(dir.0.join("data.bin")).expect("binary survives"),
                            [0xde, 0xad]
                        );
                        assert_preserved(&dir.0.join("image.bin"), Some(b"previous image"));
                        if mode == "json" {
                            assert!(run.stderr.is_empty());
                            let errors: Vec<asm198x::Diagnostic> =
                                serde_json::from_slice(&run.stdout).expect("JSON rejection");
                            assert_eq!(errors.len(), 1);
                            assert!(errors[0].message.contains("refusing"));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn sidecars_preserve_linker_configuration_and_source_named_images() {
    for mode in ["human", "json"] {
        let dir = Scratch::new();
        let config = "MEMORY { PRG: start=$8000, size=$100, file=%O, fill=yes; }\nSEGMENTS { CODE: load=PRG, type=ro; }\n";
        std::fs::write(dir.0.join("program.s"), ".segment \"CODE\"\nstart: nop\n").expect("source");
        std::fs::write(dir.0.join("layout.cfg"), config).expect("configuration");
        std::fs::write(dir.0.join("image.bin"), b"previous image").expect("previous image");
        let control = assemble(
            &dir.0,
            "ca65",
            mode,
            &["-C", "layout.cfg", "-o", "control.bin", "--sym=valid.sym"],
        );
        assert!(control.status.success(), "valid config: {control:?}");
        let run = assemble(
            &dir.0,
            "ca65",
            mode,
            &["-C", "layout.cfg", "-o", "image.bin", "--sym=layout.cfg"],
        );
        assert!(!run.status.success(), "{run:?}");
        assert_eq!(
            std::fs::read_to_string(dir.0.join("layout.cfg")).expect("config survives"),
            config
        );
        assert_preserved(&dir.0.join("image.bin"), Some(b"previous image"));
        std::fs::write(
            dir.0.join("program.s"),
            "!to \"image.bin\",plain\n!symbollist \"alias\"\n*=$1000\nstart: lda #42\n",
        )
        .expect("source request");
        std::fs::hard_link(dir.0.join("image.bin"), dir.0.join("alias")).expect("image alias");
        let run = assemble(&dir.0, "acme", mode, &[]);
        assert!(!run.status.success(), "{run:?}");
        assert_preserved(&dir.0.join("image.bin"), Some(b"previous image"));
    }
}

#[test]
fn sidecars_cannot_replace_absent_images_or_save_artifacts_through_aliases() {
    for mode in ["human", "json"] {
        for saved in [false, true] {
            for existing in [false, true] {
                for symlink in [false, true] {
                    #[cfg(not(unix))]
                    if symlink {
                        continue;
                    }
                    let dir = Scratch::new();
                    std::fs::create_dir(dir.0.join("nested")).expect("alias parent");
                    let source = if saved {
                        " DEVICE ZXSPECTRUM48\n ORG $8000\nstart: db 42\n SAVEBIN \"saved.bin\",$8000,1\n"
                    } else {
                        "*=$1000\nstart: lda #42\n"
                    };
                    std::fs::write(dir.0.join("program.s"), source).expect("source");
                    let target = if saved { "saved.bin" } else { "image.bin" };
                    if existing {
                        std::fs::write(dir.0.join(target), b"previous output")
                            .expect("previous output");
                    }
                    let alias = if symlink {
                        #[cfg(unix)]
                        std::os::unix::fs::symlink(target, dir.0.join("alias"))
                            .expect("possibly dangling symlink");
                        "alias".to_owned()
                    } else {
                        format!("nested/../{target}")
                    };
                    let flag = format!("--sym={alias}");
                    let run = assemble(
                        &dir.0,
                        if saved { "sjasmplus" } else { "acme" },
                        mode,
                        &["-o", "image.bin", &flag],
                    );
                    assert!(
                        !run.status.success(),
                        "{mode}/{saved}/{existing}/{symlink}: {run:?}"
                    );
                    assert_preserved(
                        &dir.0.join(target),
                        existing.then_some(b"previous output".as_slice()),
                    );
                    if saved {
                        assert!(!dir.0.join("image.bin").exists());
                    }
                }
            }
        }
    }
}

#[test]
fn distinct_files_with_identical_bytes_are_valid_output_destinations() {
    for mode in ["human", "json"] {
        let dir = Scratch::new();
        std::fs::write(dir.0.join("program.s"), "*=$1000\nstart: lda #42\n").expect("source");
        for name in ["image.bin", "symbols.sym"] {
            std::fs::write(dir.0.join(name), b"same previous bytes").expect("distinct file");
        }
        let run = assemble(
            &dir.0,
            "acme",
            mode,
            &["-o", "image.bin", "--sym=symbols.sym"],
        );
        assert!(run.status.success(), "{run:?}");
        assert_eq!(
            std::fs::read(dir.0.join("image.bin")).expect("machine code"),
            [0xa9, 42]
        );
        assert!(
            std::fs::read_to_string(dir.0.join("symbols.sym"))
                .expect("symbols")
                .contains("start")
        );
    }
}

#[cfg(unix)]
#[test]
fn write_only_sidecars_do_not_require_read_access_for_identity_checks() {
    use std::os::unix::fs::PermissionsExt;
    for mode in ["human", "json"] {
        let dir = Scratch::new();
        std::fs::write(dir.0.join("program.s"), "*=$1000\nstart: lda #42\n").expect("source");
        let symbols = dir.0.join("symbols.sym");
        std::fs::write(&symbols, b"previous symbols").expect("previous symbols");
        std::fs::set_permissions(&symbols, std::fs::Permissions::from_mode(0o200))
            .expect("write only");
        let run = assemble(
            &dir.0,
            "acme",
            mode,
            &["-o", "image.bin", "--sym=symbols.sym"],
        );
        std::fs::set_permissions(&symbols, std::fs::Permissions::from_mode(0o600))
            .expect("restore read access");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(
            std::fs::read(dir.0.join("image.bin")).expect("machine code"),
            [0xa9, 42]
        );
        assert!(
            std::fs::read_to_string(symbols)
                .expect("new symbols")
                .contains("start")
        );
    }
}

#[test]
fn conflicting_sidecars_are_rejected_before_any_output_changes() {
    let options = [
        ("--debug", "debug sidecar"),
        ("--sym", "symbol table"),
        ("--listing", "listing"),
        ("--listing-json", "JSON listing"),
        ("--map", "memory map"),
    ];
    for mode in ["human", "json"] {
        for first in 0..options.len() {
            for second in (first + 1)..options.len() {
                let control = Scratch::new();
                let source = "*=$1000\nstart: lda #42\n";
                std::fs::write(control.0.join("program.s"), source).expect("control source");
                let a = format!("{}=first.out", options[first].0);
                let b = format!("{}=second.out", options[second].0);
                let valid = assemble(&control.0, "acme", mode, &["-o", "image.bin", &a, &b]);
                assert!(valid.status.success(), "{mode}/{a}/{b}: {valid:?}");
                for name in ["first.out", "second.out"] {
                    assert!(
                        !std::fs::read(control.0.join(name))
                            .expect("both distinct sidecars exist")
                            .is_empty()
                    );
                }
                assert_eq!(
                    std::fs::read(control.0.join("image.bin")).expect("control machine code"),
                    [0xa9, 42]
                );
                for existing in [false, true] {
                    for alias in ["exact", "parent", "hard-link", "symlink", "windows-case"] {
                        if alias == "hard-link" && !existing {
                            continue;
                        }
                        #[cfg(not(unix))]
                        if alias == "symlink" {
                            continue;
                        }
                        #[cfg(not(windows))]
                        if alias == "windows-case" {
                            continue;
                        }
                        let dir = Scratch::new();
                        std::fs::create_dir(dir.0.join("nested")).expect("alias parent");
                        std::fs::write(dir.0.join("program.s"), source).expect("source");
                        std::fs::write(dir.0.join("image.bin"), b"previous image")
                            .expect("previous image");
                        if existing {
                            std::fs::write(dir.0.join("shared.out"), b"previous sidecar")
                                .expect("previous sidecar");
                        }
                        let target = match alias {
                            "exact" => "shared.out",
                            "parent" => "nested/../shared.out",
                            "hard-link" => {
                                std::fs::hard_link(dir.0.join("shared.out"), dir.0.join("alias"))
                                    .expect("hard link");
                                "alias"
                            }
                            "symlink" => {
                                #[cfg(unix)]
                                std::os::unix::fs::symlink("shared.out", dir.0.join("alias"))
                                    .expect("possibly dangling symlink");
                                "alias"
                            }
                            _ => "SHARED.OUT",
                        };
                        let a = format!("{}=shared.out", options[first].0);
                        let b = format!("{}={target}", options[second].0);
                        let run = assemble(&dir.0, "acme", mode, &["-o", "image.bin", &a, &b]);
                        assert!(!run.status.success(), "{mode}/{a}/{b}/{existing}: {run:?}");
                        assert_preserved(&dir.0.join("image.bin"), Some(b"previous image"));
                        assert_preserved(
                            &dir.0.join("shared.out"),
                            existing.then_some(b"previous sidecar".as_slice()),
                        );
                        let message = if mode == "json" {
                            assert!(run.stderr.is_empty());
                            let errors: Vec<asm198x::Diagnostic> =
                                serde_json::from_slice(&run.stdout).expect("JSON rejection");
                            assert_eq!(errors.len(), 1);
                            errors[0].message.clone()
                        } else {
                            String::from_utf8_lossy(&run.stderr).into_owned()
                        };
                        for expected in [options[first].1, options[second].1, "shared.out"] {
                            assert!(message.contains(expected), "{message}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn inferred_sidecar_names_participate_in_collision_checks() {
    for mode in ["human", "json"] {
        for source_named in [false, true] {
            let dir = Scratch::new();
            let source = if source_named {
                "!symbollist \"program.sym\"\n*=$1000\nstart: lda #42\n"
            } else {
                "*=$1000\nstart: lda #42\n"
            };
            std::fs::write(dir.0.join("program.s"), source).expect("source");
            std::fs::write(dir.0.join("program.sym"), b"previous symbols")
                .expect("previous symbols");
            let args = if source_named {
                vec!["-o", "image.bin", "--listing=program.sym"]
            } else {
                vec!["-o", "image.bin", "--sym", "--listing=program.sym"]
            };
            let run = assemble(&dir.0, "acme", mode, &args);
            assert!(!run.status.success(), "{run:?}");
            assert!(!dir.0.join("image.bin").exists());
            assert_preserved(&dir.0.join("program.sym"), Some(b"previous symbols"));
        }
    }
}
