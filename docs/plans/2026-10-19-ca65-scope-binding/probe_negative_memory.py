"""Retain absolute-memory range controls independently of scoped lookup."""

import probe


def cases() -> dict[str, str]:
    out = {}
    for operation in [
        "lda {x}",
        "lda {x},x",
        "sta {x},y",
        "jmp {x}",
        "jmp ({x})",
        "lda #{x}",
        ".word {x}",
    ]:
        for number in ["-1", "-128", "-32768", "$FFFF"]:
            out[f"negative_memory_{operation}_{number}"] = (
                operation.format(x=number) + "\n"
            )
    return out


if __name__ == "__main__":
    probe.cases = cases
    probe.main()
