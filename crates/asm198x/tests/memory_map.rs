//! The memory map and the space budget (#499): used/free per area of the
//! active layout, and `; asm198x: free(<area>) >= N` failing the build when
//! a program outgrows its headroom. Layer 1 of the issue's three: figures
//! come from the layout, not from any claim about a booted machine.

use asm198x::source::MemoryLoader;
use asm198x::{assemble_acme, assemble_ca65_files_with_config, render_map};

const CFG: &str = r#"
MEMORY {
    HDR: start = $0000, size = $0004, type = ro, file = %O, fill = yes, fillval = $00;
    PRG: start = $8000, size = $0100, type = ro, file = %O, fill = yes, fillval = $00;
}
SEGMENTS {
    HEADER:  load = HDR, type = ro;
    CODE:    load = PRG, type = ro, start = $8000;
    VECTORS: load = PRG, type = ro, start = $80FA;
}
"#;

/// Areas account capacity, use, free, and — the number a new routine needs —
/// the largest contiguous hole, which a pinned segment can make smaller than
/// the free total suggests.
#[test]
fn areas_account_use_free_and_the_largest_hole() {
    let src = ".segment \"HEADER\"\n.byte 1,2,3,4\n.segment \"CODE\"\n.res 16\n.segment \"VECTORS\"\n.word $8000\n";
    let r = assemble_ca65_files_with_config(src, "main.s", &MemoryLoader::new(), CFG)
        .expect("assembles");
    let prg = r.areas.iter().find(|a| a.name == "PRG").expect("PRG row");
    assert_eq!((prg.size, prg.used, prg.free), (0x100, 18, 0x100 - 18));
    // CODE ends at $8010; VECTORS pins at $80FA and runs to $80FC. The free
    // space is one hole ($8010..$80FA) plus the tail ($80FC..$8100).
    assert_eq!(prg.largest_free, 0xFA - 0x10);
    assert_eq!(prg.segments.len(), 2);
}

/// A flat dialect reports the one implicit area: the 64K space, the program's
/// span, and the larger of the runs below and above it.
#[test]
fn flat_dialects_report_the_single_space() {
    let r = assemble_acme("        * = $c000\n        !fill 16, 0\n").expect("assembles");
    assert_eq!(r.areas.len(), 1);
    let a = &r.areas[0];
    assert_eq!((a.start, a.size, a.used), (0, 0x1_0000, 16));
    assert_eq!(a.free, 0x1_0000 - 16);
    assert_eq!(a.largest_free, 0xC000, "below the origin is the bigger run");
}

/// R-budget: `free(<area>) >= N` holds the headroom — within passes, beyond
/// fails naming the area, the requirement, and the actuals. `$hex` accepted.
#[test]
fn space_budget_passes_within_and_fails_beyond() {
    let src = "; asm198x: free(PRG) >= $E0\n.segment \"CODE\"\n.res 16\n";
    assemble_ca65_files_with_config(src, "main.s", &MemoryLoader::new(), CFG)
        .expect("224 free covers $E0");

    let src = "; asm198x: free(PRG) >= $F1\n.segment \"CODE\"\n.res 16\n";
    let e = assemble_ca65_files_with_config(src, "main.s", &MemoryLoader::new(), CFG)
        .expect_err("only 240 free against 241");
    for needle in ["PRG", "240", "space budget"] {
        assert!(e.error.message.contains(needle), "{}", e.error.message);
    }
    assert_eq!(e.error.line, 1);
}

/// A budget naming an unknown area lists the areas that exist.
#[test]
fn space_budget_on_an_unknown_area_names_the_real_ones() {
    let src = "; asm198x: free(ROM) >= 1\n.segment \"CODE\"\n.res 1\n";
    let e = assemble_ca65_files_with_config(src, "main.s", &MemoryLoader::new(), CFG)
        .expect_err("no ROM area");
    assert!(e.error.message.contains("PRG"), "{}", e.error.message);
}

/// The human map renders every area with its segments; pinned here so the
/// shape is deliberate.
#[test]
fn the_map_renders_areas_and_segments() {
    let src = ".segment \"CODE\"\n.res 16\n.segment \"VECTORS\"\n.word $8000\n";
    let r = assemble_ca65_files_with_config(src, "main.s", &MemoryLoader::new(), CFG)
        .expect("assembles");
    let map = render_map(&r);
    assert!(map.contains("memory map"), "{map}");
    assert!(map.contains("PRG"), "{map}");
    assert!(map.contains("VECTORS  $80FA"), "{map}");
}
