from bindings.python.bridge import CETBridge


def test_within_window_enforced():
    b=CETBridge('build/liboxdsi_cet.so')
    q=b.parse_query('q','A,B',1,1)
    events=[(1,'p','A',1),(2,'p','B',100)]
    edges=[(1,2,0,100)]
    out=b.run_hcet(q,events,edges)
    assert out.paths == []


def test_skip_till_any_match_path_exists():
    b=CETBridge('build/liboxdsi_cet.so')
    q=b.parse_query('q','A,B,C',60000,10000)
    events=[(1,'p','A',1),(2,'p','X',2),(3,'p','B',3),(4,'p','C',4)]
    edges=[(1,2,0,10),(2,3,0,10),(3,4,0,10)]
    out=b.run_hcet(q,events,edges)
    assert any(p[-2:]==[3,4] for p in out.paths)


if __name__ == '__main__':
    test_within_window_enforced(); test_skip_till_any_match_path_exists(); print('property semantics ok')
