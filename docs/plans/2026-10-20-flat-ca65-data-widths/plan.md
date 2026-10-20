# Audit flat ca65 data-width limits after #906

1. Measure native acceptance and emitted bytes around signed and unsigned
   boundaries for `.byte`, `.word`, `.addr`, `.dbyt`, `.dword` and `.faraddr`,
   in direct and invoked macro source for both flat CPUs. Keep extraction and
   unused-macro controls. Retain exact tools, sources and process streams.
2. Replay the frozen #904/#906 producer; report only measured differences.
   Check existing issues before filing focused symptoms with minimal repros.
3. Explain each root cause and use existing approved data-range machinery when
   it fits. Obtain agreement before any further shared architectural change.
4. Verify native, permanent CLI, formatter/debug, archive and CI evidence;
   preserve existing consumers and clean task-owned work before completion.
