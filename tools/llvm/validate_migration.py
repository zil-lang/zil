#!/usr/bin/env python3
"""Real LLVM17/23 cache isolation and version-rejection controls (not a benchmark).

The source is the three-module native exit-42 case in object_cache212.rs. Source
commit arguments are declarations bound to Git trees, not proof of binary origin;
retain the corresponding build receipts separately. Every output directory must
be new. Failures, command logs and the successful prefix remain available.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys


SOURCE = {
    "main.zil": "import a\nimport b\nfn main()->i32 { return a.get()+b.get(); }\n",
    "a.zil": "fn hidden()->i32 { return 19; }\npub fn get()->i32 { return hidden(); }\n",
    "b.zil": "pub fn get()->i32 { return 23; }\n",
}
CONFIGS = [("O0", 4, 2), ("O2", 1, 1)]
VERSION_PROBE = """import ctypes,json,sys
library=ctypes.CDLL(sys.argv[1])
values=[ctypes.c_uint() for _ in range(3)]
library.LLVMGetVersion.argtypes=[ctypes.POINTER(ctypes.c_uint)]*3
library.LLVMGetVersion.restype=None
library.LLVMGetVersion(*(ctypes.byref(v) for v in values))
print(json.dumps({'version':'.'.join(str(v.value) for v in values)}))
"""


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def identity(path):
    path = Path(path).resolve(strict=True)
    require(path.is_file(), f"not a regular file: {path}")
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}


def save(path, data):
    Path(path).write_text(json.dumps(data, indent=2) + "\n")


def snapshot(root):
    return {str(p.relative_to(root)): digest(p) for p in sorted(root.rglob("*.zobj"))}


def environment(clang, library, extra):
    env = os.environ.copy()
    for name in ("LD_PRELOAD", "LD_AUDIT"):
        env.pop(name, None)
    dirs = [str(Path(library).parent), *[str(Path(p).resolve(strict=True)) for p in extra]]
    env["LD_LIBRARY_PATH"] = os.pathsep.join(dict.fromkeys(dirs))
    env["PATH"] = str(Path(clang).parent) + os.pathsep + env.get("PATH", "")
    env["ZIL_CLANG"] = clang
    env["ZIL_LLVM_LIB_DIR"] = str(Path(library).parent)
    return env


class Commands:
    def __init__(self, root, timeout):
        self.root, self.timeout, self.records = root, timeout, []
        root.mkdir()

    def run(self, name, argv, env, expected=0):
        stem = self.root / f"{len(self.records):03d}-{name}"
        record = {"name": name, "argv": [str(a) for a in argv], "cwd": str(self.root.parent),
                  "environment": {k: env.get(k) for k in
                                  ("PATH", "LD_LIBRARY_PATH", "LD_PRELOAD", "LD_AUDIT", "ZIL_CLANG", "ZIL_LLVM_LIB_DIR")},
                  "expected_exit": expected, "timeout_seconds": self.timeout,
                  "status": "Fail", "timed_out": False}
        stdout, stderr = stem.with_suffix(".stdout"), stem.with_suffix(".stderr")
        try:
            with stdout.open("xb") as out, stderr.open("xb") as err:
                process = subprocess.Popen(record["argv"], cwd=self.root.parent, env=env,
                                           stdin=subprocess.DEVNULL, stdout=out, stderr=err,
                                           start_new_session=True)
                try:
                    record["exit_code"] = process.wait(timeout=self.timeout)
                except subprocess.TimeoutExpired:
                    record["timed_out"] = True
                    os.killpg(process.pid, signal.SIGKILL)
                    record["exit_code"] = process.wait()
            require(not record["timed_out"], f"command timed out: {name}")
            require(record["exit_code"] == expected,
                    f"unexpected exit for {name}: {record['exit_code']} != {expected}")
            record["status"] = "Pass"
        except Exception as error:
            record["error"] = str(error)
            raise
        finally:
            record["stdout"] = identity(stdout) if stdout.exists() else None
            record["stderr"] = identity(stderr) if stderr.exists() else None
            self.records.append(record)
            save(stem.with_suffix(".json"), record)
        return stdout.read_bytes(), stderr.read_bytes()


def verify_build(report, version, opt, units, warm):
    require(report["status"] == "Success", "build did not report Success")
    require(report["verification_runs"] == 1, "current Verify did not run exactly once")
    require(len(report["commands"]) == 1, "cache hit must still link exactly once")
    native = report["native_codegen"]
    require(native["backend"] == "llvm-api" and native["fallback"] is None,
            "not a direct API build")
    actual = native["actual_units"]
    require(type(actual) is int and 0 < actual <= units, "invalid actual unit count")
    require(units == 1 or actual > 1, "multi-unit control did not partition")
    objects = native["object_reports"]
    require(len(objects) == actual, "missing object receipts")
    cache = native["object_cache"]
    require(cache["requested"] is True and cache["enabled"] is True,
            "requested cache was not enabled")
    expected = {"hits": actual if warm else 0, "misses": 0 if warm else actual,
                "stored": 0 if warm else actual, "corrupt": 0, "write_failures": 0}
    require(all(type(cache[k]) is int and cache[k] == v for k, v in expected.items()),
            f"unexpected cache counts: {cache}")
    for item in objects:
        require(item["llvm_version"] == version and item["backend"] == "llvm-api",
                "wrong LLVM object identity")
        require(item["triple"] == "x86_64-unknown-linux-gnu", "wrong target")
        require(item["parse_ns"] == 0, "production API path parsed text")
        require(item["cache"]["state"] == ("hit" if warm else "miss"), "wrong object cache state")
        require(item["input_kind"] == ("object-cache" if warm else "builder"), "wrong object producer")
        if warm:
            require(item["llvm_verification_this_request"] is False and
                    item["producer_verified_before_after"] is True, "misleading cache verification receipt")
        else:
            require(item["verify_before_after"] is True and item["pipeline"] == f"default<{opt}>",
                    "missing LLVM verification or changed pipeline")
    return actual


def execute(args, out, result):
    require(sys.platform == "linux", "controls require a Linux host")
    repo = Path(args.repo).resolve(strict=True)
    checker = Path(args.candidate_checker).resolve(strict=True)
    commands = Commands(out / "logs", args.timeout)
    result["commands"] = commands.records
    arms = {}
    frozen = {"runner": identity(__file__), "checker": identity(checker)}
    result["frozen"] = frozen
    result["configurations"] = [{"optimization": o, "units": u, "jobs": j} for o, u, j in CONFIGS]
    for label, version in (("baseline", "17.0.6"), ("candidate", "23.1.4")):
        cli = identity(getattr(args, label))
        clang = identity(getattr(args, label + "_clang"))
        llvm = identity(getattr(args, label + "_llvm"))
        source = getattr(args, label + "_source")
        require(re.fullmatch(r"[0-9a-f]{40}", source) is not None, "source identity must be a full Git SHA")
        env = environment(clang["path"], llvm["path"], getattr(args, label + "_library_dir"))
        git, _ = commands.run(label + "-source", ["git", "-C", repo, "rev-parse", source + "^{commit}"], env)
        require(git.decode().strip() == source, "source identity was not resolved exactly")
        tree, _ = commands.run(label + "-crates", ["git", "-C", repo, "rev-parse", source + ":crates"], env)
        version_out, _ = commands.run(label + "-llvm-version", [sys.executable, "-c", VERSION_PROBE, llvm["path"]], env)
        require(json.loads(version_out)["version"] == version, "wrong actual DSO version")
        banner, _ = commands.run(label + "-clang-version", [clang["path"], "--version"], env)
        require(re.search(rb"\bclang version " + version.encode().replace(b".", rb"\.") + rb"(?:\s|$)", banner),
                "wrong actual Clang version")
        linked, _ = commands.run(label + "-linked-library", ["ldd", cli["path"]], env)
        libraries = re.findall(r"^\s*\S*LLVM\S*\s+=>\s+(\S+)\s+\(", linked.decode(), re.M)
        require(len(libraries) == 1 and Path(libraries[0]).resolve(strict=True) == Path(llvm["path"]),
                "CLI loader did not select the declared LLVM DSO")
        arms[label] = {"cli": cli["path"], "clang": clang["path"], "llvm": llvm["path"],
                       "env": env, "version": version}
        frozen[label] = {"cli": cli, "clang": clang, "llvm": llvm,
                         "declared_source": source, "crates_tree": tree.decode().strip()}
    require(frozen["baseline"]["cli"]["sha256"] != frozen["candidate"]["cli"]["sha256"], "compiler arms are identical")
    result["source_origin"] = "crates/zil-driver/tests/object_cache212.rs: three-module exit-42 control"
    source = out / "source"
    source.mkdir()
    for name, text in SOURCE.items():
        (source / name).write_text(text)
    result["source_files"] = {name: identity(source / name) for name in SOURCE}
    builds = result["builds"] = []
    result["controls"] = []
    for opt, units, jobs in CONFIGS:
        work = out / opt
        work.mkdir()
        cache = work / "objects"
        program = work / "program"
        before_candidate = None
        previous_cache = {}
        for label, state in (("baseline", "cold"), ("baseline", "warm"), ("candidate", "cold"),
                             ("baseline", "warm-after-candidate"), ("candidate", "warm")):
            arm = arms[label]
            argv = [arm["cli"], "build", source / "main.zil", "-o", program, "--json", "--backend", "llvm-api",
                    "--clang", arm["clang"], "--opt", opt, "--codegen-units", str(units), "--codegen-jobs", str(jobs),
                    "--object-cache", cache]
            name = f"{opt}-{label}-{state}"
            stdout, stderr = commands.run(name, argv, arm["env"])
            require(not stderr, f"unexpected CLI stderr: {name}")
            report = json.loads(stdout)
            actual = verify_build(report, arm["version"], opt, units, state != "cold")
            require(Path(report["artifacts"]["executable"]).resolve() == program, "wrong published executable")
            machine_out, machine_err = commands.run(name + "-native", [program], arm["env"], expected=42)
            require(not machine_out and not machine_err, "native exit-42 oracle produced unexpected output")
            current = snapshot(cache)
            require(current and all(current.get(p) == value for p, value in previous_cache.items()),
                    "previous cache entries disappeared or changed")
            if state == "cold":
                added = set(current) - set(previous_cache)
                require(len(added) == actual, "cold build did not add one entry per unit")
                if label == "candidate":
                    require(before_candidate and added.isdisjoint(before_candidate), "candidate reused old object files")
            else:
                require(current == previous_cache, "warm build changed object entries")
            if label == "baseline" and state == "warm":
                before_candidate = set(current)
            builds.append({"name": name, "version": arm["version"], "actual_units": actual,
                           "cache": report["native_codegen"]["object_cache"], "executable": identity(program),
                           "llvm_ir": identity(report["artifacts"]["llvm"]), "cache_files": current,
                           "native_exit": 42, "verification_runs": report["verification_runs"], "status": "Pass"})
            previous_cache = current
        result["controls"].append({"name": "cache-" + opt, "status": "Pass"})

    # Deliberately override only the version function, not an entire LLVM library.
    # This is a loader-interposition rejection control, not a real LLVM17 ABI run.
    c_source = out / "wrong-version.c"
    c_source.write_text("void LLVMGetVersion(unsigned *a,unsigned *b,unsigned *c){*a=17;*b=0;*c=6;}\n")
    preload = out / "wrong-version.so"
    candidate = arms["candidate"]
    commands.run("build-version-interposer", [candidate["clang"], "-shared", "-fPIC", "-O0", c_source, "-o", preload], candidate["env"])
    prior = {str(program): identity(program), str(program) + ".ll": identity(str(program) + ".ll")}
    cache_before = snapshot(cache)
    injected = candidate["env"].copy()
    injected["LD_PRELOAD"] = str(preload)
    # argv is still the final candidate O2/warm build with its populated cache.
    stdout, _ = commands.run("wrong-llvm-version", argv, injected, expected=2)
    rejected = json.loads(stdout)
    require(rejected["status"] == "InfraError" and rejected["verification_runs"] == 1,
            "version mismatch did not fail after current Verify")
    require(any(d["code"] == "E-LLVM-API-VERSION" and "23.1.4" in d["message"] for d in rejected["diagnostics"]),
            "version mismatch failed for the wrong reason")
    require(not rejected["commands"] and rejected["native_codegen"] is None, "version mismatch reached native publication")
    require(all(identity(path) == value for path, value in prior.items()) and snapshot(cache) == cache_before,
            "version rejection changed previous output or cached objects")
    result["controls"].append({"name": "wrong-llvm-version", "status": "Pass", "kind": "version-function interposition",
                               "interposer": identity(preload), "preserved": prior})

    # Use the real Clang17 executable with the candidate's actual LLVM23 DSO.
    wrong_clang = candidate["env"].copy()
    wrong_clang["ZIL_CLANG"] = arms["baseline"]["clang"]
    wrong_clang["LD_LIBRARY_PATH"] += os.pathsep + arms["baseline"]["env"]["LD_LIBRARY_PATH"]
    stdout, _ = commands.run("wrong-clang-version", [sys.executable, checker], wrong_clang, expected=1)
    rejected = json.loads(stdout)
    require(rejected["status"] == "Fail" and rejected["checks"]["llvm"]["version"] == "23.1.4",
            "checker did not reach the real Clang mismatch")
    require("23.1.4" in rejected["error"] and "17.0.6" in rejected["error"], "checker failed for the wrong reason")
    result["controls"].append({"name": "wrong-clang-version", "status": "Pass", "kind": "actual Clang17 with LLVM23"})

    for arm in ("baseline", "candidate"):
        for key in ("cli", "clang", "llvm"):
            require(identity(frozen[arm][key]["path"]) == frozen[arm][key], "tool identity changed during controls")
    require(identity(checker) == frozen["checker"] and identity(__file__) == frozen["runner"], "validation tool changed")
    require(all(identity(source / name) == value for name, value in result["source_files"].items()), "source changed")
    require(len(builds) == 10 and len(result["controls"]) == 4, "missing control cells")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--candidate-checker", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--timeout", type=float, default=120)
    for arm in ("baseline", "candidate"):
        for suffix in ("", "-clang", "-llvm", "-source"):
            parser.add_argument("--" + arm + suffix, required=True)
        parser.add_argument("--" + arm + "-library-dir", action="append", default=[])
    args = parser.parse_args()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    result = {"status": "Fail", "scope": "LLVM17/23 tool identity, cache isolation and fail-closed version controls",
              "performance": "NotMeasured", "independent_review": "NotRun",
              "limitations": ["Linux host only", "Git source declarations require separate build receipts",
                              "Cache isolation covers the complete compiler+DSO identity, not one key field in isolation"]}
    try:
        require(args.timeout > 0, "timeout must be positive")
        execute(args, out, result)
        result["status"] = "Pass"
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
    save(out / "result.json", result)
    print(json.dumps({"status": result["status"], "result": str(out / "result.json"), "error": result.get("error")}))
    return 0 if result["status"] == "Pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
