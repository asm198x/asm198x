"""Measure width inference separately from deferred numeric binding."""

import probe


def cases() -> dict[str, str]:
    out = {}
    for outer in ["$12", "$1234"]:
        for later in [None, "$34", "$1234"]:
            tail = "" if later is None else f"VALUE={later}\n"
            for expression in [
                "VALUE+255+1",
                "VALUE-1",
                "VALUE-250",
                "VALUE*20",
                "VALUE&255",
                "VALUE|256",
                "VALUE^256",
                "VALUE<<1",
                "VALUE>>1",
                "-VALUE",
                "~VALUE",
                "VALUE/2",
            ]:
                for alias in [False, True]:
                    definition = f"OTHER={expression}\n" if alias else ""
                    operand = "OTHER" if alias else expression
                    out[f"width_{outer}_{later}_{expression}_{alias}"] = (
                        f"VALUE={outer}\n.scope inner\n{definition}lda {operand}\n{tail}.endscope\n"
                    )
    return out


if __name__ == "__main__":
    probe.cases = cases
    probe.main()
