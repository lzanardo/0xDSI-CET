from bindings.python.dsl import parse_pattern


def test_dsl_fuzz_smoke():
    patterns = [
        "A,B,C",
        "A+,B,C",
        "A?,B,C",
        "A{1,3},B,C",
        "(A|B),C",
        "ProcessStart,(FileWrite|CredentialAccess)?,NetworkConnect",
    ]
    for p in patterns:
        assert parse_pattern(p)


if __name__ == "__main__":
    test_dsl_fuzz_smoke(); print("dsl fuzz smoke ok")
