SPEED = 3
.segment "ZEROPAGE"
pos:    .res 1
.segment "CODE"
reset:  lda #SPEED
        sta pos
loop:   jmp loop
.segment "VECTORS"
        .word 0, reset, 0
