//! A source map for a dialect that reads source as bytes keeps every file's
//! bytes, and shows each file as UTF-8 when it is, else as Latin-1.

use asm198x_ir::source::{MemoryLoader, SourceLoader, SourceMap, shown};
use asm198x_ir::span::FileId;

#[test]
fn bytes_are_shown_as_utf8_when_they_are() {
    assert_eq!(shown("café".as_bytes()), "café");
    assert_eq!(shown(b"caf\xe9"), "café");
    assert_eq!(shown(b"\xff\x80"), "\u{ff}\u{80}");
}

#[test]
fn a_map_read_from_bytes_keeps_them() {
    let loader = MemoryLoader::new()
        .source_bytes("latin1.inc", b"; \xe9\n".to_vec())
        .text("utf8.inc", "; é\n");
    let mut map = SourceMap::from_bytes("root.s", b".byte \"\xe9\"\n".to_vec());
    assert_eq!(map.bytes(FileId(0)), Some(&b".byte \"\xe9\"\n"[..]));
    assert_eq!(map.contents(FileId(0)), Some(".byte \"é\"\n"));
    let latin1 = map
        .load(&loader, "latin1.inc", FileId(0), 1)
        .expect("loads");
    assert_eq!(map.bytes(latin1), Some(&b"; \xe9\n"[..]));
    assert_eq!(map.contents(latin1), Some("; é\n"));
    // A file the loader has only as text gives its UTF-8 bytes.
    let utf8 = map.load(&loader, "utf8.inc", FileId(0), 2).expect("loads");
    assert_eq!(map.bytes(utf8), Some("; é\n".as_bytes()));
}

#[test]
fn a_map_of_text_reads_text() {
    let loader = MemoryLoader::new()
        .source_bytes("latin1.inc", b"; \xe9\n".to_vec())
        .source_bytes("ascii.inc", b"; e\n".to_vec());
    let mut map = SourceMap::new("root.s", "");
    assert_eq!(map.bytes(FileId(0)), None);
    let ascii = map
        .load(&loader, "ascii.inc", FileId(0), 1)
        .expect("UTF-8 loads");
    assert_eq!(map.contents(ascii), Some("; e\n"));
    assert_eq!(map.bytes(ascii), None);
    let refused = map
        .load(&loader, "latin1.inc", FileId(0), 2)
        .expect_err("not UTF-8");
    assert!(refused.to_string().contains("valid UTF-8"), "{refused}");
}

#[test]
fn a_loader_gives_bytes_by_default_from_its_text() {
    struct TextOnly;
    impl SourceLoader for TextOnly {
        fn load_text(
            &self,
            request: &str,
            _: Option<&str>,
        ) -> Result<(String, String), asm198x_ir::source::LoadError> {
            Ok((request.to_owned(), "é".to_owned()))
        }
        fn load_binary(
            &self,
            request: &str,
            from: Option<&str>,
        ) -> Result<Vec<u8>, asm198x_ir::source::LoadError> {
            Err(asm198x_ir::source::LoadError {
                request: request.to_owned(),
                from: from.map(str::to_owned),
                message: "none".to_owned(),
            })
        }
    }
    let (path, bytes) = TextOnly.load_bytes_at("f", None, 1).expect("loads");
    assert_eq!((path.as_str(), bytes.as_slice()), ("f", "é".as_bytes()));
}

#[test]
fn the_file_system_loader_reads_bytes() {
    let dir = std::env::temp_dir().join(format!("asm198x-ir-bytes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("latin1.inc"), b"\xe9").expect("file");
    let loader = asm198x_ir::source::FsLoader::new(&dir, Vec::new());
    let (_, bytes) = loader.load_bytes_at("latin1.inc", None, 1).expect("reads");
    assert_eq!(bytes, b"\xe9");
    assert!(loader.load_bytes_at("missing.inc", None, 1).is_err());
    std::fs::remove_dir_all(&dir).ok();
}
