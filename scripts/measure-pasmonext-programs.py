"""Measures whole programs on PasmoNext 0.1.3: their bytes, symbols or rejection.

Reads cases from a file in which each case is a line `=== name`, then its
source lines, optionally `+++ file.bin` lines naming files to create first
(as hex bytes after the name, for INCBIN), and optionally `!!! options` lines
giving command-line options, such as `!!! --pass3`. Writes each case with what PasmoNext
did: `--- bytes <hex>` and `--- symbols` lines `name value`, or
`--- rejected <message>`, then `--- passes 3` if it switched to three passes.

Usage: python3 scripts/measure-pasmonext-programs.py <cases.txt> <out.txt>
Needs PasmoNext 0.1.3 on the PATH as `pasmo`. The output is
crates/asm198x/tests/fixtures/pasmonext-0.1.3-programs.txt, which
tests/pasmonext_lang198x.rs replays through Lang198x's PasmoNext front end.
The cases are the output with its `---` lines removed.
"""
import os, re, subprocess, sys, tempfile

def cases(path):
    name, lines, files, options = None, [], [], []
    for line in open(path):
        if line.startswith("=== "):
            if name:
                yield name, lines, files, options
            name, lines, files, options = line[4:].strip(), [], [], []
        elif line.startswith("+++ "):
            parts = line[4:].split()
            files.append((parts[0], bytes.fromhex("".join(parts[1:]))))
        elif line.startswith("!!! "):
            options.extend(line[4:].split())
        elif name is not None:
            lines.append(line.rstrip("\n"))
    if name:
        yield name, lines, files, options

def run(lines, files, options):
    with tempfile.TemporaryDirectory() as d:
        for fname, data in files:
            open(os.path.join(d, fname), "wb").write(data)
        open(os.path.join(d, "t.asm"), "w").write("\n".join(lines) + "\n")
        r = subprocess.run(["pasmo", *options, "t.asm", "t.bin", "t.sym"], cwd=d,
                           capture_output=True, text=True, timeout=60)
        out = (r.stdout + r.stderr).splitlines()
        three = ["--- passes 3"] if any("3 pass mode" in l for l in out) else []
        if r.returncode != 0 or not os.path.exists(os.path.join(d, "t.bin")):
            message = next((l[len("ERROR: "):] for l in out if l.startswith("ERROR: ")), None)
            if message is None:
                message = f"exit {r.returncode}"
            return ["--- rejected " + message.strip()] + three
        result = ["--- bytes " + open(os.path.join(d, "t.bin"), "rb").read().hex()]
        result.append("--- symbols")
        for line in open(os.path.join(d, "t.sym")):
            m = re.match(r"(\S+)\s+EQU\s+0?([0-9A-F]+)H", line)
            if m:
                result.append(f"{m.group(1)} {int(m.group(2), 16):04x}")
        return result + three

if __name__ == "__main__":
    src, dst = sys.argv[1:]
    with open(dst, "w") as out:
        out.write("# Programs assembled by PasmoNext 0.1.3 with scripts/measure-pasmonext-programs.py.\n")
        for name, lines, files, options in cases(src):
            out.write(f"=== {name}\n")
            for fname, data in files:
                out.write(f"+++ {fname} {data.hex()}\n")
            if options:
                out.write("!!! " + " ".join(options) + "\n")
            out.write("\n".join(lines) + "\n")
            out.write("\n".join(run(lines, files, options)) + "\n")
