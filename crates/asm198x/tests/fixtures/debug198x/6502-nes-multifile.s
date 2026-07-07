.segment "CODE"
reset:  lda #$01
.include "6502-nes-multifile.inc"
.segment "VECTORS"
        .word 0, reset, 0
