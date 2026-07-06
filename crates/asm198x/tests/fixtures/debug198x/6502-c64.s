* = $c000
border = $d020
start:
    lda #$02
    sta border
loop:
    dex
    bne loop
    rts
data:
    !byte 1, 2, 3
