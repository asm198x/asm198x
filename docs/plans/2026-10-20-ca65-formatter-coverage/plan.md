# Restore resolved ca65 formatter checks

The numeric-admission and repeat/end fixtures still mark 18 cases with closed
issues #833/#834. The clean producer 3606d654 formats all 18 idempotently and
reassembles their exact native bytes. The flags suppress working regression
coverage; this is separate from the 185 genuine #821 formatter failures.

1. Retain all 18 process replays and original fixture/native references. File
   the measured coverage gap before changing metadata.
2. Remove only resolved #833/#834 formatter flags from the two fixtures. Their
   existing Rust and package loops must now execute the normal four-command
   native/formatter round trip. Leave actual #821 flags until its fix qualifies.
3. Run both complete fixture suites and Node/package checks. Inspect retained
   package commands to prove the newly enabled formats actually ran; source
   behavior needs no change. Include the normal full-slice verification and
   cleanup when landing this alongside the HuC6280 formatter fix.
