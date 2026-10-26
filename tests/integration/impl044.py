#!/usr/bin/env python3
"""Real stdin/stdout scalar C-ABI parity. Bounded integration, not a product gate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def expected(data):
    output = bytearray()
    for byte in data:
        if (byte < 32 and byte not in (9, 10, 13)) or byte > 126:
            return 2, bytes(output)
        output.append(byte - 32 if 97 <= byte <= 122 else byte)
    return (0 if data else 3), bytes(output)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--zil", type=Path, required=True)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    out = (args.out or Path(tempfile.mkdtemp(prefix="zil-044-"))).resolve()
    out.mkdir(parents=True, exist_ok=True)
    commands = []

    def command(argv, **kwargs):
        argv = [str(arg) for arg in argv]
        commands.append(argv)
        result = subprocess.run(argv, capture_output=True, timeout=60, **kwargs)
        if result.returncode:
            raise RuntimeError(f"{argv}: exit {result.returncode}\n{result.stderr.decode(errors='replace')}\n{result.stdout.decode(errors='replace')}")
        return result

    source = ROOT / "examples/impl044/main.zil"
    build = command([args.zil.resolve(), "build", source, "-o", out / "zil-o0", "--emit-dir", out / "phases", "--json"])
    report = json.loads(build.stdout)
    assert report["status"] == "Success" and report["verification_runs"] == 1, report
    llvm_path = Path(report["artifacts"]["llvm"])
    llvm = llvm_path.read_text()
    for text in ("declare i32 @getchar()", "declare i32 @putchar(i32)", "call i32 @getchar()", "call i32 @putchar("):
        assert text in llvm, text
    assert "alloca" in llvm
    for forbidden in ("@malloc", "@calloc", "@realloc", "@zil.runtime", "@printf", "@puts"):
        assert forbidden not in llvm, forbidden
    clang = os.environ.get("ZIL_CLANG", "clang")
    command([clang, "-x", "ir", "-O2", llvm_path, "-o", out / "zil-o2"])
    for level in (0, 2):
        command(["rustc", "--edition=2024", f"-Copt-level={level}", ROOT / "examples/impl044/main.rs", "-o", out / f"rust-o{level}"])

    rng = random.Random(44044)
    alphabet = list(range(32, 127)) + [9, 10, 13]
    cases = {
        "mixed_ascii": b"Hello Zil 044!\nabc xyz\tRust\r\n",
        "empty": b"",
        "invalid_first": b"\x00ignored",
        "invalid_after_prefix": b"abc Zil\n\x00ignored",
        "non_ascii": b"ok\xfftail",
        "all_valid_ascii": bytes(alphabet),
        "dynamic_65536": bytes(rng.choice(alphabet) for _ in range(65536)),
    }
    rows = []
    programs = [out / f"{language}-o{level}" for level in (0, 2) for language in ("zil", "rust")]
    for name, data in cases.items():
        code, output = expected(data)
        for program in programs:
            observed = subprocess.run([program], input=data, capture_output=True, timeout=30)
            assert (observed.returncode, observed.stdout, observed.stderr) == (code, output, b""), (name, program, observed)
            rows.append({"case": name, "program": program.name, "input_bytes": len(data), "input_sha256": digest(data),
                         "exit": observed.returncode, "stdout_bytes": len(observed.stdout), "stdout_sha256": digest(observed.stdout), "stderr_bytes": 0, "status": "Pass"})
    # Large output forces libc to observe the /dev/full error during putchar,
    # rather than a buffered flush after the program has already returned.
    for program in programs:
        with open("/dev/full", "wb", buffering=0) as sink:
            observed = subprocess.run([program], input=b"abc\n" * 20000, stdout=sink, stderr=subprocess.PIPE, timeout=30)
        assert (observed.returncode, observed.stderr) == (4, b""), (program, observed)
        rows.append({"case": "output_failure_dev_full", "program": program.name, "input_bytes": 80000, "exit": 4, "stderr_bytes": 0, "status": "Pass"})

    evidence = {
        "slice": "044", "status": "Pass", "scope": "Real ZIL source -> independent verification -> LLVM -> O0/O2 native stdin/stdout; paired Rust behavior only",
        "commands": commands, "cases": rows, "checks": len(rows), "llvm_sha256": digest(llvm.encode()),
        "source_sha256": {str(p.relative_to(ROOT)): digest(p.read_bytes()) for p in sorted((ROOT / "examples/impl044").glob("*.*")) if p.suffix in (".zil", ".rs")},
        "stages": {name: {"bytes": Path(path).stat().st_size, "sha256": digest(Path(path).read_bytes())} for name, path in report["artifacts"].items() if name != "executable"},
        "boundary": "Exact libc C int signatures are trusted; no pointer crossing, retention, callbacks, unwinding, resource ownership or asynchronous operation is supported.",
        "limits": ["getchar EOF does not distinguish input errors", "putchar catches only errors observed by its call; process-exit flush errors are outside this adapter", "No timing or peak-memory comparison; no product-gate or superiority claim"],
    }
    if args.evidence:
        args.evidence.parent.mkdir(parents=True, exist_ok=True)
        args.evidence.write_text(json.dumps(evidence, indent=2) + "\n")
        shutil.copyfile(llvm_path, args.evidence.parent / "uppercase.ll")
        (args.evidence.parent / "native-build.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": "Pass", "checks": len(rows), "evidence": str(args.evidence) if args.evidence else None}))


if __name__ == "__main__":
    main()
