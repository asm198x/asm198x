# Plan: ACME's Force Bits

**Status:** Proposed and built 2026-11-05, all five steps, postfixes
included. See "Outcome" below.
**Binding records:** `decisions/acme-zero-page.md` (zero-page sizing from a
tracked location counter), `decisions/reference-parity-goal.md` (a
reference's rejection stays a rejection).
**Issues:** #967 (sources ACME rejects as "Number out of range"), the Atari
2600 `tile-map` source in #968 (different bytes).

## The problem

ACME gives a value **Force Bits** that choose its addressing width
(`AddrModes.txt` in ACME 0.97's documentation). Asm198x only notices a
leading-zero hex literal written directly in the operand
(`address_forces_absolute`, a text check), so it loses the force as soon as
the value passes through a symbol or an expression:

| Source | ACME 0.97 | Asm198x 0.0.74 |
|---|---|---|
| `NR=$0070` / `lda NR` | `AD 70 00` | `A5 70` — wrong bytes |
| `lda $0070+0` | `AD 70 00` | `A5 70` |
| `lda %0000000001110000` | `AD 70 00` | `A5 70` |
| `NR=$0070` / `lda (NR),y` | Number out of range | `B1 70` |
| `lda $00070` (6502) | Number out of range | `AD 70 00` |
| `lda+2 $70`, `S+2 = $fd` | `AD 70 00`, absolute | cannot parse |

## What ACME does, measured on 0.97

- **A literal's written width forces it.** Hex: more than 2 digits forces 16
  bits, more than 4 forces 24 (`$070` and `$0070` are 16, `$00070` is 24).
  Binary: more than 8 digits 16, more than 16 digits 24. Octal (`&`): more
  than 3 digits 16. Decimal never forces (`0070` is 70, zero page).
- **A symbol keeps the force of the expression that defined it**, and passes
  it on (`T = S+1`). A label set by the location counter has none.
- **Across an expression the widest force wins** (`$0070-$0070+$70` is 16).
- **`<`, `>` and `^` force 8 bits over what they cover**, and they bind
  loosely: `<NR + $0001` is 8, but `$0001 + <NR` and `(<NR) + $0001` are 16.
  This is the tree `mos6502::parse_expr` already builds with
  `BytePrec::Loose`.
- **Postfixes:** `+1`/`+2`/`+3` after a mnemonic set the force and beat the
  operand's; after a symbol's name in its definition (`S+1 = $00fd`) they
  replace the value's. `jmp+1` and `lda+3` on a 6502 are "Illegal
  combination of command and postfix".
- **Forced wider than a mode allows is "Number out of range"**: an immediate,
  `(zp),y`, `(zp,x)` or `(zp)` with a 16-bit force, any 24-bit force on a
  6502, and `+1` with a value over 255.

## The change

1. Literals carry their written width out of the ACME number reader, and the
   walk computes an operand's force over the parsed tree: a literal's width,
   a symbol's stored force, byte operators reset to 8, widest wins.
2. The walk stores a force beside each symbol it defines with `=` (or a
   postfixed name), and none for labels.
3. Mode choice takes the mnemonic postfix, else the operand force, else the
   value as today (`acme-zero-page.md` is unchanged for unforced operands).
   A forced width the mode cannot take is "Number out of range".
4. `address_forces_absolute` is deleted; its test moves to the new rule.
5. `acme-zero-page.md` gains a section saying force bits come first.

## Verification

- Each row above, and each measured rule, becomes a test against ACME 0.97's
  recorded bytes or rejection.
- The ACME verdict corpus and the curriculum's ACME sources replay unchanged
  except where ACME agrees with the new bytes; any change is listed.
- #967's Number-out-of-range sources at code-samples `5bade0fa` are
  rejected, and #968's Atari 2600 `tile-map` matches ACME byte for byte.

## Outcome

Built as planned, with two changes of method. The shared expression parser
keeps folding literals into numbers. A small ACME scanner in
`dialects/acme/force.rs` reads the Force Bits from the operand's text
instead. And ACME's `&` octal, `0x`, `0b` and `^` bank-byte spellings had to
be read first, because none of them was.

The rules above were measured on the ACME 0.97 installed here, the 28 June
2020 build. Every case was measured again on the arbiter's pin, SVN r448 (1
Aug 2025), built in a throwaway container. The two builds give the same bytes
and the same refusals in all 172 cases. Only one message differs: for a
postfix the instruction cannot take, r448 says "CPU does not support this
postfix for this mnemonic" where the 2020 build said "Illegal combination of
command and postfix". We use r448's words.

Measuring turned up rules the table above did not have:

- A literal forces only while its value would fit the narrower width.
  `$0100` carries no force, so `$1000-$0fff` sizes by its value, 1.
- A comparison's result carries no force.
- `!set` accumulates force, while a loop variable and `*` carry none.
- A force picks the narrowest form at least that wide. So `jmp <$1234` is
  still absolute, and only a shape with no form wide enough is "Number out of
  range". A postfix must match a form exactly.
- Data, branches and `rmb`/`smb`/`bbr`/`bbs` ignore a force.
- A postfix on a symbol already used is "Too late for postfix".
- On the 65816, a 24-bit force picks the long forms, with `jsr` becoming
  `jsl`, and an immediate ignores its force.

The 65816 has two cases left as gaps. One is a forward symbol that turns out
to force 24 bits, which ACME widens after the fact. The other is `+2` on an
immediate. Both are refused, and the message says the gap is ours.

The curriculum at code-samples `639e9da0` changes in 13 sources and no
others. The Atari 2600 `tile-map` now matches ACME byte for byte. The
twelve `object-pool` sources for the BBC Micro and the Atari 800XL are now
refused as "Number out of range", at the line where ACME refuses them. The
eight #967 sources at `5bade0fa` are refused at ACME's first line too.

## Not in this plan

ACME resolving `!source` from the folder it runs in (the other half of #967)
is a separate change with its own corpus check. #968's two BBC Micro sources
differ through forward-reference sizing, not force bits.
