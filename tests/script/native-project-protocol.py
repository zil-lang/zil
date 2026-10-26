#!/usr/bin/env python3
"""Actual four-module Native entry, synchronous error and cleanup protocol."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
if not TARGET.is_absolute():
    TARGET = ROOT / TARGET
ZIL = Path(os.environ.get("ZIL_BIN", TARGET / "debug/zil")).resolve()
DEST = Path(os.environ.get("ZIL_PROTOCOL_REPORT_DIR", ROOT / "out/runs/106"))
DEST.mkdir(parents=True, exist_ok=True)
OUT = Path(tempfile.mkdtemp(prefix="native-project-", dir=DEST))
results = []
assertions = 0


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def expect(condition, message):
    global assertions
    assertions += 1
    if not condition:
        raise AssertionError(message)


def execute(label, argv, data=b"", json_output=False):
    directory = OUT / label
    directory.mkdir(parents=True)
    command = list(map(str, argv))
    (directory / "stdin").write_bytes(data)
    record = {"label": label, "argv": command, "stdin_sha256": sha(directory / "stdin")}
    results.append(record)
    try:
        p = subprocess.run(command, input=data, capture_output=True, cwd=ROOT, timeout=90)
        (directory / "stdout").write_bytes(p.stdout)
        (directory / "stderr").write_bytes(p.stderr)
        record.update(exit=p.returncode, stdout_sha256=sha(directory / "stdout"),
                      stderr_sha256=sha(directory / "stderr"))
        (directory / "process.json").write_text(json.dumps(record, indent=2) + "\n")
        if json_output:
            return p, json.loads(p.stdout)
        return p
    except Exception as error:
        record["exception"] = str(error)
        (directory / "process.json").write_text(json.dumps(record, indent=2) + "\n")
        raise


def compiler(label, action, source, *extra, data=b""):
    return execute(label, [ZIL, action, source, *extra, "--json"], data, True)


def success(report):
    expect(report["status"] == "Success", "source pipeline did not succeed")
    expect(report["verification_runs"] == 1, "independent Verify must execute")


def run_pair(label, source, binary, data, output, code, opt):
    direct = execute(label + "-direct", [binary], data)
    p, wrapped = compiler(label + "-wrapped", "run", source, "--opt", opt,
                          "--output", OUT / (label + "-wrapped-program"), data=data)
    success(wrapped["compile"])
    expect(direct.returncode == p.returncode == wrapped["run"]["exit_code"] == code,
           "direct/run process exits differ from registered mapping")
    expect(direct.stdout == output == wrapped["run"]["stdout"].encode(),
           "evaluation/cleanup/import execution trace differs")
    expect(direct.stderr == b"" and wrapped["run"]["stderr"] == "" and p.stderr == b"",
           "entry added diagnostics or implicit error printing")
    expect(wrapped["run"]["signal"] is None, "a signal is not an error-union exit")


def main():
    source_dir = OUT / "source"
    shutil.copytree(ROOT / "tests/script/native-project", source_dir)
    source = source_dir / "main.zil"
    original = source.read_text()
    # This exact ordering comes from lexical ownership and explicit calls:
    # root Z, outer A, inner B; B exits before A, then root Z.
    cases = [("normal", b"a", b"S12B3AEZ", 7),
             ("error", b"e", b"S12BAZ", 1),
             ("zero", b"\0", b"S12B3AEZ", 7),
             ("nonascii", b"\xff", b"S12B3AEZ", 7),
             ("eof", b"", b"S12B3AEZ", 7)]
    for opt in ("O0", "O2"):
        binary = OUT / ("program-" + opt)
        p, report = compiler("build-" + opt, "build", source, "--opt", opt,
                             "--output", binary, "--emit-dir", OUT / ("phases-" + opt))
        expect(p.returncode == 0, "native build failed")
        success(report)
        expect(len(report["source_files"]) == 4, "four actual modules must be discovered")
        for label, data, output, code in cases:
            run_pair(label + "-" + opt, source, binary, data, output, code, opt)
        p, report = compiler("wide-" + opt, "run", source, "--opt", opt, "--entry", "wide",
                             "--output", OUT / ("wide-program-" + opt))
        success(report["compile"])
        expect(p.returncode == 0 and report["run"]["exit_code"] == 0 and
               report["run"]["stdout"] == "1099511628800\n", "full-width explicit entry lost value")
        expect(report["run"]["stderr"] == "", "wide probe must not execute main or imported main")

        void_dir = OUT / ("void-" + opt)
        shutil.copytree(source_dir, void_dir)
        void = void_dir / "main.zil"
        void.write_text("import lifetime\nimport io\nfn main() {\n"
                        "let root = lifetime.token(86)\nio.emit(71)\n}\n")
        binary = OUT / ("void-program-" + opt)
        p, report = compiler("void-build-" + opt, "build", void, "--opt", opt, "-o", binary)
        expect(p.returncode == 0, "void native build failed")
        success(report)
        run_pair("void-" + opt, void, binary, b"", b"GV", 0, opt)

        error_void_dir = OUT / ("error-void-" + opt)
        shutil.copytree(source_dir, error_void_dir)
        error_void = error_void_dir / "main.zil"
        error_void.write_text(original.replace("app.Fault!i32", "app.Fault!void")
                              .replace("return result", "return"))
        binary = OUT / ("error-void-program-" + opt)
        p, report = compiler("error-void-build-" + opt, "build", error_void,
                             "--opt", opt, "-o", binary)
        expect(p.returncode == 0, "fallible void native build failed")
        success(report)
        for label, data, output, code in cases[:2]:
            run_pair("error-void-" + label + "-" + opt, error_void, binary, data, output,
                     0 if code == 7 else 1, opt)

    # Nearest invalid entry forms. Exact phase/code expectations are registered.
    negatives = [
        ("missing-main", "import app\nfn value()->i32{return 0}\n",
         [], 1, "SafeReject", "codegen", "E-CODEGEN-ENTRY"),
        ("duplicate-main", "fn main()->i32{return 0}\nfn main()->i32{return 1}\n",
         [], 1, "SafeReject", "expand", "E-NAME-DUPLICATE"),
        ("bad-signature", "fn main()->bool{return true}\n",
         [], 1, "SafeReject", "types", "E-ENTRY-SIGNATURE"),
        ("top-level", "import io\nio.emit(88)\nfn main()->i32{return 0}\n",
         [], 1, "SafeReject", "parse", "E-PARSE-MODULE-STATEMENT"),
        ("missing-entry", original, ["--entry", "absent"], 1, "SafeReject", "driver", "E-ENTRY-NOT-FOUND"),
        ("managed", original, ["--profile", "managed"], 3, "Unsupported", "driver", "E-UNSUPPORTED"),
        ("cancelled", original, ["--compile-timeout-ms", "0"], 5, "Cancelled", "driver", "E-CANCELLED"),
        ("missing-tool", original, ["--clang", str(OUT / "not-a-tool")], 2,
         "InfraError", "link", "E-LINK-IO")]
    for name, text, extra, code, category, phase, diagnostic in negatives:
        folder = OUT / ("source-" + name)
        shutil.copytree(source_dir, folder)
        candidate = folder / "main.zil"
        candidate.write_text(text)
        output = OUT / ("negative-" + name)
        p, report = compiler("reject-" + name, "build", candidate, "-o", output, *extra)
        expect(p.returncode == code and report["status"] == category and report["phase"] == phase,
               f"{name}: wrong category/phase/exit: {report}")
        expect(report["diagnostics"][0]["code"] == diagnostic,
               f"{name}: wrong reason: {report['diagnostics']}")
        expect(not output.exists() and "executable" not in report["artifacts"],
               f"{name}: a failed request published executable")


status = "Pass"
error = None
try:
    main()
except Exception as exception:
    status = "Fail"
    error = f"{type(exception).__name__}: {exception}"
manifest = [{"path": str(p.relative_to(OUT)), "bytes": p.stat().st_size, "sha256": sha(p)}
            for p in sorted(OUT.rglob("*")) if p.is_file()]
summary = {"status": status, "executed": len(results), "assertions": assertions,
           "scope": "four-module Native run/build/explicit-entry, synchronous error and cleanup",
           "compiler": str(ZIL), "compiler_sha256": sha(ZIL), "source_baseline":
           "24ca9583d8789cfdfe3ccdbfe3eeff614bf56429", "results": results,
           "error": error, "files": manifest, "retained_directory": str(OUT)}
(OUT / "result.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary))
sys.exit(status != "Pass")
