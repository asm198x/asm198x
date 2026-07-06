# SLD long-address projection — the banked fixture's second validation leg

Plan R10 requires a desk exercise mapping every banked record of the
hand-authored Spectrum 128 fixture onto sjasmplus **SLD long addresses**, with
no exporter code — proving the record model loses nothing an SLD consumer
(DeZog and kin) needs.

sjasmplus's long address for a banked location is `page * 0x4000 +
offset_within_page`: the page (RAM bank) number scaled by the 16K page size,
plus the offset inside the page. Our record's `(section -> page via space,
offset)` projects onto it directly; the CPU address is `slot_base + offset`
with slot 3 based at `$C000`.

| record | section | space (slot, page) | offset | CPU addr | SLD long address |
|--------|---------|--------------------|--------|----------|------------------|
| symbol `draw` | 0 (`bank1`) | (3, 1) | $0010 | $C010 | 1 * $4000 + $0010 = **$4010** |
| symbol `music` | 1 (`bank3`) | (3, 3) | $0010 | $C010 | 3 * $4000 + $0010 = **$C010** |
| line 5 span | 0 (`bank1`) | (3, 1) | $0010 | $C010 | **$4010** (length 2) |
| line 12 span | 1 (`bank3`) | (3, 3) | $0010 | $C010 | **$C010** (length 2) |

Every field an SLD line needs is recoverable: the page from `space.page`, the
in-page offset from `offset` (pages are section-aligned in this fixture), and
the CPU address from the paged-in slot base. Nothing in the record has to be
guessed — the projection is a pure function of the fixture's data. (That
bank 3's long address coincides with its CPU address is arithmetic, not
meaning: `3 * $4000 = $C000`.)

The third validation leg — cross-checking the slot/page expectations against
Emu198x's actual Spectrum 128 paging model — is cross-repo and lives on the
format's freeze checklist (plan U6/DoD), not in this test tree.
