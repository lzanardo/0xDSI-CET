"""Native library resolution for packaged and source-tree 0xDSI CET deployments.

Resolution order:
1. explicit `lib_path` argument;
2. OXDSI_CET_LIB_PATH;
3. packaged wheel resource: bindings/python/native/liboxdsi_cet.*;
4. source-tree build directory: <repo>/build/liboxdsi_cet.*;
5. dynamic loader name: liboxdsi_cet.so / dylib / dll.

The loader intentionally does not import ctypes. It only resolves and verifies the
artifact so the bridge can remain small and testable.
"""
from __future__ import annotations

from pathlib import Path
import hashlib
import os
import platform


def platform_library_name() -> str:
    system = platform.system().lower()
    if system == "darwin":
        return "liboxdsi_cet.dylib"
    if system == "windows":
        return "oxdsi_cet.dll"
    return "liboxdsi_cet.so"


def _package_native_dir() -> Path:
    return Path(__file__).resolve().parent / "native"


def candidate_library_paths(explicit: str | os.PathLike[str] | None = None) -> list[Path]:
    libname = platform_library_name()
    candidates: list[Path] = []
    if explicit:
        candidates.append(Path(explicit))
    env = os.getenv("OXDSI_CET_LIB_PATH")
    if env:
        candidates.append(Path(env))
    candidates.append(_package_native_dir() / libname)
    repo_root = Path(__file__).resolve().parents[2]
    candidates.append(repo_root / "build" / libname)
    candidates.append(repo_root / "build" / "Release" / libname)
    return candidates


def resolve_native_library(explicit: str | os.PathLike[str] | None = None) -> str:
    for p in candidate_library_paths(explicit):
        if p.exists():
            return str(p)
    return platform_library_name()


def sha256_file(path: str | os.PathLike[str]) -> str:
    h = hashlib.sha256()
    with Path(path).open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def verify_native_library(path: str | os.PathLike[str], expected_sha256: str | None = None) -> None:
    expected = expected_sha256 if expected_sha256 is not None else os.getenv("OXDSI_CET_SHA256", "")
    if not expected:
        return
    p = Path(path)
    if not p.exists():
        if p.name == str(path):
            return
        raise FileNotFoundError(f"native CET library not found: {path}")
    actual = sha256_file(p)
    if actual.lower() != expected.lower():
        raise RuntimeError(f"Library checksum verification failed: expected={expected} actual={actual}")
