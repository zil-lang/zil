#!/usr/bin/env python3
"""Compile and exercise the real independent boundary; never substitute IR
model tests for ZIL source/native execution evidence. Only JSON goes to stdout.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]

CASES = {
    "construct-module": ("E0451", """
fn counterfeit(module: zil_ir::TypedModule, plan: zil_ir::CandidatePlan) {
    let _ = zil_verify::VerifiedModule { module, plan, fingerprint: zil_base::Fingerprint::ZERO };
}
fn main() {}
"""),
    "construct-ct": ("E0451", """
fn counterfeit(module: zil_ir::CtModule) { let _ = zil_verify::VerifiedCTIR { module }; }
fn main() {}
"""),
    "mutate-module": ("E0596", """
fn mutate(value: &zil_verify::VerifiedModule) { value.module().functions.clear(); }
fn main() {}
"""),
    "mutate-ct": ("E0596", """
fn mutate(value: &zil_verify::VerifiedCTIR) { value.module().instructions.clear(); }
fn main() {}
"""),
    "deserialize-module-authority": ("E0277", """
fn counterfeit(bytes: &[u8], limits: &zil_base::Limits) -> zil_verify::VerifiedModule {
    let (module, _) = zil_ir::wire::decode_module_candidate(bytes, limits).unwrap();
    module.into()
}
fn main() {}
"""),
    "deserialize-ct-authority": ("E0277", """
fn counterfeit(bytes: &[u8], limits: &zil_base::Limits) -> zil_verify::VerifiedCTIR {
    zil_ir::wire::decode_ct(bytes, limits).unwrap().into()
}
fn main() {}
"""),
}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("ct", "isolation"))
    mode = parser.parse_args().mode
    records = []
    cargo = os.environ.get("CARGO") or shutil.which("cargo")
    rustc = os.environ.get("RUSTC") or shutil.which("rustc")
    if not cargo or not rustc:
        raise RuntimeError("cargo and rustc must identify the selected actual toolchain")
    with tempfile.TemporaryDirectory(prefix="zil-verify-boundary-") as directory:
        temporary = Path(directory)
        env = dict(os.environ, CARGO_TARGET_DIR=str(temporary / "target"))

        def run(command):
            process = subprocess.run([str(arg) for arg in command], cwd=ROOT, env=env,
                                     capture_output=True, text=True, timeout=90)
            records.append({"command": [str(arg) for arg in command], "exit_code": process.returncode,
                            "stdout": process.stdout, "stderr": process.stderr})
            return process

        build = run([cargo, "build", "--offline", "--locked", "-p", "zil-verify", "--message-format=json"])
        if build.returncode:
            raise RuntimeError("real verifier build failed: " + build.stderr)
        artifacts = {}
        for line in build.stdout.splitlines():
            item = json.loads(line)
            if item.get("reason") == "compiler-artifact":
                name = item["target"]["name"]
                files = [Path(path) for path in item["filenames"] if path.endswith(".rlib")]
                if files:
                    artifacts[name] = files[0]
        if set(artifacts) != {"zil_base", "zil_ir", "zil_verify"}:
            raise RuntimeError("unexpected real verifier dependency build: " + repr(sorted(artifacts)))
        flags = ["--edition=2024", "-L", "dependency=" + str(temporary / "target/debug/deps")]
        for name, path in sorted(artifacts.items()):
            flags += ["--extern", name + "=" + str(path)]
        binary = temporary / "boundary-harness"
        compiled = run([rustc, ROOT / "tests/verify/boundary_harness.rs", *flags, "-o", binary])
        if compiled.returncode:
            raise RuntimeError("positive boundary harness failed to compile: " + compiled.stderr)
        executed = run([binary, mode, temporary])
        if executed.returncode:
            raise RuntimeError("boundary assertions failed: " + executed.stderr)
        result = json.loads(executed.stdout)
        if result.get("status") != "Pass" or result.get("executed", 0) < 1 or result.get("assertions", 0) < 1:
            raise RuntimeError("boundary harness returned no successful assertions")
        assertions = result["assertions"] + 3
        count = result["executed"] + 2
        compile_rejections = []
        if mode == "isolation":
            for name, (diagnostic, source) in CASES.items():
                path = temporary / (name + ".rs")
                path.write_text(source, encoding="utf-8")
                rejected = run([rustc, path, *flags, "--crate-name", "boundary_attempt", "--error-format=json", "-o", temporary / name])
                errors = [json.loads(line) for line in rejected.stderr.splitlines() if line.startswith("{")]
                codes = [error.get("code", {}).get("code") for error in errors if isinstance(error.get("code"), dict)]
                if rejected.returncode == 0 or diagnostic not in codes:
                    raise RuntimeError(f"{name} did not fail for {diagnostic}: {rejected.stderr}")
                if (temporary / name).exists():
                    raise RuntimeError("negative boundary attempt produced an executable")
                assertions += 3; count += 1
                compile_rejections.append({"case": name, "expected": diagnostic, "actual_codes": codes,
                                           "source_sha256": hashlib.sha256(source.encode()).hexdigest()})
        output = {"status": "Pass", "mode": mode, "evidence_level": "RawCandidateBoundary/CompilerBoundary",
                  "executed": count, "assertions": assertions, "built_crates": sorted(artifacts),
                  "runtime": result, "compile_rejections": compile_rejections, "commands": records}
        print(json.dumps(output, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(json.dumps({"status": "Fail", "executed": 0, "assertions": 0, "reason": str(error)}))
        raise SystemExit(1)
