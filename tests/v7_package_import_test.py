import bindings.python.standing as standing


def test_imports():
    assert standing.StandingTrendRuntime
    assert standing.MemoryTrendSink


if __name__ == "__main__":
    test_imports()
    print("v7 package imports ok")
