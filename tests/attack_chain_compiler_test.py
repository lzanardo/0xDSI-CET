from bindings.python.attack_chain import compile_attack_chain


def test_attack_chain_compiles_to_query_registry():
    doc = {
        "chain_id": "priv_esc_exfil",
        "version": "v1",
        "name": "Privilege escalation to exfil",
        "steps": [
            {"event_type": "AuthFail", "min_repeats": 1, "max_repeats": None},
            {"event_type": "PrivEsc"},
            {"event_type": "DataAccess"},
        ],
        "within_ms": 1800000,
        "relations": [{"field":"user_id"}],
        "mitre": ["T1078"],
    }
    reg = compile_attack_chain(doc)
    q = reg["queries"][0]
    assert q["pattern"] == "AuthFail+,PrivEsc,DataAccess"
    assert "T1078" in q["tags"]


if __name__ == "__main__":
    test_attack_chain_compiles_to_query_registry(); print("attack chain compiler ok")
