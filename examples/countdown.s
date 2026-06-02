; countdown.s — a tiny 6502 example for Asm198x (ACME syntax).
;
; Fills $0400..$0407 with zero by counting X down from 8, then returns.
; Assemble with:  asm198x examples/countdown.s -o countdown.bin

        *= $0200

start:  lda #$00        ; value to store
        ldx #$08        ; loop counter
loop:   sta $0400,x     ; store at $0400 + X
        dex             ; X = X - 1
        bne loop        ; until X wraps to zero
        rts
