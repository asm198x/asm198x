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
