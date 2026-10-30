#!/usr/bin/env python3
"""CP17: reproduce compile4's frozen research matrix on a fresh Linux runner.

Standard library only. This orchestrates the existing runners; it does not
replace their estimators, change weights, or grant product acceptance.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
SCHEMA = "zil-compile4-ci/1"
CASES = {"tree4x2": .05, "tree32x8": .15, "tree256x8": .30, "tree1024x8": .30, "retained": .20}
REQUESTS = {"cold_process": .10, "unchanged": .35, "private_edit": .40, "public_edit": .15}
REQUIRED_TESTS = {"zil-types": {"project_lookup_cp10"}, "zil-backend-llvm": {"closed_executable_cp13"},
                  "zil-driver": {"borrowed_inference_cp15", "query_cache_cp12"}}


def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def save(path, value):
    temporary = path.with_name(path.name + ".writing")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()


def terminate_tree(pid):
    # The benchmark runners start compiler children in their own sessions.
    # Stop each parent before discovering children, then kill bottom-up.
    pending, stopped = [pid], []
    while pending:
        current = pending.pop()
        try:
            os.kill(current, signal.SIGSTOP)
        except ProcessLookupError:
            continue
        stopped.append(current)
        for status in Path("/proc").glob("[0-9]*/status"):
            try:
                parent = re.search(r"^PPid:\s+(\d+)$", status.read_text(), re.M)
                if parent and int(parent.group(1)) == current:
                    pending.append(int(status.parent.name))
            except (FileNotFoundError, ProcessLookupError):
                pass
    for current in reversed(stopped):
        try:
            os.kill(current, signal.SIGKILL)
        except ProcessLookupError:
            pass


def manifest(path):
    path = path.resolve(strict=True)
    relative = path.relative_to(ROOT)
    raw = path.read_bytes()
    committed = subprocess.check_output(["git", "-C", str(ROOT), "show", "HEAD:" + relative.as_posix()])
    if raw != committed or git("status", "--porcelain"):
        raise ValueError("The manifest and checkout must be committed and clean")
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise ValueError("The manifest must be a JSON object")
    if value.get("schema") != SCHEMA or not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_.-]{0,63}", value.get("run_id", "")):
        raise ValueError("Invalid manifest schema or run_id")
    base, candidate = value.get("base_revision", ""), git("rev-parse", "HEAD")
    if not re.fullmatch(r"[0-9a-f]{40}", base) or git("rev-parse", "--verify", base + "^{commit}") != base:
        raise ValueError("base_revision must be an existing full commit SHA")
    if value.get("candidate_revision") not in ("HEAD", candidate):
        raise ValueError("candidate_revision must resolve exactly to the current HEAD")
    if os.environ.get("GITHUB_SHA") and os.environ["GITHUB_SHA"] != candidate:
        raise ValueError("The checkout differs from the GitHub event SHA")
    if os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_REF_NAME") != "research/compile-performance":
        raise ValueError("This workflow only measures the research/compile-performance branch")
    subprocess.run(["git", "-C", str(ROOT), "merge-base", "--is-ancestor", base, candidate], check=True)
    if base == candidate or value.get("candidate_crates_tree") != git("rev-parse", "HEAD:crates"):
        raise ValueError("Candidate source tree differs from the explicit manifest, or baseline equals candidate")
    if (value.get("llvm_version") != "23.1.4" or value.get("rust_version") != "1.99.1"
            or (ROOT / "tools/llvm/version.txt").read_text().strip() != value["llvm_version"]
            or type(value.get("blocks")) is not int or value["blocks"] != 3
            or type(value.get("seed")) is not int):
        raise ValueError("CP17 requires LLVM 23.1.4, Rust 1.99.1 and three paired blocks")
    selected = {name: set() for name in REQUIRED_TESTS}
    types_lib = False
    tests = value.get("tests")
    if not isinstance(tests, list) or not tests:
        raise ValueError("Explicit targeted test groups are required")
    for group in tests:
        if not isinstance(group, dict):
            raise ValueError("Each targeted test group must be an object")
        package = group.get("package")
        targets, features = group.get("targets"), group.get("features")
        if (package not in selected or type(group.get("lib")) is not bool or not isinstance(targets, list)
                or not isinstance(features, list) or any(feature != "llvm-api" for feature in features)
                or (package != "zil-types" and "llvm-api" not in features)):
            raise ValueError("Invalid targeted test group or missing LLVM API feature")
        if not targets and not group["lib"]:
            raise ValueError("An empty test selection cannot validate this candidate")
        for target in targets:
            if not isinstance(target, str) or not re.fullmatch(r"[a-zA-Z0-9_]+", target):
                raise ValueError("Invalid test target name")
            if not (ROOT / "crates" / package / "tests" / (target + ".rs")).is_file():
                raise ValueError("Missing targeted test: " + package + "/" + target)
        selected[package].update(targets)
        types_lib |= package == "zil-types" and group["lib"]
    if not types_lib or any(not required <= selected[package] for package, required in REQUIRED_TESTS.items()):
        raise ValueError("The manifest omits required CP14/15 or cache controls")
    return value, {"manifest": str(relative), "manifest_sha256": sha(path), "base_revision": base,
                   "candidate_revision": candidate, "candidate_crates_tree": value["candidate_crates_tree"],
                   "base_crates_tree": git("rev-parse", base + ":crates"), "runner_sha256": sha(__file__)}


class Run:
    def __init__(self, args, spec, identity):
        self.args, self.spec, self.identity = args, spec, identity
        self.out, self.work = args.out.resolve(), args.work.resolve()
        if self.out == self.work or self.out in self.work.parents or self.work in self.out.parents:
            raise ValueError("Evidence and disposable work directories must be separate")
        self.out.mkdir(parents=True, exist_ok=False)
        self.work.mkdir(parents=True, exist_ok=False)
        (self.out / "logs").mkdir()
        (self.out / "binaries").mkdir()
        self.env = os.environ.copy()
        prefix = args.llvm_prefix.resolve(strict=True)
        self.clang, self.llvm, self.lld = prefix / "bin/clang", prefix / "lib/libLLVM-23.so", prefix / "bin/ld.lld"
        self.env.update(RUSTUP_TOOLCHAIN=spec["rust_version"], CARGO_INCREMENTAL="0",
                        ZIL_CLANG=str(self.clang), ZIL_LLVM_LIB_DIR=str(self.llvm.parent),
                        LD_LIBRARY_PATH=str(self.llvm.parent), PATH=str(prefix / "bin") + os.pathsep + self.env.get("PATH", ""))
        self.deadline, self.index, self.windows = time.monotonic() + args.budget_seconds, 0, []
        self.result = {"schema": SCHEMA, "run_id": spec["run_id"], **identity, "status": "InProgress_NoScore",
                       "original_evidence_root": str(self.out), "budget_seconds": args.budget_seconds,
                       "windows": self.windows, "product_gates": "NotRun", "independent_review": "NotRun",
                       "performance_adoption": "NotEvaluated", "started_unix": time.time()}
        save(self.out / "result.json", self.result)
        save(self.out / "manifest.json", spec)

    def command(self, label, argv, cwd=ROOT, extra_env=None):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("The research time budget expired; remaining windows are not scored")
        self.index += 1
        stem = f"{self.index:03d}-{label}"
        stdout, stderr = self.out / "logs" / (stem + ".stdout"), self.out / "logs" / (stem + ".stderr")
        command = [str(arg) for arg in argv]
        record = {"label": label, "argv": command, "cwd": str(cwd), "started_unix": time.time(),
                  "extra_environment": extra_env or {}, "stdout": str(stdout.relative_to(self.out)),
                  "stderr": str(stderr.relative_to(self.out)), "status": "Running"}
        receipt = self.out / "logs" / (stem + ".json")
        save(receipt, record)
        print(label, "start", flush=True)
        start = time.monotonic()
        with stdout.open("wb") as out, stderr.open("wb") as err:
            child = subprocess.Popen(command, cwd=cwd, env={**self.env, **(extra_env or {})},
                                     stdout=out, stderr=err, start_new_session=True)
            try:
                child.wait(timeout=remaining)
                record["timeout"] = False
            except subprocess.TimeoutExpired:
                terminate_tree(child.pid)
                child.wait()
                record["timeout"] = True
        record.update(exit=child.returncode, seconds=time.monotonic() - start,
                      stdout_sha256=sha(stdout), stderr_sha256=sha(stderr),
                      status="Pass" if child.returncode == 0 and not record["timeout"] else "Fail")
        save(receipt, record)
        print(label, record["status"], round(record["seconds"], 3), "seconds", flush=True)
        if record["status"] != "Pass":
            raise RuntimeError(label + " failed; retained logs: " + str(receipt))
        return stdout

    def research(self, runner, label, action, *argv):
        return self.command(label + "-" + action, [sys.executable, HERE / runner, action, *argv])

    def check_plan(self, path, requests=None):
        plan = json.loads(path.read_text())
        observed = {case["id"]: case["weight"] for case in plan["cases"]}
        if observed != CASES or len(plan["cases"]) != len(CASES) or plan["blocks"] != 3:
            raise ValueError("Prepared plan changed the five-case/weight/block contract")
        if requests is not None and plan.get("request_weights") != requests:
            raise ValueError("Prepared plan changed the request weights")
        return plan

    @staticmethod
    def passing(path):
        receipt = json.loads(path.read_text())
        if receipt.get("pass") is not True or receipt.get("failures"):
            raise ValueError("Preflight/build receipt is incomplete or failed: " + str(path))

    def summary(self, name, path):
        value = json.loads(path.read_text())
        if value.get("verdict") not in ("ExploratoryComplete_NotAProductGate", "ExploratoryRuntimeQuality_NotAProductGate"):
            raise ValueError("Incomplete or unknown summary: " + str(path))
        if value.get("failures") or value.get("coverage_complete") is not True or value.get("product_gates") != "NotRun":
            raise ValueError("Summary does not establish a complete exploratory window")
        self.windows.append({"name": name, "summary": str(path.relative_to(self.out)), "sha256": sha(path),
                             "verdict": value["verdict"]})
        save(self.out / "result.json", self.result)

    def release_target(self, path):
        if path.parent != self.work or path.is_symlink() or path.resolve() != path:
            raise ValueError("Refusing to remove a compiler cache outside this run's work directory")
        shutil.rmtree(path)

    def build_compilers(self):
        self.command("toolchain-check", [sys.executable, ROOT / "tools/llvm/check_toolchain.py"])
        self.rustc = Path(self.command("rustc-path", ["rustup", "which", "--toolchain", "1.99.1", "rustc"]).read_text().strip())
        cargo = Path(self.command("cargo-path", ["rustup", "which", "--toolchain", "1.99.1", "cargo"]).read_text().strip())
        self.command("rustc-version", [self.rustc, "-Vv"])
        self.command("source-tree", ["git", "ls-tree", "-r", "HEAD"])
        self.command("formatting", [cargo, "fmt", "--all", "--", "--check"])
        for name in ("test_bench.py", "test_rust_bench.py", "test_runtime_bench.py"):
            self.command(name.removesuffix(".py"), [sys.executable, HERE / name])
        baseline = self.work / "baseline-source"
        self.command("baseline-worktree", ["git", "worktree", "add", "--detach", baseline, self.identity["base_revision"]])
        self.binaries = {}
        builds = {}
        for label, source, revision in (("baseline", baseline, self.identity["base_revision"]),
                                        ("candidate", ROOT, self.identity["candidate_revision"])):
            target = self.work / ("target-" + label)
            env = {"CARGO_TARGET_DIR": str(target)}
            if label == "candidate":
                self.command("candidate-check", [cargo, "check", "--locked", "--workspace"], extra_env=env)
                for index, group in enumerate(self.spec["tests"]):
                    argv = [cargo, "test", "--release", "--locked", "-p", group["package"]]
                    if group["lib"]:
                        argv.append("--lib")
                    for test in group["targets"]:
                        argv.extend(("--test", test))
                    if group["features"]:
                        argv.extend(("--features", ",".join(group["features"])))
                    log = self.command(f"candidate-tests-{index}", argv, extra_env=env)
                    counts = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed;", log.read_text())
                    if (len(counts) != len(group["targets"]) + int(group["lib"])
                            or any(int(passed) == 0 or int(failed) for passed, failed in counts)):
                        raise ValueError("Every selected test target must report successful nonempty execution: " + str(log))
            self.command(label + "-build", [cargo, "build", "--release", "--locked", "-p", "zil-cli"], cwd=source, extra_env=env)
            binary = self.out / "binaries" / ("zil-" + label)
            shutil.copy2(target / "release/zil", binary)
            self.binaries[label] = binary
            builds[label] = {"source_commit": revision, "path": str(binary), "sha256": sha(binary), "bytes": binary.stat().st_size}
            save(self.out / "compiler-builds.json", builds)
            self.release_target(target)
        if git("rev-parse", "HEAD") != self.identity["candidate_revision"] or git("status", "--porcelain"):
            raise ValueError("The candidate checkout changed during build/tests")

    def matrix(self):
        windows = self.out / "windows"
        windows.mkdir()
        cpus = sorted(os.sched_getaffinity(0))[:4]
        if not cpus:
            raise ValueError("No available CPU affinity")
        affinity = ",".join(map(str, cpus))
        shared = ["--clang", self.clang, "--tool", self.llvm, "--tool", self.lld, "--blocks", "3", "--seed", str(self.spec["seed"])]
        sources = ["--baseline", self.binaries["baseline"], "--candidate", self.binaries["candidate"],
                   "--baseline-source", self.identity["base_revision"], "--candidate-source", self.identity["candidate_revision"]]
        bridge = windows / "o0-bridge"
        bridge.mkdir()
        self.research("bench.py", "bridge", "prepare", "--out", bridge / "plan", *sources, *shared,
                      "--profile", "O0", "--phase", "build", "--units", "8", "--jobs", "4", "--object-cache", "--cpus", affinity)
        bridge_plan = bridge / "plan/plan.json"
        self.check_plan(bridge_plan, REQUESTS)
        self.research("bench.py", "bridge", "preflight", "--plan", bridge_plan, "--out", bridge / "preflight")
        self.passing(bridge / "preflight/result.json")
        self.research("bench.py", "bridge", "measure", "--plan", bridge_plan, "--preflight", bridge / "preflight/result.json",
                      "--out", bridge / "build", "--quiet-window")
        for line in (bridge / "build/records.jsonl").read_text().splitlines():
            row = json.loads(line)
            report = json.loads((bridge / "build/logs" / row["stdout"]).read_text())
            if (report.get("native_codegen", {}).get("backend") != "llvm-api" or report.get("timings") is not None
                    or (report.get("query_cache") or {}).get("requested")):
                raise ValueError("Bridge changed the LLVM API / no-timings / no-query contract")
        self.research("bench.py", "bridge", "summarize", "--plan", bridge_plan, "--records", bridge / "build/records.jsonl",
                      "--out", bridge / "summary.json")
        self.summary("o0-bridge-source-object-no-query", bridge / "summary.json")
        for label, profile, cache, flags in (("rust-o0-cold", "O0", "cold", []),
                                            ("rust-o0-incremental", "O0", "incremental", ["--query-cache"]),
                                            ("rust-o2-closed", "O2", "cold", ["--closed-executable"])):
            folder = windows / label
            folder.mkdir()
            self.research("rust_bench.py", label, "prepare", "--out", folder / "plan", *shared,
                          "--zil", self.binaries["candidate"], "--zil-source", self.identity["candidate_revision"], "--rustc", self.rustc,
                          "--profile", profile, "--cache-mode", cache, "--rust-profile", "native", "--rust-lto", "native",
                          "--units", "8", "--jobs", "4", "--cpus", affinity, *flags)
            plan, preflight = folder / "plan/plan.json", folder / "preflight/result.json"
            self.check_plan(plan, REQUESTS if cache == "incremental" else {"cold_process": 1.})
            self.research("rust_bench.py", label, "preflight", "--plan", plan, "--out", folder / "preflight")
            self.passing(preflight)
            for kind, action in (("build", "measure"), ("runtime", "measure-runtime")):
                self.research("rust_bench.py", label, action, "--plan", plan, "--preflight", preflight,
                              "--out", folder / kind, "--quiet-window")
                summary = folder / (kind + "-summary.json")
                self.research("rust_bench.py", label + "-" + kind, "summarize", "--plan", plan, "--preflight", preflight,
                              "--records", folder / kind / "records.jsonl", "--kind", kind, "--out", summary)
                self.summary(label + "-" + kind, summary)
        quality = windows / "o2-direct-quality"
        quality.mkdir()
        self.research("runtime_bench.py", "quality", "prepare", "--out", quality / "plan", "--corpus-plan", bridge_plan,
                      *sources, *shared, "--profile", "O2", "--baseline-units", "1", "--baseline-jobs", "1",
                      "--candidate-units", "8", "--candidate-jobs", "4", "--baseline-closed-executable",
                      "--candidate-closed-executable", "--cpu", str(cpus[0]))
        plan, built = quality / "plan/plan.json", quality / "build/result.json"
        self.check_plan(plan)
        self.research("runtime_bench.py", "quality", "build", "--plan", plan, "--out", quality / "build")
        self.passing(built)
        self.research("runtime_bench.py", "quality", "measure", "--plan", plan, "--build-receipt", built,
                      "--out", quality / "runtime", "--quiet-window")
        self.research("runtime_bench.py", "quality", "summarize", "--plan", plan, "--build-receipt", built,
                      "--records", quality / "runtime/records.jsonl", "--out", quality / "summary.json")
        self.summary("baseline-closed-o2-1x1-vs-candidate-closed-o2-8x4", quality / "summary.json")

    def finish(self, error=None):
        self.result.update(finished_unix=time.time(), status="IncompleteOrFailed_NoScore" if error else "Complete_Exploratory_NotAProductGate")
        if error:
            self.result["error"] = repr(error)
        self.result["scope"] = ("Five cases, original weights, three paired blocks; source/object/no-query ZIL bridge differs from query-enabled Rust incremental comparison. "
                                "All compiler builds/tests precede serial measurement. Direct O2 runtime compares baseline closed 1/1 and candidate closed 8/4. "
                                "No pooled score, no claim of physical host exclusivity, no automatic performance adoption.")
        save(self.out / "result.json", self.result)
        files = {str(path.relative_to(self.out)): {"bytes": path.stat().st_size, "sha256": sha(path)}
                 for path in sorted(self.out.rglob("*")) if path.is_file() and path.name != "files.json"}
        save(self.out / "files.json", files)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=HERE / "compile4-run.json")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--llvm-prefix", type=Path)
    parser.add_argument("--budget-seconds", type=int, default=3600)
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    run = None
    try:
        spec, identity = manifest(args.manifest)
        if args.validate_only:
            print(json.dumps({"status": "ManifestValidated_NotMeasured", **identity}, indent=2))
            return 0
        if not all((args.out, args.work, args.llvm_prefix)) or args.budget_seconds <= 0:
            parser.error("--out, --work, --llvm-prefix and a positive budget are required")
        run = Run(args, spec, identity)
        run.build_compilers()
        run.matrix()
        run.finish()
        return 0
    except (OSError, ValueError, RuntimeError, KeyError, TypeError, TimeoutError, subprocess.SubprocessError) as error:
        if run is not None:
            run.finish(error)
        print("CP17 failed without an overall score:", repr(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
