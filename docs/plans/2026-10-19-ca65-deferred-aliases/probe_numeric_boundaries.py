"""Retain numeric-token follow-up controls for the separate issue #788."""

import probe


def cases() -> dict[str, str]:
    out = {}
    for value in [
        4294967294,
        4294967295,
        4294967296,
        4294967299,
        4294967300,
        4294967305,
        4294967306,
        9223372036854775807,
    ]:
        for base, literal in [
            ("decimal", str(value)),
            ("hex_prefix", "$" + format(value, "X")),
            ("hex_suffix", "0" + format(value, "X") + "h"),
            ("binary", "%" + format(value, "b")),
        ]:
            for context, source in [
                ("unused_alias", f"OTHER={literal}\n.byte 7\n"),
                (
                    "empty_tag",
                    f".struct Empty\n.endstruct\n.struct Record\nfield .tag Empty {literal}\n.endstruct\n.byte 7\n",
                ),
            ]:
                out[f"{context}_{base}_{value}"] = source
    return out


if __name__ == "__main__":
    probe.cases = cases
    probe.main()
