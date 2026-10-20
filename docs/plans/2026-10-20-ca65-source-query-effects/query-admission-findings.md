# Next #795 source-query admission controls

The 54 native controls accept 35 programs and refuse 19. The corrected source
classifier producer 499a58a29015f79a0b3897572db1e1ba9dc816c8 still accepts
15 programs native ca65 rejects. They cover missing names in .ifconst,
cancellation, zero products, logical .const arguments and explicit root lookup,
each directly, in includes and in macros. Each comparison seeds an existing output and records whether refusal preserves
it, alongside current CLI acceptance and actual bytes.

Query-only .ref/.defined arguments, forward definitions, scoped outward fallback,
shadowing, logical source reads and lazy bodies are controls. They must keep
working when final required-name admission is added. The ordinary read inside
0&&VALUE remains observable through the next .ref even though evaluation can
short-circuit. A missing name read through 0&&MISSING still requires admission.

Reader.record_reference normalizes a scoped reference into a qualified key for
its reference set. That set alone cannot reconstruct explicit qualification
versus deferred outward fallback. Final admission must preserve the existing
bound lookup key and source span at the context seam, then use the existing
scope/Deferred resolution path. It must not create a second namespace or keep
numeric effect sentinels/output statements alive after a predicate folds.

This remains part of the approved source-aware #795 design. It is not a claim
that the classifier PR implements query admission, live .const ordering or
numeric string argument effects. The next parser/context slice must address
these alongside the retained 128-case ordering matrix and text controls.
