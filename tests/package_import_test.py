def test_package_imports():
    import bindings.python as cet
    assert cet.CETRuntimeV4 is not None
    assert cet.SecurityGraphBuilder is not None


if __name__ == "__main__":
    test_package_imports(); print("package imports ok")
