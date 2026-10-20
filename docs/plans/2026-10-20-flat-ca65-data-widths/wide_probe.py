"""Native unsigned wide-data boundary and delayed-resolution controls."""

import importlib.util
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).parent
FILES: dict[str, str] = {}


def load(path: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location("capture", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cases() -> dict[str, str]:
    broad = load(ROOT / "probe.py").cases()
    out = {
        name: source
        for name, source in broad.items()
        if name.split("_")[1] in {"dbyt", "dword"} and not name.endswith("_counter")
    }
    for cpu in ["65816", "huc6280"]:
        preamble = f'.setcpu "{cpu}"\n.org $1000\n'
        for directive, maximum in [("dbyt", 65535), ("dword", 4294967295)]:
            for context, body in {
                "included_negative": f".{directive} -1\n",
                "included_max": f".{directive} {maximum}\n",
                "forward_high": f".{directive} target+{maximum}\ntarget: .byte 7\n",
                "forward_negative": f".{directive} target-8192\ntarget: .byte 7\n",
                "arithmetic_high": f".{directive} {maximum}+1\n",
                "two_values": f".{directive} 1,{maximum}\n",
                "inactive": f".if 0\n.{directive} -1\n.endif\n.byte 7\n",
            }.items():
                name = f"{cpu}_{directive}_{context}"
                if context.startswith("included_"):
                    filename = name + ".inc"
                    FILES[filename] = body
                    body = f'.include "{filename}"\n'
                out[name] = preamble + body
    assert len(out) == 108
    return out


if __name__ == "__main__":
    runner = load(ROOT.parent / "2026-10-19-ca65-scope-binding/probe.py")
    prepared = cases()
    runner.FILES = FILES
    runner.cases = lambda: prepared
    runner.main()
