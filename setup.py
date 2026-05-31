from __future__ import annotations

from pathlib import Path
import os
import platform
import shutil
import subprocess

from setuptools import setup, find_packages
from setuptools.command.build_py import build_py as _build_py


class build_py(_build_py):
    """Build the native C shared library and include it inside the wheel.

    Set OXDSI_SKIP_NATIVE_BUILD=1 to skip CMake, useful for pure-Python static
    analysis jobs. Normal release builds should leave it enabled.
    """

    def run(self):
        if os.getenv("OXDSI_SKIP_NATIVE_BUILD", "").lower() not in {"1", "true", "yes"}:
            self._build_native()
        super().run()
        self._copy_native_into_build_lib()

    def _lib_name(self) -> str:
        system = platform.system().lower()
        if system == "darwin":
            return "liboxdsi_cet.dylib"
        if system == "windows":
            return "oxdsi_cet.dll"
        return "liboxdsi_cet.so"

    def _build_native(self) -> None:
        build_dir = Path("build")
        build_dir.mkdir(exist_ok=True)
        cfg = os.getenv("CMAKE_BUILD_TYPE", "Release")
        subprocess.check_call(["cmake", "-S", ".", "-B", str(build_dir), f"-DCMAKE_BUILD_TYPE={cfg}"])
        subprocess.check_call(["cmake", "--build", str(build_dir), "--config", cfg])

    def _copy_native_into_build_lib(self) -> None:
        libname = self._lib_name()
        source_candidates = [Path("build") / libname, Path("build") / "Release" / libname]
        src = next((p for p in source_candidates if p.exists()), None)
        if src is None:
            if os.getenv("OXDSI_SKIP_NATIVE_BUILD", "").lower() in {"1", "true", "yes"}:
                return
            raise FileNotFoundError(f"native CET library not found in build output: {source_candidates}")
        dest_dir = Path(self.build_lib) / "bindings" / "python" / "native"
        dest_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dest_dir / libname)


setup(
    packages=find_packages(include=["bindings", "bindings.*", "jobs", "jobs.*"]),
    include_package_data=True,
    package_data={"bindings.python": ["native/*.so", "native/*.dylib", "native/*.dll"]},
    cmdclass={"build_py": build_py},
)
