"""Probe anonymous enums and direct self references before choosing a design."""


def cases() -> dict[str, str]:
    out = {
        "anonymous_defaults": ".enum\nRED\nGREEN\n.endenum\n.byte RED,GREEN\n",
        "anonymous_previous": ".enum\nRED=2\nGREEN=RED+1\n.endenum\n.byte RED,GREEN\n",
        "anonymous_forward": ".enum\nRED=GREEN\nGREEN=3\n.endenum\n.byte RED,GREEN\n",
        "anonymous_scope": ".scope inner\n.enum\nRED=GREEN\nGREEN=3\n.endenum\n.byte RED,GREEN\n.endscope\n",
        "named_scope_reopen": ".enum Colors\nRED=2\n.endenum\n.scope Colors\nGREEN=3\n.endscope\n.byte Colors::RED,Colors::GREEN\n",
        "default_after_undefined": ".enum Colors\nRED=MISSING\nGREEN\n.endenum\n.byte 7\n",
        "size_member_only": ".enum Colors\nRED=2\n.endenum\n.word .sizeof(Colors::RED)\n",
        "enum_nested_struct": ".struct Record\n.enum Colors\nRED=2\n.endenum\nfield .word\n.endstruct\n.byte Record::Colors::RED\n",
        "struct_nested_enum": ".enum Colors\n.struct Record\nfield .word\n.endstruct\nRED=2\n.endenum\n.byte Colors::RED\n",
        "plain_assignment_in_enum": ".enum Colors\nRED=2\nVALUE=RED+1\nGREEN\n.endenum\n.byte Colors::RED,Colors::VALUE,Colors::GREEN\n",
    }
    for outer in ["", "RED=9\n"]:
        for opening, closing, key in [
            ("", "", "RED"),
            (".enum Colors\n", ".endenum\n", "Colors::RED"),
            (".enum\n", ".endenum\n", "RED"),
        ]:
            if not opening and outer:
                continue
            for rhs in ["RED", "RED+1", "RED-RED", ".defined(RED)"]:
                for use in [".byte 7\n", f".byte {key}\n"]:
                    name = f"self_{bool(outer)}_{opening.strip()}_{rhs}_{use.strip()}"
                    out[name] = outer + opening + f"RED={rhs}\n" + closing + use
    return out


if __name__ == "__main__":
    import importlib.util
    from pathlib import Path

    source = Path(__file__).parents[1] / "2026-10-19-ca65-scope-binding/probe.py"
    spec = importlib.util.spec_from_file_location("native_capture", source)
    assert spec is not None and spec.loader is not None
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.cases = cases
    runner.main()
