# BranchOutOfRange

A relative branch encodes a distance, not a complete destination address.
The destination exists, but its displacement does not fit the selected
instruction's field. The diagnostic reports the distance and allowed range.

For an ordinary 8-bit PC-relative branch, the displacement is signed:
`-128` through `127`. It is measured from the instruction's architectural
reference address, usually the address immediately after the branch. For
example, a two-byte branch at `$1000` targeting `$1082` needs a displacement
of `$1082 - $1002 = 128`, one beyond the positive limit. It can reach `$1081`.
Other CPUs use different field widths, reference addresses or address units;
use the range printed for the instruction rather than assuming bytes.

Possible fixes are to move the target closer, select a longer branch form
where the CPU and dialect provide one, or invert the condition and jump:

```asm
; 6502, ACME syntax: replace an out-of-range BNE distant
    beq nearby
    jmp distant
nearby:
```

Check both paths after such a change. The replacement uses more space and
different cycle counts; it can matter inside an interrupt handler or a
raster-timed routine. Do not truncate the displacement or change the label's
value merely to silence the error: that changes where execution goes.

The assembler does not silently rewrite a short branch into this sequence.
Whether a dialect relaxes a branch is part of its reference behaviour, not
permission to change every branch automatically.
