from bindings.python.bridge import CETBridge


def test_golden_sequence():
    b=CETBridge('build/liboxdsi_cet.so')
    q=b.parse_query('qv1','A,B,C',60000,10000)
    events=[(1,'p','A',1),(2,'p','B',2),(3,'p','C',3)]
    edges=[(1,2,0,10),(2,3,0,10)]
    out=b.run_hcet(q,events,edges)
    assert [1,2,3] in out.paths


if __name__ == '__main__':
    test_golden_sequence()
    print('golden replay ok')
