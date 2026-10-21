ca65 accepts physical register tokens as `.repeat` counter names, although native ca65 refuses them.

```asm
.repeat 1,A
.byte 7
.endrepeat
```

Native ca65 V2.18 refuses line 1 with `Identifier expected`. Asm198x accepts and emits `07`. The same false acceptance occurs for physical `X` and `Y`.

The valid generated form must stay accepted:

```asm
.repeat 1,.ident("A")
.byte 7
.endrepeat
```

Native accepts and emits `07`. An expanded-spelling check rejecting every `A`/`X`/`Y` would reject valid source.

An 18-case native audit covers plain/generated `A`, `X`, `Y`, `Z`, `S`, and `INDEX`, plus inactive controls for all six names. Native accepts 15 and refuses only the three physical register names. Both the checked #920 producer (`14295cef80345ac2e38ac16eb2ff20b162595956`, executable SHA-256 `b95498886cb76841ff4831c9018eaefb1efc859b3e86bf3fb70f7722bc9f9f43`) and the corrected #922 source (`b69d510f334847a3c1ad54d6798bc335efc755c5`, executable SHA-256 `76066fe8048d203a7c567b43953d0fce193db8c756254617c1f71294575c4449`) falsely accept those same three cases. No prior native match is lost; this is a pre-existing gap, separate from #795's ordered-expression fix.

The current `project::fold_repeat` validates the expanded counter spelling with `is_ident`, which cannot distinguish the physical register token from `.ident`'s identifier result. This is the repeat-header counterpart of #918's visibility-operand provenance gap.

Preserve per-field native token admission at the existing text/reader seam. Add direct/include/macro and lazy controls, mixed generated/physical headers, target-dependent register cases, counter substitution, formatter/debug round trips, source/macro diagnostics, and unchanged outputs on refusal. Keep generated-register names valid rather than adding a whole-line “contains .ident” exception.
