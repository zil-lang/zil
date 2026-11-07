#!/usr/bin/env python3
"""Replay explicit byte storage, owned ABI and borrowed windows on x64.

The program is entirely ZIL. Win64 execution uses Wine and is not a claim of
native Windows/driver validation. This bounded runner makes no timing claim.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("compiler", "imports", "heap-library", "wine", "clang", "linker", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    out = args.out.absolute()
    out.mkdir(parents=True, exist_ok=False)
    commands, checks, binaries = [], [], []

    def check(name, okay, detail=None):
        checks.append({"name": name, "passed": bool(okay), "detail": detail})
        if not okay:
            raise AssertionError(name)

    def run(argv, name, expected=0):
        argv = [str(arg) for arg in argv]
        try:
            result = subprocess.run(argv, capture_output=True, timeout=45, cwd=root)
        except subprocess.TimeoutExpired as error:
            (out / f"{name}.stdout").write_bytes(error.stdout or b"")
            (out / f"{name}.stderr").write_bytes(error.stderr or b"")
            commands.append({"name": name, "argv": argv, "exit_code": None, "timeout": True})
            raise
        (out / f"{name}.stdout").write_bytes(result.stdout)
        (out / f"{name}.stderr").write_bytes(result.stderr)
        commands.append({"name": name, "argv": argv, "exit_code": result.returncode})
        check(name + ": process exit", result.returncode == expected, result.returncode)
        return result.stdout

    error = None
    tools = {}
    inputs = {}
    try:
        for tool in (args.compiler, args.wine, args.clang, args.linker):
            check("tool " + tool.name, tool.is_file())
            tools[str(tool.absolute())] = sha(tool)
        libraries = [args.imports.absolute() / "kernel32.lib", args.heap_library.absolute()]
        for library in libraries:
            check("explicit import archive " + library.name, library.is_file())
            inputs[str(library)] = sha(library)
        for name, tool in (("compiler", args.compiler), ("clang", args.clang),
                           ("linker", args.linker), ("wine", args.wine)):
            run([tool.absolute(), "--version"], name + "-version")
        source = root / "examples/mainline/byte_buffer/portable.zil"
        for target in ("native", "win64"):
            for optimization in ("O0", "O2"):
                name = target + "-" + optimization
                directory = out / name
                directory.mkdir()
                binary = directory / ("program.exe" if target == "win64" else "program")
                argv = [args.compiler.absolute(), "build", source, "--target", target,
                        "--opt", optimization, "--clang", args.clang.absolute(),
                        "-o", binary, "--emit-dir", directory / "phases", "--json"]
                if target == "win64":
                    # Do not resolve the lld-link alias to the lld personality.
                    argv += ["--linker", args.linker.absolute()]
                    for library in libraries:
                        argv += ["--windows-lib", library]
                report = json.loads(run(argv, name + "-build"))
                check(name + ": full independent Verify", report["status"] == "Success"
                      and report["verification_runs"] == 1, report["status"])
                check(name + ": real native tool invocations", len(report["commands"]) ==
                      (2 if target == "win64" else 1), report["commands"])
                data = binary.read_bytes()
                check(name + ": executable format", data.startswith(b"MZ" if target == "win64" else b"\x7fELF"))
                llvm_path = Path(report["artifacts"]["llvm"])
                llvm = llvm_path.read_text()
                triple = "x86_64-pc-windows-msvc" if target == "win64" else "x86_64-unknown-linux-gnu"
                check(name + ": exact target", triple in llvm)
                check(name + ": inline owning head and borrowed window",
                      "{ ptr, i64, i64 }" in llvm and "{ ptr, i64 }" in llvm)
                check(name + ": no unproved noalias", " noalias " not in llvm)
                symbols = ("GetProcessHeap", "HeapAlloc", "HeapReAlloc", "HeapFree") if target == "win64" else ("realloc", "free")
                check(name + ": explicit platform allocator", all("@" + symbol + "(" in llvm for symbol in symbols))
                output = run([args.wine.absolute(), binary] if target == "win64" else [binary], name + "-run")
                check(name + ": program semantic checks", output == b"")
                binaries.append({"name": name, "sha256": sha(binary), "bytes": len(data),
                                 "llvm_sha256": sha(llvm_path),
                                 "execution": "Wine compatibility" if target == "win64" else "Linux native"})
    except Exception as caught:
        error = repr(caught)
    files = [{"path": str(path.relative_to(out)), "bytes": path.stat().st_size,
              "sha256": sha(path)} for path in sorted(out.rglob("*")) if path.is_file()]
    report = {"format": "zil-byte-buffer-abi-replay/1", "status": "Pass" if error is None else "Fail",
              "scope": "Explicit u8 storage; growth, initialized prefix, generic mutable window, create/give parameter and return, and checksum; no native Windows or timing claim",
              "tools": tools, "inputs": inputs,
              "sources": {str(path.relative_to(root)): sha(path) for path in sorted((root / "examples/mainline/byte_buffer").glob("*.zil"))},
              "environment": {name: os.environ.get(name) for name in ("WINEPREFIX", "WINEARCH", "WINEDEBUG")},
              "commands": commands, "checks": checks, "assertions": len(checks),
              "binaries": binaries, "executions": len(binaries), "files": files, "error": error}
    (out / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "assertions": len(checks),
                      "executions": len(binaries), "error": error}))
    return int(error is not None)


if __name__ == "__main__":
    sys.exit(main())
