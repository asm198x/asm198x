# Share the published SNA encoder

Base: 9a9291a869f9be923f73b33196a401484d4a2bd8. The user approved the
format198x-sinclair-zx-spectrum-sna 0.1 dependency on 2026-10-28.

1. Record existing sna_48k validation tests and complete-file comparisons
   against pasmo --sna, including RAM/attribute/stack boundaries.
2. Add the approved dependency in Cargo.toml and crates/asm198x/Cargo.toml.
   Replace only header serialization and PC pushing in src/sna.rs with
   Sna::new_48k and encode. Keep Asm validation order/messages, RAM placement,
   screen attributes, SP default, entry and public signature. Use file-shaped
   codec types; no emulator Snapshot model moves. Update root/web lockfiles.
3. Repeat the unchanged unit controls and full-file Pasmo comparisons. Run
   workspace tests with the pinned curriculum, strict Clippy, docs/formatting,
   shipped dependency and excluded-web lock checks. Preserve before/after
   evidence and clean the temporary curriculum clone after retention checks.
4. Push checked commits through a PR and require all current CI and five
   native candidate-package qualifications before rebase merge. Verify
   asm198x/asm198x#941 and emu198x/emu198x#1995 closure, then clean workspaces,
   fixtures, tool processes and caches.

## Differential evidence

Four complete 49,179-byte files before and after the switch have identical
SHA-256 values. `normal`, `ram_start` and `stack_overlap` are also byte-identical
to Pasmo. The `attributes` case exposes an existing policy difference: Pasmo
fills attributes after copying code, replacing $5800/$5801 with $38; Asm198x
keeps the assembled $47/$01 bytes. The migration preserves that policy. The
portable comparison script checks that these are the only differing offsets;
the before/after receipts are committed alongside it.
