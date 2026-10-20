# Verified ca65 string consumers

The raw argument foundation preserves cooked access for RGBASM and every
existing shared text consumer. ca65 will opt into raw bytes explicitly.

| Consumer | Existing integration | Required value contract |
| --- | --- | --- |
| `.concat`, `.string`, `.sprintf`, `.ident` | `ca65_flat.rs::Ca65Text::function` | Byte tokens for construction; UTF-8/ASCII identifier validation for `.ident` |
| Numeric format conversions and selector bounds | `ca65_flat.rs::format_one`, `selector_bound` | Refuse both authored and computed strings |
| Token-list predicates/selectors | `ca65_flat.rs::token_list`, `tokens`, `token_slice` | Preserve selected token identity; quoted admission follows byte qualification |
| Linked `.byte`/`.asciiz` | `ca65/mod.rs::parse_data_list`, `parse_asciiz` | Materialize byte strings into numeric byte operations |
| Flat `.byte`/`.asciiz` | `ca65_816.rs` and `ca65_huc6280.rs` directive/byte-list readers | Same byte values, retained through each file expansion |
| `.word`, `.dword`, instruction operands | Existing numeric expression readers | Continue refusing strings |
| `.setcpu` | `ca65/mod.rs::Walker::select_cpu` and flat source-only directives | Valid text name; no binary coercion |
| `.segment` | `ca65/mod.rs::segment_switch`, directive parser | Text name used by projection and link configuration |
| `.include`/`.incbin` | `ca65_flat.rs` walk directive parsing and linked `project.rs` resolution | Text file request, existing source/include order |
| `.assert`, messages | `ca65/mod.rs::parse_assert` and directive parser | Text diagnostics; never expose a private handle |
| `.strlen`, `.strat` | `ca65_flat.rs::expr_function` | Existing numeric expression-function boundary; inventory nested computed input before migration |
| Formatter | Syntax-only readers and canonical AST output | Authored spelling; no expansion or whole-file fallback |

The linked structural walkers must continue to pass source tokens unread to the
selected reader. Their short lifetime must not allocate or resolve byte values.
The selected reader owns its assembly store. Flat walkers must retain their
private expansion store while their shared walk parses each expanded file.
