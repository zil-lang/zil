#!/usr/bin/env python3
"""Validate the explicit LLVM API toolchain. Does not install or choose a fallback."""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys


def main():
    report = {"status": "Fail", "required_llvm": "17.0.6", "checks": {}}
    try:
        if platform.system() != "Linux" or platform.machine() not in ("x86_64", "amd64"):
            raise RuntimeError("209 mainline host validation is Linux x86_64 only; use the documented reference build elsewhere")
        libdir = os.environ.get("ZIL_LLVM_LIB_DIR")
        if not libdir:
            raise RuntimeError("set ZIL_LLVM_LIB_DIR to the directory containing libLLVM-17.so")
        path = (Path(libdir) / "libLLVM-17.so").resolve(strict=True)
        library = ctypes.CDLL(str(path))
        version = [ctypes.c_uint() for _ in range(3)]
        library.LLVMGetVersion.argtypes = [ctypes.POINTER(ctypes.c_uint)] * 3
        library.LLVMGetVersion.restype = None
        library.LLVMGetVersion(*(ctypes.byref(v) for v in version))
        actual = '.'.join(str(v.value) for v in version)
        if actual != "17.0.6":
            raise RuntimeError(f"LLVM library version mismatch: {actual}")
        report['checks']['llvm'] = {'path': str(path), 'version': actual, 'sha256': hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()}
        clang = shutil.which(os.environ.get('ZIL_CLANG', 'clang'))
        if not clang:
            raise RuntimeError('ZIL_CLANG is not an executable')
        text = subprocess.check_output([clang, '--version'], text=True, timeout=15)
        if not re.search(r'clang version 17\.0\.6(?:\s|$)', text):
            raise RuntimeError(f'expected Clang 17.0.6, got {text.splitlines()[0]}')
        report['checks']['clang'] = {'path': clang, 'version': text.splitlines()[0]}
        rust = subprocess.check_output(['rustc', '--version'], text=True, timeout=15).strip()
        if not rust.startswith('rustc 1.99.1 '):
            raise RuntimeError(f'expected pinned Rust 1.99.1, got {rust}')
        report['checks']['rustc'] = rust
        report['status'] = 'Pass'
    except (OSError, RuntimeError, subprocess.SubprocessError, AttributeError) as error:
        report['error'] = str(error)
    print(json.dumps(report, indent=2))
    return 0 if report['status'] == 'Pass' else 1

if __name__ == '__main__':
    sys.exit(main())
