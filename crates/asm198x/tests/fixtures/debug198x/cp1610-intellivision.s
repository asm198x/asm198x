	org 5000h
COUNT	equ 4
start:	mvii COUNT,r0
loop:	decr r0
	bneq loop
done:	hlt
