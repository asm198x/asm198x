# Configured far-address baseline

The 190-case audit reproduces all original 90 native outcomes and tool hashes.
Native accepts 136 and refuses 54. Checked #920 differs on 51: 39 false accepts
outside the unsigned 24-bit range and 12 wrong-byte cases at PC carry boundaries.
There are no native-accepted refusals. All direct/include/macro and lazy controls
are retained with source/config/process streams.

The configured parser wraps every original faraddr value in Lo/Hi/Bank and
returns Kind::Bytes. link::emit then evaluates these three clones at three
successive PCs and only checks each extracted byte. This loses both the complete
24-bit range and the original expression's single evaluation position.
`.org $ffff / .faraddr *` is ffff00 natively but ff0001 here.

Retain a private FarAddrs Vec<Expr> through Kind and Resolved, as the existing
Words/DWords variants do. At final emission use link::to_width(v,0xffffff), emit
three little-endian bytes and advance PC by three once per original value.
Update demanded-value widths, symbol mapping and source offset metadata through
the same existing matches. The configured backend already checks unsigned data;
there is no shared engine Operation change or new range policy to introduce.

The original expression functions, lazy source, scoped aliases, flat dialects,
byte extractors and file/layout resolver remain on their existing paths.
