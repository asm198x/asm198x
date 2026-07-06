	section code,code
start:	lea data(pc),a0
	moveq #5,d0
loop:	dbf d0,loop
	rts
	section data,data
data:	dc.w 1,2,3
msg:	dc.b "hi",0
	even
tail:	dc.l msg
