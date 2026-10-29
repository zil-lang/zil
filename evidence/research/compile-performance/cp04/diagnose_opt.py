#!/usr/bin/env python3
"""Independent LLVM text diagnostic; never a ZIL production/performance test."""

import collections
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import traceback


ROOT = Path(__file__).resolve().parent
WORK = ROOT.parent
WRAPPER = WORK / "run-tool.py"
OPT = ROOT / "tools/opt"
LIBRARY = Path(
    "/workspace/scratch/3cf12ce32e36/toolchains/llvm17/usr/lib/x86_64-linux-gnu/libLLVM-17.so"
).resolve()
INPUTS = ["tree1024x8", "retained"]
PIPELINES = {
    "baseline": "default<O0>",
    "candidate": "default<O0>,function(mem2reg)",
}


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


class IRCounts:
    """Count every one-line instruction in these generated ZIL modules.

    The input language has no invoke/switch or multiline instructions. Unknown
    lines are recorded as a diagnostic failure, not silently omitted.
    """

    OPCODES = set(
        "ret br switch indirectbr invoke callbr resume catchswitch catchret "
        "cleanupret unreachable fneg add fadd sub fsub mul fmul udiv sdiv fdiv "
        "urem srem frem shl lshr ashr and or xor extractelement insertelement "
        "shufflevector extractvalue insertvalue alloca load store fence cmpxchg "
        "atomicrmw getelementptr trunc zext sext fptrunc fpext fptoui fptosi "
        "uitofp sitofp ptrtoint inttoptr bitcast addrspacecast icmp fcmp phi "
        "select freeze call va_arg landingpad catchpad cleanuppad".split()
    )

    def __init__(self):
        self.counts = collections.Counter()
        self.active = False
        self.function = None
        self.remaining_allocas = []
        self.unknown = []

    def feed(self, raw):
        line = raw.decode("utf-8").strip()
        if line.startswith("define "):
            self.active = True
            self.function = line
            self.counts["functions"] += 1
            return
        if line == "}":
            self.active = False
            return
        if not self.active or not line or line.startswith(";"):
            return
        if re.match(r'^(?:[-a-zA-Z$._0-9]+|"[^"]+"):', line):
            self.counts["blocks"] += 1
            return
        text = line
        if text.startswith("%"):
            text = text.split(" = ", 1)[1]
        words = text.split()
        opcode = words[0]
        if opcode in ("tail", "musttail", "notail"):
            opcode = words[1]
        if opcode not in self.OPCODES:
            self.unknown.append(line)
            return
        self.counts["instructions"] += 1
        self.counts[opcode] += 1
        if opcode == "alloca" and len(self.remaining_allocas) < 40:
            self.remaining_allocas.append({"function": self.function, "instruction": line})
        if opcode == "call":
            if "@llvm.trap(" in line:
                self.counts["llvm_trap_calls"] += 1
            if ".with.overflow." in line:
                self.counts["overflow_calls"] += 1

    def result(self):
        return {
            "counts": dict(sorted(self.counts.items())),
            "remaining_alloca_examples_first_40": self.remaining_allocas,
            "unknown_instruction_lines": self.unknown,
        }


def capture(command, prefix):
    result = subprocess.run(command, capture_output=True)
    stdout = ROOT / (prefix + ".stdout")
    stderr = ROOT / (prefix + ".stderr")
    stdout.write_bytes(result.stdout)
    stderr.write_bytes(result.stderr)
    return {
        "command": command,
        "exit_code": result.returncode,
        "stdout_path": str(stdout),
        "stderr_path": str(stderr),
        "stdout_sha256": sha256(stdout),
        "stderr_sha256": sha256(stderr),
    }


def main():
    # Keep failures and previous evidence immutable by requiring a fresh run.
    output = ROOT / "opt-diagnostic.json"
    if output.exists():
        raise RuntimeError("opt-diagnostic.json already exists; use a new review run")
    receipt = {
        "classification": "diagnostic LLVM text pass experiment, not production Builder or performance evidence",
        "script_sha256": sha256(Path(__file__)),
        "wrapper_path": str(WRAPPER),
        "wrapper_sha256": sha256(WRAPPER),
        "opt_path": str(OPT),
        "opt_sha256": sha256(OPT),
        "llvm_library_path": str(LIBRARY),
        "llvm_library_sha256": sha256(LIBRARY),
        "post_ir_policy": "stdout hashed and counted in memory as a stream; full post IR not saved",
        "pipelines": PIPELINES,
        "runs": [],
        "status": "started",
    }
    wrapper = [sys.executable, str(WRAPPER)]
    try:
        receipt["version"] = capture(wrapper + [str(OPT), "--version"], "opt-version")
        receipt["dynamic_libraries"] = capture(wrapper + ["ldd", str(OPT)], "opt-ldd")
        if receipt["version"]["exit_code"] != 0 or receipt["dynamic_libraries"]["exit_code"] != 0:
            raise RuntimeError("tool version or dynamic library check failed")
        for case in INPUTS:
            source = WORK / "runs/cp02-profile" / (case + "-build-program.ll")
            target_report = WORK / "runs/cp02-profile" / (case + "-build.json")
            target = json.loads(target_report.read_text())["native_codegen"]["object_reports"][0]
            input_counts = IRCounts()
            with source.open("rb") as handle:
                for line in handle:
                    input_counts.feed(line)
            for role, pipeline in PIPELINES.items():
                invocation_pipeline = "verify," + pipeline + ",verify"
                command = wrapper + [
                    str(OPT),
                    "--passes=" + invocation_pipeline,
                    "--verify-each",
                    "--mtriple=" + target["triple"],
                    "--mcpu=" + target["cpu"],
                    "--mattr=" + target["features"],
                    "--relocation-model=pic",
                    "--data-layout=" + target["data_layout"],
                    "-S", "-o", "-", str(source),
                ]
                stderr = ROOT / (case + "-" + role + ".stderr")
                run = {
                    "case": case,
                    "role": role,
                    "semantic_pipeline": pipeline,
                    "invocation_pipeline": invocation_pipeline,
                    "command": command,
                    "input_path": str(source),
                    "input_sha256": sha256(source),
                    "input_bytes": source.stat().st_size,
                    "input_statistics": input_counts.result(),
                    "production_target_report_path": str(target_report),
                    "production_target_report_sha256": sha256(target_report),
                    "target": {key: target[key] for key in (
                        "llvm_version", "triple", "cpu", "features", "data_layout", "relocation", "code_model"
                    )},
                    "stderr_path": str(stderr),
                }
                receipt["runs"].append(run)
                output.write_text(json.dumps(receipt, indent=2) + "\n")
                digest = hashlib.sha256()
                byte_count = 0
                counts = IRCounts()
                with stderr.open("wb") as errors:
                    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=errors)
                    for line in process.stdout:
                        digest.update(line)
                        byte_count += len(line)
                        counts.feed(line)
                    process.stdout.close()
                    run["exit_code"] = process.wait()
                run.update(
                    stdout_sha256=digest.hexdigest(),
                    stdout_bytes=byte_count,
                    output_statistics=counts.result(),
                    stderr_sha256=sha256(stderr),
                )
                run["verify"] = "pass" if run["exit_code"] == 0 else "failed"
                output.write_text(json.dumps(receipt, indent=2) + "\n")
                if run["exit_code"] != 0 or counts.unknown or input_counts.unknown:
                    raise RuntimeError("pass execution or instruction classification failed")
                print(json.dumps({
                    "case": case,
                    "role": role,
                    "exit_code": run["exit_code"],
                    "counts": counts.result()["counts"],
                    "stdout_sha256": run["stdout_sha256"],
                }), flush=True)
        receipt["status"] = "pass"
    except Exception as error:
        receipt.update(status="failed", error=repr(error), traceback=traceback.format_exc())
        raise
    finally:
        output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
