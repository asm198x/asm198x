# Native empty ca65 function calls (#927)

Start from checked #925 main ee522abd40a0bfa7a61b5581679679690717ccff.
The approved canonical reader work is retained on fix/ca65-canonical-reader;
its preparation refactors are not part of this independent bug fix.

1. Extend the retained 85-case native audit to configured ca65, 65816 and
   HuC6280, plain/whitespace empty calls, all five source contexts and valid
   nonempty counterparts and nested numeric/constructor token-list controls. probe.py records 840 native programs and both streams.
   compare.py uses each explicit dialect, requires output preservation and
   records exact exit codes so a panic is not treated as ordinary refusal.
2. In ca65_flat.rs Ca65Text::function, keep line-zero recognition but enforce
   native selected-source rules. Blank/tcount return native numeric results;
   invalid empty calls return AsmError. Make select_ca65_live reject an unexpected
   fold result rather than panic. Preserve nonempty rules and lazy reading.
3. Retain a complete native fixture and mandatory package cohort in existing
   ca65_deferred_aliases.rs/test-candidate-package.cjs patterns. Verify all 840
   bytes/status outcomes, exact refusal code 1, JSON, original diagnostics and
   macro frames, official debug-reader output, formatting and output retention.
   The shared fixture helper may read an explicit dialect; preserve old defaults.
4. Show checked #925 fails the new cohort. Run focused/full Rust/Lua, all-target
   Clippy, Node, fmt/docs/version/ecosystem/ledger checks, release, fresh4510 and
   artifact controls. Preserve all checked predecessor matches in the retained
   baseline. Qualify all five native archives and exact-source checks.
5. Push verified commits, rebase-merge the checked PR, retain receipts and clean
   task artifacts/processes. Reuse the sole linked worktree and existing caches,
   then resume the approved canonical reader migration. No dependency, public
   contract, layout, scope model or binding decision changes are needed.

The expanded capture exposes existing #895 for 112 flat-family inactive
programs. Keep those in the complete audit inventory with explicit #895
dispositions; qualify the other 728 cases. No existing test is disabled.
Nested controls must prevent a real-call guard from making numeric functions
fold inside captured lists: physical numeric calls remain tokens there, while
constructors continue folding. Verify the shared brace-fold policy against
native on all three paths before adopting it.
