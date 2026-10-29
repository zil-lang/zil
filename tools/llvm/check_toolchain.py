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
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def read_pin(root=ROOT):
    version = (root / 'tools/llvm/version.txt').read_text().strip()
    if not re.fullmatch(r'(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)', version):
        raise ValueError('LLVM version pin must be canonical major.minor.patch')
    profile = json.loads((root / 'tools/toolchain-profile.json').read_text())
    rust = tomllib.loads((root / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    if profile['llvm_version'] != version or profile['llvm_api']['version_exact'] != version:
        raise ValueError('toolchain profile disagrees with tools/llvm/version.txt')
    if profile['rust_version'] != rust:
        raise ValueError('toolchain profile disagrees with rust-toolchain.toml')
    return version, rust


def identity(path):
    path = Path(path).resolve(strict=True)
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': digest}


def command(name, pattern, expected):
    path = shutil.which(name)
    if not path:
        raise RuntimeError(f'required executable not found: {name}')
    text = subprocess.check_output([path, '--version'], text=True, timeout=15)
    match = re.search(pattern, text)
    if not match or match.group(1) != expected:
        banner = text.splitlines()[0] if text else '<empty>'
        raise RuntimeError(f'expected {name} {expected}, got {banner}')
    return {**identity(path), 'version': match.group(1), 'banner': text.splitlines()[0]}


def main():
    report = {"status": "Fail", "checks": {}}
    try:
        version_pin, rust_pin = read_pin()
        report['required_llvm'] = version_pin
        report['required_rust'] = rust_pin
        if platform.system() != "Linux" or platform.machine() not in ("x86_64", "amd64"):
            raise RuntimeError('LLVM API host validation is Linux x86_64 only; see docs/LLVM-API.md')
        if any(os.environ.get(key) for key in ('LD_PRELOAD', 'LD_AUDIT')):
            raise RuntimeError('toolchain validation requires no dynamic-loader interposition')
        libdir = os.environ.get("ZIL_LLVM_LIB_DIR")
        libname = f"libLLVM-{version_pin.split('.')[0]}.so"
        if not libdir:
            raise RuntimeError(f'set ZIL_LLVM_LIB_DIR to the directory containing {libname}')
        path = (Path(libdir) / libname).resolve(strict=True)
        library = ctypes.CDLL(str(path))
        version = [ctypes.c_uint() for _ in range(3)]
        library.LLVMGetVersion.argtypes = [ctypes.POINTER(ctypes.c_uint)] * 3
        library.LLVMGetVersion.restype = None
        library.LLVMGetVersion(*(ctypes.byref(v) for v in version))
        actual = '.'.join(str(v.value) for v in version)
        if actual != version_pin:
            raise RuntimeError(f'expected LLVM {version_pin}, got {actual}')
        report['checks']['llvm'] = {**identity(path), 'version': actual}
        report['checks']['clang'] = command(
            os.environ.get('ZIL_CLANG', 'clang'), r'\bclang version ([0-9]+\.[0-9]+\.[0-9]+)(?:\s|$)', version_pin)
        # The default Win64 link driver is selected from this PATH entry.
        report['checks']['lld_link'] = command(
            'lld-link', r'\bLLD ([0-9]+\.[0-9]+\.[0-9]+)(?:\s|$)', version_pin)
        report['checks']['rustc'] = command(
            'rustc', r'^rustc ([0-9]+\.[0-9]+\.[0-9]+)(?:\s|$)', rust_pin)
        report['checks']['cargo'] = command(
            'cargo', r'^cargo ([0-9]+\.[0-9]+\.[0-9]+)(?:\s|$)', rust_pin)
        report['status'] = 'Pass'
    except (OSError, RuntimeError, ValueError, KeyError, subprocess.SubprocessError, AttributeError) as error:
        report['error'] = str(error)
    print(json.dumps(report, indent=2))
    return 0 if report['status'] == 'Pass' else 1

if __name__ == '__main__':
    sys.exit(main())
