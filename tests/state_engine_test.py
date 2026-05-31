from bindings.python.state import PartialTrendState, compact_states, expire_states


def test_state_compaction_and_expiry():
    s1 = PartialTrendState("q","v1","p",(1,2),1,2,10,{})
    s2 = PartialTrendState("q","v1","p",(1,2),1,3,12,{"new":True})
    s3 = PartialTrendState("q","v1","p",(9,),9,9,9,{})
    compacted = compact_states([s1,s2,s3])
    assert len(compacted) == 2
    assert any(s.payload.get("new") for s in compacted)
    assert len(expire_states(compacted, 10)) == 1


if __name__ == "__main__":
    test_state_compaction_and_expiry(); print("state engine ok")
