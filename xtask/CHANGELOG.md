# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.33](https://github.com/asm198x/asm198x/compare/xtask-v0.0.32...xtask-v0.0.33) - 2026-08-25

### Added

- *(xtask)* show what coverage is, and what a pull request did to it ([#267](https://github.com/asm198x/asm198x/pull/267))
- *(docs)* generate the conformance ledger as a published page ([#266](https://github.com/asm198x/asm198x/pull/266))
- *(xtask)* a CPU that arbitrates nothing cannot merge ([#263](https://github.com/asm198x/asm198x/pull/263))
- *(release)* refuse to tag while arbitration debt is owed ([#262](https://github.com/asm198x/asm198x/pull/262))
- *(xtask)* a shortfall declares why it exists, and the check holds it there ([#261](https://github.com/asm198x/asm198x/pull/261))
- *(xtask)* coverage counts rows, so the six unscored CPUs get a number ([#242](https://github.com/asm198x/asm198x/pull/242))
- *(isa)* rows for the three word CPUs, and a class names itself ([#240](https://github.com/asm198x/asm198x/pull/240))
- declare what the last four references have and we do not ([#224](https://github.com/asm198x/asm198x/pull/224))
- *(ca65)* declare the ninety-seven directives ca65 has and we do not ([#222](https://github.com/asm198x/asm198x/pull/222))
- *(sjasmplus)* take the optional leading dot on every directive ([#221](https://github.com/asm198x/asm198x/pull/221))
- *(xtask)* measure how much of each reference's vocabulary we take ([#212](https://github.com/asm198x/asm198x/pull/212))
- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(xtask)* the curriculum receipt compares contents, not paths ([#265](https://github.com/asm198x/asm198x/pull/265))
- *(xtask)* a lagging coverage stamp is drift too, and now fails the check ([#260](https://github.com/asm198x/asm198x/pull/260))
- *(xtask)* stop counting a wider target as a gap ([#219](https://github.com/asm198x/asm198x/pull/219))
- *(cp1610)* speak strict asl, and give the corpus a way to retire a listing ([#218](https://github.com/asm198x/asm198x/pull/218))
- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- release v0.0.32 ([#234](https://github.com/asm198x/asm198x/pull/234))
- release v0.0.31
- ca65's visibility words need all three answers, and rgbasm's needs none
- Refusing a word the reference refuses is not a gap: a category for it
- Give each surface run its own scratch directory
- release v0.0.30 ([#217](https://github.com/asm198x/asm198x/pull/217))
- release v0.0.29 ([#208](https://github.com/asm198x/asm198x/pull/208))
- release v0.0.28 ([#204](https://github.com/asm198x/asm198x/pull/204))
- release v0.0.27 ([#197](https://github.com/asm198x/asm198x/pull/197))
- release v0.0.26 ([#189](https://github.com/asm198x/asm198x/pull/189))
- release v0.0.25 ([#182](https://github.com/asm198x/asm198x/pull/182))
- Say which failure it is: pasmo's unimplemented include, and one wording for an unknown word ([#181](https://github.com/asm198x/asm198x/pull/181))
- Generate a page per dialect, and plan the gaps it exposes ([#180](https://github.com/asm198x/asm198x/pull/180))
- release v0.0.24 ([#173](https://github.com/asm198x/asm198x/pull/173))
- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.32](https://github.com/asm198x/asm198x/compare/xtask-v0.0.31...xtask-v0.0.32) - 2026-08-24

### Added

- declare what the last four references have and we do not ([#224](https://github.com/asm198x/asm198x/pull/224))
- *(ca65)* declare the ninety-seven directives ca65 has and we do not ([#222](https://github.com/asm198x/asm198x/pull/222))
- *(sjasmplus)* take the optional leading dot on every directive ([#221](https://github.com/asm198x/asm198x/pull/221))
- *(xtask)* measure how much of each reference's vocabulary we take ([#212](https://github.com/asm198x/asm198x/pull/212))
- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(xtask)* stop counting a wider target as a gap ([#219](https://github.com/asm198x/asm198x/pull/219))
- *(cp1610)* speak strict asl, and give the corpus a way to retire a listing ([#218](https://github.com/asm198x/asm198x/pull/218))
- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- release v0.0.31
- ca65's visibility words need all three answers, and rgbasm's needs none
- Refusing a word the reference refuses is not a gap: a category for it
- Give each surface run its own scratch directory
- release v0.0.30 ([#217](https://github.com/asm198x/asm198x/pull/217))
- release v0.0.29 ([#208](https://github.com/asm198x/asm198x/pull/208))
- release v0.0.28 ([#204](https://github.com/asm198x/asm198x/pull/204))
- release v0.0.27 ([#197](https://github.com/asm198x/asm198x/pull/197))
- release v0.0.26 ([#189](https://github.com/asm198x/asm198x/pull/189))
- release v0.0.25 ([#182](https://github.com/asm198x/asm198x/pull/182))
- Say which failure it is: pasmo's unimplemented include, and one wording for an unknown word ([#181](https://github.com/asm198x/asm198x/pull/181))
- Generate a page per dialect, and plan the gaps it exposes ([#180](https://github.com/asm198x/asm198x/pull/180))
- release v0.0.24 ([#173](https://github.com/asm198x/asm198x/pull/173))
- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.31](https://github.com/asm198x/asm198x/compare/xtask-v0.0.30...xtask-v0.0.31) - 2026-08-24

### Added

- declare what the last four references have and we do not ([#224](https://github.com/asm198x/asm198x/pull/224))
- *(ca65)* declare the ninety-seven directives ca65 has and we do not ([#222](https://github.com/asm198x/asm198x/pull/222))
- *(sjasmplus)* take the optional leading dot on every directive ([#221](https://github.com/asm198x/asm198x/pull/221))
- *(xtask)* measure how much of each reference's vocabulary we take ([#212](https://github.com/asm198x/asm198x/pull/212))
- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(xtask)* stop counting a wider target as a gap ([#219](https://github.com/asm198x/asm198x/pull/219))
- *(cp1610)* speak strict asl, and give the corpus a way to retire a listing ([#218](https://github.com/asm198x/asm198x/pull/218))
- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- ca65's visibility words need all three answers, and rgbasm's needs none
- Refusing a word the reference refuses is not a gap: a category for it
- Give each surface run its own scratch directory
- release v0.0.30 ([#217](https://github.com/asm198x/asm198x/pull/217))
- release v0.0.29 ([#208](https://github.com/asm198x/asm198x/pull/208))
- release v0.0.28 ([#204](https://github.com/asm198x/asm198x/pull/204))
- release v0.0.27 ([#197](https://github.com/asm198x/asm198x/pull/197))
- release v0.0.26 ([#189](https://github.com/asm198x/asm198x/pull/189))
- release v0.0.25 ([#182](https://github.com/asm198x/asm198x/pull/182))
- Say which failure it is: pasmo's unimplemented include, and one wording for an unknown word ([#181](https://github.com/asm198x/asm198x/pull/181))
- Generate a page per dialect, and plan the gaps it exposes ([#180](https://github.com/asm198x/asm198x/pull/180))
- release v0.0.24 ([#173](https://github.com/asm198x/asm198x/pull/173))
- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.30](https://github.com/asm198x/asm198x/compare/xtask-v0.0.29...xtask-v0.0.30) - 2026-08-23

### Added

- *(xtask)* measure how much of each reference's vocabulary we take ([#212](https://github.com/asm198x/asm198x/pull/212))
- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(cp1610)* speak strict asl, and give the corpus a way to retire a listing ([#218](https://github.com/asm198x/asm198x/pull/218))
- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- release v0.0.29 ([#208](https://github.com/asm198x/asm198x/pull/208))
- release v0.0.28 ([#204](https://github.com/asm198x/asm198x/pull/204))
- release v0.0.27 ([#197](https://github.com/asm198x/asm198x/pull/197))
- release v0.0.26 ([#189](https://github.com/asm198x/asm198x/pull/189))
- release v0.0.25 ([#182](https://github.com/asm198x/asm198x/pull/182))
- Say which failure it is: pasmo's unimplemented include, and one wording for an unknown word ([#181](https://github.com/asm198x/asm198x/pull/181))
- Generate a page per dialect, and plan the gaps it exposes ([#180](https://github.com/asm198x/asm198x/pull/180))
- release v0.0.24 ([#173](https://github.com/asm198x/asm198x/pull/173))
- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.29](https://github.com/asm198x/asm198x/compare/xtask-v0.0.28...xtask-v0.0.29) - 2026-08-23

### Added

- *(xtask)* measure how much of each reference's vocabulary we take ([#212](https://github.com/asm198x/asm198x/pull/212))
- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- release v0.0.28 ([#204](https://github.com/asm198x/asm198x/pull/204))
- release v0.0.27 ([#197](https://github.com/asm198x/asm198x/pull/197))
- release v0.0.26 ([#189](https://github.com/asm198x/asm198x/pull/189))
- release v0.0.25 ([#182](https://github.com/asm198x/asm198x/pull/182))
- Say which failure it is: pasmo's unimplemented include, and one wording for an unknown word ([#181](https://github.com/asm198x/asm198x/pull/181))
- Generate a page per dialect, and plan the gaps it exposes ([#180](https://github.com/asm198x/asm198x/pull/180))
- release v0.0.24 ([#173](https://github.com/asm198x/asm198x/pull/173))
- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.28](https://github.com/asm198x/asm198x/compare/xtask-v0.0.27...xtask-v0.0.28) - 2026-08-23

### Added

- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- release v0.0.27 ([#197](https://github.com/asm198x/asm198x/pull/197))
- release v0.0.26 ([#189](https://github.com/asm198x/asm198x/pull/189))
- release v0.0.25 ([#182](https://github.com/asm198x/asm198x/pull/182))
- Say which failure it is: pasmo's unimplemented include, and one wording for an unknown word ([#181](https://github.com/asm198x/asm198x/pull/181))
- Generate a page per dialect, and plan the gaps it exposes ([#180](https://github.com/asm198x/asm198x/pull/180))
- release v0.0.24 ([#173](https://github.com/asm198x/asm198x/pull/173))
- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.27](https://github.com/asm198x/asm198x/compare/xtask-v0.0.26...xtask-v0.0.27) - 2026-08-23

### Added

- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- release v0.0.26 ([#189](https://github.com/asm198x/asm198x/pull/189))
- release v0.0.25 ([#182](https://github.com/asm198x/asm198x/pull/182))
- Say which failure it is: pasmo's unimplemented include, and one wording for an unknown word ([#181](https://github.com/asm198x/asm198x/pull/181))
- Generate a page per dialect, and plan the gaps it exposes ([#180](https://github.com/asm198x/asm198x/pull/180))
- release v0.0.24 ([#173](https://github.com/asm198x/asm198x/pull/173))
- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.26](https://github.com/asm198x/asm198x/compare/xtask-v0.0.25...xtask-v0.0.26) - 2026-08-23

### Added

- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- release v0.0.25 ([#182](https://github.com/asm198x/asm198x/pull/182))
- Say which failure it is: pasmo's unimplemented include, and one wording for an unknown word ([#181](https://github.com/asm198x/asm198x/pull/181))
- Generate a page per dialect, and plan the gaps it exposes ([#180](https://github.com/asm198x/asm198x/pull/180))
- release v0.0.24 ([#173](https://github.com/asm198x/asm198x/pull/173))
- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.25](https://github.com/asm198x/asm198x/compare/xtask-v0.0.24...xtask-v0.0.25) - 2026-08-22

### Added

- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- Say which failure it is: pasmo's unimplemented include, and one wording for an unknown word ([#181](https://github.com/asm198x/asm198x/pull/181))
- Generate a page per dialect, and plan the gaps it exposes ([#180](https://github.com/asm198x/asm198x/pull/180))
- release v0.0.24 ([#173](https://github.com/asm198x/asm198x/pull/173))
- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.24](https://github.com/asm198x/asm198x/compare/xtask-v0.0.23...xtask-v0.0.24) - 2026-08-22

### Added

- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- Generate the search index mdBook's withdrawal left missing ([#179](https://github.com/asm198x/asm198x/pull/179))
- *(ci)* fail while the newest changelog entry still reads like a draft ([#177](https://github.com/asm198x/asm198x/pull/177))
- *(deps)* bump sha2 from 0.10.9 to 0.11.0 ([#151](https://github.com/asm198x/asm198x/pull/151))
- Compare against what a reader already has, without guessing ([#176](https://github.com/asm198x/asm198x/pull/176))
- release v0.0.23 ([#169](https://github.com/asm198x/asm198x/pull/169))
- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.23](https://github.com/asm198x/asm198x/compare/xtask-v0.0.22...xtask-v0.0.23) - 2026-08-21

### Added

- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- Explain multi-file projects, with the resolution anchors generated ([#168](https://github.com/asm198x/asm198x/pull/168))
- release v0.0.22 ([#156](https://github.com/asm198x/asm198x/pull/156))
- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.22](https://github.com/asm198x/asm198x/compare/xtask-v0.0.21...xtask-v0.0.22) - 2026-08-21

### Added

- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- Generate /why's evidence figures, and shrink the introduction to orientation ([#166](https://github.com/asm198x/asm198x/pull/166))
- Prove the declared directive surface, and generate the migration table from it ([#165](https://github.com/asm198x/asm198x/pull/165))
- release v0.0.21 ([#159](https://github.com/asm198x/asm198x/pull/159))
- publish where we differ, and make the case for adopting it ([#158](https://github.com/asm198x/asm198x/pull/158))
- release v0.0.20 ([#155](https://github.com/asm198x/asm198x/pull/155))
- release v0.0.19 ([#150](https://github.com/asm198x/asm198x/pull/150))
- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.19](https://github.com/asm198x/asm198x/compare/xtask-v0.0.18...xtask-v0.0.19) - 2026-08-21

### Added

- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))
- release v0.0.18 ([#149](https://github.com/asm198x/asm198x/pull/149))
- release v0.0.17 ([#147](https://github.com/asm198x/asm198x/pull/147))
- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.18](https://github.com/asm198x/asm198x/compare/xtask-v0.0.17...xtask-v0.0.18) - 2026-08-21

### Added

- *(xtask)* carry the nav and the dead-link gate without mdBook ([#148](https://github.com/asm198x/asm198x/pull/148))
- *(xtask)* generate the curriculum parity figures from the corpus ([#146](https://github.com/asm198x/asm198x/pull/146))

## [0.0.17](https://github.com/asm198x/asm198x/compare/xtask-v0.0.16...xtask-v0.0.17) - 2026-08-21

### Added

- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

### Other

- release v0.0.16 ([#145](https://github.com/asm198x/asm198x/pull/145))
- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.16](https://github.com/asm198x/asm198x/compare/xtask-v0.0.15...xtask-v0.0.16) - 2026-08-20

### Added

- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Other

- release v0.0.15 ([#143](https://github.com/asm198x/asm198x/pull/143))
- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.15](https://github.com/asm198x/asm198x/compare/xtask-v0.0.14...xtask-v0.0.15) - 2026-08-20

### Added

- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Other

- *(book)* name each page's sources instead of explaining the citation policy
- Let release-plz determine versions again, and cut v0.0.14 ([#142](https://github.com/asm198x/asm198x/pull/142))
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.14](https://github.com/asm198x/asm198x/compare/xtask-v0.0.13...xtask-v0.0.14) - 2026-08-20

### Added

- *(isa)* record where each specification came from, and cite it ([#140](https://github.com/asm198x/asm198x/pull/140))
- *(docs)* render the Z8000, completing the instruction reference ([#139](https://github.com/asm198x/asm198x/pull/139))
- *(docs)* render the 6809, and give its spec the summaries it lacked ([#138](https://github.com/asm198x/asm198x/pull/138))
- *(docs)* render the 68000 in the instruction reference ([#137](https://github.com/asm198x/asm198x/pull/137))
- *(docs)* render the word-oriented CPUs in the reference ([#136](https://github.com/asm198x/asm198x/pull/136))
- *(docs)* generate the instruction reference from the spec (R1) ([#135](https://github.com/asm198x/asm198x/pull/135))
- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))

### Other

- Let release-plz determine versions again
- Link each CPU to the machines that used it, and stop the 68000 operand column rendering blank ([#141](https://github.com/asm198x/asm198x/pull/141))
- release v0.0.13 ([#101](https://github.com/asm198x/asm198x/pull/101))

## [0.0.13](https://github.com/asm198x/asm198x/compare/xtask-v0.0.12...xtask-v0.0.13) - 2026-08-19

### Added

- *(docs)* build the book from the repo that can invalidate it ([#132](https://github.com/asm198x/asm198x/pull/132))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* one command to grow the corpus, and a note for contributors ([#116](https://github.com/asm198x/asm198x/pull/116))
- *(xtask)* generate the conformance ledger from the corpus ([#115](https://github.com/asm198x/asm198x/pull/115))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))
