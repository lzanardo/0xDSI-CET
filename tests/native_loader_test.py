from bindings.python.native_loader import platform_library_name, candidate_library_paths


def test_native_loader_candidates_are_ordered():
    name = platform_library_name()
    assert name.startswith("liboxdsi_cet") or name == "oxdsi_cet.dll"
    candidates = candidate_library_paths("/tmp/custom.so")
    assert str(candidates[0]) == "/tmp/custom.so"


if __name__ == "__main__":
    test_native_loader_candidates_are_ordered(); print("native loader ok")
