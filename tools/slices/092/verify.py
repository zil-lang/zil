#!/usr/bin/env python3
"""Bounded ordinary-slice ABI replay on Linux and Win64/Wine.

Wine runs are compatibility evidence, never native Windows/driver evidence.
This runner makes no timing claim and does not replace the independent kernel.
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
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--imports", type=Path, required=True)
    parser.add_argument("--wine", type=Path, required=True)
    parser.add_argument("--clang", type=Path, required=True)
    parser.add_argument("--linker", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
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
            completed = subprocess.run(argv, capture_output=True, timeout=45, cwd=root)
        except subprocess.TimeoutExpired as error:
            (out / f"{name}.stdout").write_bytes(error.stdout or b"")
            (out / f"{name}.stderr").write_bytes(error.stderr or b"")
            commands.append({"name": name, "argv": argv, "exit_code": None, "timeout": True})
            raise
        (out / f"{name}.stdout").write_bytes(completed.stdout)
        (out / f"{name}.stderr").write_bytes(completed.stderr)
        commands.append({"name": name, "argv": argv, "exit_code": completed.returncode})
        check(f"{name}: process exit", completed.returncode == expected, completed.returncode)
        return completed.stdout

    error = None
    compiler_hash = None
    try:
        for tool in [args.compiler, args.wine, args.clang, args.linker]:
            check(f"tool {tool.name}", tool.is_file())
        compiler_hash = sha(args.compiler)
        library = args.imports.absolute() / "kernel32.lib"
        check("explicit kernel32 import archive", library.is_file())
        source = root / "examples/mainline/slices/portable.zil"
        for target in ["native", "win64"]:
            for optimization in ["O0", "O2"]:
                name = f"{target}-{optimization}"
                directory = out / name
                directory.mkdir()
                binary = directory / ("program.exe" if target == "win64" else "program")
                argv = [args.compiler.absolute(), "build", source, "--target", target,
                        "--opt", optimization, "--clang", args.clang.absolute(),
                        "-o", binary, "--emit-dir", directory / "phases", "--json"]
                if target == "win64":
                    # Keep lld-link's invocation path; resolving its alias to
                    # lld would select a different command-line personality.
                    argv += ["--linker", args.linker.absolute(), "--windows-lib", library]
                report = json.loads(run(argv, f"{name}-build"))
                check(f"{name}: complete independent Verify", report["status"] == "Success"
                      and report["verification_runs"] == 1, report["status"])
                check(f"{name}: real tool invocations", len(report["commands"]) ==
                      (2 if target == "win64" else 1), report["commands"])
                data = binary.read_bytes()
                check(f"{name}: binary format", data.startswith(b"MZ" if target == "win64" else b"\x7fELF"))
                llvm_path = Path(report["artifacts"]["llvm"])
                llvm = llvm_path.read_text()
                triple = "x86_64-pc-windows-msvc" if target == "win64" else "x86_64-unknown-linux-gnu"
                check(f"{name}: target and actual window representation", triple in llvm and "{ ptr, i64 }" in llvm)
                check(f"{name}: no implicit allocator or noalias", "@malloc(" not in llvm and " noalias " not in llvm)
                output = run([args.wine.absolute(), binary] if target == "win64" else [binary], f"{name}-run")
                check(f"{name}: quiet successful semantic checks", output == b"")
                binaries.append({"name": name, "sha256": sha(binary), "bytes": len(data),
                                 "llvm_sha256": sha(llvm_path),
                                 "execution": "Wine compatibility" if target == "win64" else "Linux native"})
    except Exception as caught:
        error = repr(caught)
    files = []
    for path in sorted(out.rglob("*")):
        if path.is_file():
            files.append({"path": str(path.relative_to(out)), "bytes": path.stat().st_size,
                          "sha256": sha(path)})
    report = {"format": "zil-slice-abi-replay/1", "status": "Pass" if error is None else "Fail",
              "scope": "Ordinary internal slice ABI, six windows, returned mutable/shared views and zero-size/empty elements; no runtime performance or native Windows claim",
              "compiler": {"path": str(args.compiler.absolute()), "sha256": compiler_hash},
              "sources": {str(path.relative_to(root)): sha(path) for path in sorted((root / "examples/mainline/slices").glob("*.zil"))},
              "environment": {name: os.environ.get(name) for name in ("WINEPREFIX", "WINEARCH", "WINEDEBUG")},
              "commands": commands, "checks": checks, "assertions": len(checks),
              "binaries": binaries, "executions": len(binaries), "files": files, "error": error}
    (out / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "assertions": len(checks),
                      "executions": len(binaries), "error": error}))
    return int(error is not None)


if __name__ == "__main__":
    sys.exit(main())
