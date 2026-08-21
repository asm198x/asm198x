# Quickstart

Install the binary, assemble a program for the machine you care about, and run
it. Five minutes, no project layout, no build file.

## Install

```sh
# macOS, Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/asm198x/asm198x/releases/latest/download/asm198x-installer.sh | sh
```

```powershell
# Windows
irm https://github.com/asm198x/asm198x/releases/latest/download/asm198x-installer.ps1 | iex
```

Homebrew, direct archives and the rest are on [Install](install.md).

```sh
asm198x --version
```

## Pick a machine

The same command assembles for all of them. What changes is `--dialect`, which
picks the syntax your source is written in, and the flag naming what to write —
a `.prg`, a snapshot, an executable.

### Commodore 64

<!-- sample: acme -->
```asm
        *= $0801
        ; BASIC stub, so RUN reaches the code: 10 SYS 2064
        !byte $0c,$08,$0a,$00,$9e,$32,$30,$36,$34,$00,$00,$00

        *= $0810
        lda #$05                ; green
        sta $d020               ; border colour
        rts
```

```sh
asm198x --dialect acme --prg border.asm -o border.prg
```

Load it in an emulator and `RUN`. The border turns green and control returns to
BASIC.

### ZX Spectrum

<!-- sample: pasmo -->
```asm
        org $8000
start:  ld a, 2                 ; red
        out ($fe), a            ; border
loop:   jr loop
        end start
```

```sh
asm198x --dialect pasmo --sna border.asm -o border.sna
```

Open the snapshot. The border turns red and the program loops — a snapshot owns
the machine, so there is nowhere to return to. `end start` is what tells the
snapshot where to begin.

### Amiga

<!-- sample: vasm -->
```asm
        section code,code
start:  moveq   #0,d0           ; return code 0
        rts
```

```sh
asm198x --dialect vasm --exe border.asm -o border
```

That writes a loadable hunk executable, which is what AmigaDOS runs. It exits
quietly with a return code of nothing-went-wrong.

## What to read next

- [A first program](guide/first-program.md) — the same five front doors, with
  programs that do more than one thing.
- [Dialects](reference/dialects.md) — every syntax the binary accepts, and which
  assembler each one matches.
- [The command line](reference/cli.md) — every option, in one page.
