	org 8000h
BORDER	equ 254
start:
	ld a,2
	out (BORDER),a
loop:	djnz loop
	ret
msg:	db "ok",0
	end start
