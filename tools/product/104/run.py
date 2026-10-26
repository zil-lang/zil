#!/usr/bin/env python3
"""Frozen finite ZIL/Rust parity experiment; raw outputs are never overwritten."""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import random
import shutil
import signal
import statistics
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SOURCES = ROOT / "benches/rust-parity/104"
CONTRACT = json.loads((HERE / "contract.json").read_text())
LANGS = ("zil", "rust")
WRAP = ["-Wl,--wrap=" + x for x in ("malloc", "calloc", "realloc", "free")]
OWNED_OUTPUT = None


def create_output(path):
    global OWNED_OUTPUT
    path = Path(path).resolve()
    path.mkdir(parents=True, exist_ok=False)
    OWNED_OUTPUT = path
    return path


def executable(name):
    found = shutil.which(name)
    if found is None:
        raise RuntimeError("missing registered tool: " + name)
    return str(Path(found).resolve())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def dump(path, value):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def input_bytes(scenario, size):
    if scenario == "retained":
        return bytes(0 if i % 257 == 256 else (i * 73 + 19) % 255 + 1 for i in range(size))
    return bytes((i * 73 + (i // 251) * 19) & 255 for i in range(size))


def model(scenario, data, fail_at=0):
    """Independent logical model, including allocation identity and failed growth."""
    output = bytearray()
    history = bytearray()
    capacity = total = attempts = current_id = next_id = peak = allocations = releases = 0
    events = [{"op": "begin"}]
    code = 0

    def release():
        nonlocal capacity, current_id, releases
        if capacity:
            events.append({"op": "release", "id": current_id, "bytes": capacity})
            releases += 1
        capacity = current_id = 0

    for byte in data:
        if scenario == "stream":
            output.append(byte - 32 if 97 <= byte <= 122 else byte)
            continue
        if scenario == "retained" and byte == 0:
            release()
            history.clear()
            total = 0
            output.append(0)
            continue
        if len(history) == capacity:
            requested = capacity * 2 if capacity else 8
            attempts += 1
            success = attempts != fail_at
            old = capacity
            if success:
                if not capacity:
                    next_id += 1
                    current_id = next_id
                    allocations += 1
                capacity = requested
                peak = max(peak, capacity)
            events.append({"op": "reserve", "attempt": attempts, "id": current_id,
                           "old": old, "requested": requested, "success": success})
            if not success:
                code = 2
                break
        history.append(byte)
        if scenario == "retained":
            total += byte
            length = len(history)
            old_byte = history[(total + length) % length]
            output.append((total + length + old_byte) % 251)
    if scenario == "buffer" and code == 0:
        output = bytearray(byte - 32 if 97 <= byte <= 122 else byte for byte in history)
    release()
    events.append({"op": "end", "attempts": attempts, "live": 0, "peak": peak,
                   "allocations": allocations, "releases": releases, "unknown_frees": 0})
    return bytes(output), code, events


def execute(argv, directory, *, stdin=None, env=None, timeout=None):
    """Include spawn, all descendants waited by the child, and final child reap."""
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=False)
    timeout = timeout or CONTRACT["process_timeout_seconds"]
    timed_out = False
    p = None

    def deadline(_signum, _frame):
        raise TimeoutError("registered process deadline")

    old_alarm = signal.signal(signal.SIGALRM, deadline)
    with open(stdin or os.devnull, "rb") as inp, (directory / "stdout").open("wb") as out, \
            (directory / "stderr").open("wb") as err:
        begin = time.perf_counter_ns()
        try:
            signal.setitimer(signal.ITIMER_REAL, timeout)
            p = subprocess.Popen(list(map(str, argv)), stdin=inp, stdout=out, stderr=err,
                                 env=env, start_new_session=True)
            _, status, usage = os.wait4(p.pid, 0)
        except TimeoutError:
            timed_out = True
            if p is None:
                raise
            os.killpg(p.pid, signal.SIGKILL)
            _, status, usage = os.wait4(p.pid, 0)
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            signal.signal(signal.SIGALRM, old_alarm)
        end = time.perf_counter_ns()
        p.returncode = os.waitstatus_to_exitcode(status)
    result = {"argv": list(map(str, argv)), "exit": p.returncode, "timeout": timed_out,
              "wall_ns": end - begin, "user_seconds": usage.ru_utime,
              "system_seconds": usage.ru_stime, "peak_rss_kib": usage.ru_maxrss,
              "stdout_sha256": sha(directory / "stdout"),
              "stderr_sha256": sha(directory / "stderr"), "directory": str(directory)}
    dump(directory / "process.json", result)
    return result


def checked(argv, directory, **kwargs):
    result = execute(argv, directory, **kwargs)
    if result["exit"] or result["timeout"]:
        raise RuntimeError(f"command failed: {directory}: {result['exit']}")
    return result


def rust_args(source, output, opt, *, metadata=False, incremental=None, observer=None, tools=None):
    rustc = tools["rustc"]["executable"] if tools else executable("rustc")
    linker = tools["cc"]["executable"] if tools else executable("cc")
    args = [rustc, "--edition=2024", "--crate-name", "parity104", source,
            "-o", output, "-C", f"opt-level={opt}", "-C", "debuginfo=0",
            "-C", "overflow-checks=yes", "-C", "panic=abort", "-C", "codegen-units=1",
            "-C", "linker=" + linker]
    if metadata:
        args += ["--emit=metadata"]
    if incremental:
        args += ["-C", "incremental=" + str(incremental)]
    if observer:
        args += ["-C", "link-arg=" + str(observer)]
        for arg in WRAP:
            args += ["-C", "link-arg=" + arg]
    return args


def compile_args(language, source, output, opt, zil, *, metadata=False,
                 cache=None, clang=None, observer=None, tools=None):
    if language == "rust":
        return rust_args(source, output, opt, metadata=metadata,
                         incremental=cache, observer=observer, tools=tools)
    args = [zil, "check" if metadata else "build", source]
    if not metadata:
        args += ["-o", output, "--opt", f"O{opt}"]
    if cache:
        args += ["--cache-dir", cache]
    clang = clang or (tools["clang"]["executable"] if tools else executable("clang"))
    args += ["--clang", clang]
    return args


def private_edit(directory, scenario, language):
    edits = {
        ("stream", "zil"): ("transform.zil", "byte >= u8(97) && byte <= u8(122)",
                             "byte > u8(96) && byte < u8(123)"),
        ("stream", "rust"): ("transform.rs", "byte.to_ascii_uppercase()",
                              "if byte.is_ascii_lowercase() { byte - 32 } else { byte }"),
        ("buffer", "zil"): ("sequence.zil", "byte >= u8(97) && byte <= u8(122)",
                             "byte > u8(96) && byte < u8(123)"),
        ("buffer", "rust"): ("sequence.rs", "items.make_ascii_uppercase()",
                              "items[..].make_ascii_uppercase()"),
        ("retained", "zil"): ("state.zil", "state.sum += i64(byte)",
                               "state.sum = state.sum + i64(byte)"),
        ("retained", "rust"): ("state.rs", "state.sum += i64::from(byte);",
                                "state.sum = state.sum + i64::from(byte);")}
    name, before, after = edits[(scenario, language)]
    path = Path(directory) / name
    text = path.read_text()
    if text.count(before) != 1:
        raise AssertionError("private edit must bind exactly one used function body")
    old_sha = sha(path)
    path.write_text(text.replace(before, after))
    return {"file": name, "before": before, "after": after,
            "before_sha256": old_sha, "after_sha256": sha(path)}


def validate(args):
    out = create_output(args.out)
    result = {"status": "Running", "contract_sha256": sha(HERE / "contract.json"),
              "programs": {}, "checks": []}
    dump(out / "result.json", result)
    observer = out / "observer.o"
    checked(["clang", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c",
             HERE / "observer.c", "-o", observer], out / "compile-observer")
    proxy = out / "clang-observer"
    proxy.write_text("#!/usr/bin/env python3\nimport os\nimport sys\nos.execv(" +
                     repr(shutil.which("clang")) + ", [" + repr(shutil.which("clang")) +
                     "] + sys.argv[1:] + " + repr(["-x", "none", str(observer)] + WRAP) + ")\n")
    proxy.chmod(0o755)
    for scenario in CONTRACT["scenario_order"]:
        for size in CONTRACT["validation_sizes"]:
            p = out / "inputs" / f"{scenario}-{size}.bin"
            p.parent.mkdir(exist_ok=True)
            p.write_bytes(input_bytes(scenario, size))
        for language in LANGS:
            extension = "zil" if language == "zil" else "rs"
            source = SOURCES / scenario / language / ("main." + extension)
            for opt in (0, 2):
                key = f"{scenario}-{language}-O{opt}"
                binary = out / key
                checked(compile_args(language, source, binary, opt, args.zil), out / ("build-" + key))
                result["programs"][key] = {"path": str(binary), "sha256": sha(binary)}
                for size in CONTRACT["validation_sizes"]:
                    inp = out / "inputs" / f"{scenario}-{size}.bin"
                    expected, code, _ = model(scenario, inp.read_bytes())
                    run = execute([binary], out / f"run-{key}-{size}", stdin=inp)
                    okay = run["exit"] == code and not run["timeout"] and \
                        (Path(run["directory"]) / "stdout").read_bytes() == expected
                    result["checks"].append({"kind": "plain", "key": key, "size": size,
                                             "pass": okay, "process": run})
                    dump(out / "result.json", result)
                    if not okay:
                        raise AssertionError(f"native mismatch {key}/{size}")
                trace_source = out / "trace-source" / key
                shutil.copytree(source.parent, trace_source)
                main = trace_source / source.name
                text = main.read_text()
                if language == "zil":
                    text = 'extern "C" fn trace_start();\nextern "C" fn trace_end();\n' + text
                    text = text.replace("fn main() -> i32 { return run() }",
                        "fn main() -> i32 {\nunsafe { trace_start() }\nlet code = run()\n"
                        "unsafe { trace_end() }\nreturn code\n}")
                else:
                    text = 'unsafe extern "C" { fn trace_start(); fn trace_end(); }\n' + text
                    text = text.replace("fn main() { let code = run();",
                        "fn main() { unsafe { trace_start(); } let code = run(); "
                        "unsafe { trace_end(); }")
                main.write_text(text)
                trace_binary = out / (key + "-trace")
                checked(compile_args(language, main, trace_binary, opt, args.zil,
                                     clang=proxy, observer=observer), out / ("build-trace-" + key))
                size = CONTRACT["trace_sizes"][scenario]
                inp = out / "inputs" / f"{scenario}-{size}.bin"
                for failure in [0] + CONTRACT["allocation_failure_indices"]:
                    expected, code, events = model(scenario, inp.read_bytes(), failure)
                    env = dict(os.environ, ZIL_PARITY_FAIL_AT=str(failure))
                    run = execute([trace_binary], out / f"trace-{key}-fail{failure}", stdin=inp, env=env)
                    actual = [json.loads(line) for line in (Path(run["directory"]) / "stderr").read_text().splitlines()]
                    okay = run["exit"] == code and not run["timeout"] and actual == events and \
                        (Path(run["directory"]) / "stdout").read_bytes() == expected
                    check = {"kind": "trace", "key": key, "failure_index": failure,
                             "size": size, "pass": okay, "process": run,
                             "failure_triggered": any(e.get("success") is False for e in actual),
                             "expected_end": events[-1], "actual_end": actual[-1] if actual else None}
                    result["checks"].append(check)
                    dump(out / "result.json", result)
                    if not okay:
                        dump(out / f"expected-{key}-fail{failure}.json", events)
                        raise AssertionError(f"trace mismatch {key}/fail{failure}")
    # Exercise every actual private edit at both optimization levels before timing.
    for scenario in CONTRACT["scenario_order"]:
        for language in LANGS:
            destination = out / "edited" / scenario / language
            shutil.copytree(SOURCES / scenario / language, destination)
            edit = private_edit(destination, scenario, language)
            for opt in (0, 2):
                binary = out / f"edited-{scenario}-{language}-O{opt}"
                src = destination / ("main.zil" if language == "zil" else "main.rs")
                checked(compile_args(language, src, binary, opt, args.zil),
                        out / f"compile-edited-{scenario}-{language}-O{opt}")
                inp = out / "inputs" / f"{scenario}-4096.bin"
                run = execute([binary], out / f"run-edited-{scenario}-{language}-O{opt}", stdin=inp)
                expected, code, _ = model(scenario, inp.read_bytes())
                okay = run["exit"] == code and not run["timeout"] and \
                    (Path(run["directory"]) / "stdout").read_bytes() == expected
                result["checks"].append({"kind": "private_edit", "scenario": scenario,
                                         "language": language, "optimization": opt,
                                         "edit": edit, "pass": okay, "process": run})
                dump(out / "result.json", result)
                if not okay:
                    raise AssertionError("edited native mismatch")
    result["status"] = "Pass"
    result["executed_checks"] = len(result["checks"])
    result["source_hashes"] = {str(p.relative_to(ROOT)): sha(p) for p in sorted(SOURCES.rglob("*")) if p.is_file()}
    result["tool_hashes"] = {str(p.relative_to(ROOT)): sha(p) for p in sorted(HERE.iterdir()) if p.is_file()}
    result["input_hashes"] = {str(p.relative_to(out)): sha(p) for p in sorted((out / "inputs").iterdir())}
    result["zil"] = {"path": args.zil, "sha256": sha(args.zil)}
    dump(out / "result.json", result)
    print(json.dumps({"status": "Pass", "checks": result["executed_checks"], "out": str(out)}))


def schedule():
    rng = random.Random(CONTRACT["order_seed"])
    rows = []
    cells = [("runtime", s, n) for s in CONTRACT["scenario_order"] for n in CONTRACT["runtime_sizes"]]
    cells += [("compile", s, kind) for s in CONTRACT["scenario_order"] for kind in CONTRACT["compile_scenarios"]]
    for kind, scenario, variant in cells:
        order = ["zil", "rust"] * (CONTRACT["retained_pairs"] // 2)
        rng.shuffle(order)
        for i, first in enumerate(["zil", "rust"] + order):
            rows.append({"kind": kind, "scenario": scenario, "variant": variant,
                         "pair": i, "warmup": i < CONTRACT["warmup_pairs"],
                         "order": [first, "rust" if first == "zil" else "zil"]})
    return rows


def freeze(args):
    validation = Path(args.validation).resolve()
    result = json.loads((validation / "result.json").read_text())
    if result["status"] != "Pass":
        raise AssertionError("validation incomplete")
    files = dict(result["source_hashes"], **result["tool_hashes"])
    for rel, expected in files.items():
        if sha(ROOT / rel) != expected:
            raise AssertionError("source/tool changed after validation: " + rel)
    if sha(args.zil) != result["zil"]["sha256"]:
        raise AssertionError("compiler changed")
    for program in result["programs"].values():
        if sha(program["path"]) != program["sha256"]:
            raise AssertionError("program changed")
    for rel, expected in result["input_hashes"].items():
        if sha(validation / rel) != expected:
            raise AssertionError("input changed")
    out = create_output(args.out)
    versions = {}
    for name, argv in [("rustc", ["rustc", "-vV"]), ("clang", ["clang", "--version"]),
                       ("cc", ["cc", "--version"]), ("ld", ["ld", "--version"]),
                       ("zil", [args.zil, "version"]), ("system", ["uname", "-a"])]:
        p = checked(argv, out / ("version-" + name))
        executable = Path(shutil.which(str(argv[0]))).resolve()
        versions[name] = {"process": p, "text": (out / ("version-" + name) / "stdout").read_text(),
                          "executable": str(executable), "sha256": sha(executable)}
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    identity = {"contract": CONTRACT, "contract_sha256": sha(HERE / "contract.json"),
                "profile_sha256": sha(ROOT / CONTRACT["profile"]), "files": files,
                "source_commit": head, "validation": str(validation),
                "validation_sha256": sha(validation / "result.json"),
                "input_hashes": result["input_hashes"],
                "programs": result["programs"], "zil": result["zil"], "versions": versions,
                "schedule": schedule(), "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "execution_environment": {key: os.environ.get(key) for key in
                                          ("PATH", "LD_LIBRARY_PATH", "LANG", "LC_ALL", "TMPDIR")},
                "cpu_affinity_before": sorted(os.sched_getaffinity(0))}
    dump(out / "identity.json", identity)
    print(json.dumps({"identity": str(out / "identity.json"), "sha256": sha(out / "identity.json")}))


def timed(args):
    identity_path = Path(args.identity).resolve()
    identity = json.loads(identity_path.read_text())
    for rel, expected in identity["files"].items():
        if sha(ROOT / rel) != expected:
            raise AssertionError("frozen file changed: " + rel)
    if sha(identity["zil"]["path"]) != identity["zil"]["sha256"]:
        raise AssertionError("frozen compiler changed")
    for program in identity["programs"].values():
        if sha(program["path"]) != program["sha256"]:
            raise AssertionError("frozen program changed")
    for tool in identity["versions"].values():
        if sha(tool["executable"]) != tool["sha256"]:
            raise AssertionError("frozen tool changed")
    for rel, expected in identity["input_hashes"].items():
        if sha(Path(identity["validation"]) / rel) != expected:
            raise AssertionError("frozen input changed")
    if CONTRACT != identity["contract"]:
        raise AssertionError("contract changed")
    if identity["schedule"] != schedule():
        raise AssertionError("frozen schedule does not match registered complete order")
    if sha(ROOT / CONTRACT["profile"]) != identity["profile_sha256"]:
        raise AssertionError("product profile changed")
    child_env = dict(os.environ)
    for key, value in identity["execution_environment"].items():
        if value is None:
            child_env.pop(key, None)
        else:
            child_env[key] = value
    child_env["ZIL_CLANG"] = identity["versions"]["clang"]["executable"]
    out = create_output(args.out)
    os.sched_setaffinity(0, {CONTRACT["cpu"]})
    result = {"status": "Running", "identity_sha256": sha(identity_path), "identity": str(identity_path),
              "contract_sha256": sha(HERE / "contract.json"),
              "start_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "samples": []}
    validation = Path(identity["validation"])
    zil = identity["zil"]["path"]
    for row in identity["schedule"]:
        key = f"{row['kind']}-{row['scenario']}-{row['variant']}-{row['pair']:02d}"
        for language in row["order"]:
            directory = out / key / language
            scenario = row["scenario"]
            sample = dict(row, language=language)
            if row["kind"] == "runtime":
                binary = identity["programs"][f"{scenario}-{language}-O2"]["path"]
                inp = validation / "inputs" / f"{scenario}-{row['variant']}.bin"
                measured = checked([binary], directory / "measured", stdin=inp, env=child_env)
                expected, code, _ = model(scenario, inp.read_bytes())
                if code or (directory / "measured/stdout").read_bytes() != expected:
                    raise AssertionError("timed runtime observation mismatch")
            else:
                source_dir = directory / "source"
                shutil.copytree(SOURCES / scenario / language, source_dir)
                source = source_dir / ("main.zil" if language == "zil" else "main.rs")
                output = directory / "program"
                variant = row["variant"]
                cache = directory / "cache" if variant == "private_edit" else None
                cmd = compile_args(language, source, output, 0, zil,
                                   metadata=variant == "cold_check", cache=cache, tools=identity["versions"])
                if variant == "private_edit":
                    sample["prime"] = checked(cmd, directory / "prime", env=child_env)
                    sample["edit"] = private_edit(source_dir, scenario, language)
                measured = checked(cmd, directory / "measured", env=child_env)
                if variant != "cold_check":
                    inp = validation / "inputs" / f"{scenario}-4096.bin"
                    check = checked([output], directory / "native-check", stdin=inp, env=child_env)
                    expected, code, _ = model(scenario, inp.read_bytes())
                    if code or (directory / "native-check/stdout").read_bytes() != expected:
                        raise AssertionError("timed compiler produced wrong program")
                    sample["native_check"] = check
            sample["process"] = measured
            result["samples"].append(sample)
            dump(out / "result.json", result)
        print(json.dumps({"complete_pair": key}), flush=True)
    result["status"] = "Pass"
    result["end_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    dump(out / "result.json", result)
    summarize(out / "result.json", out / "summary.json")


def describe(values):
    values = sorted(values)
    return {"n": len(values), "median": statistics.median(values),
            "p95": values[math.ceil(.95 * len(values)) - 1], "min": values[0], "max": values[-1]}


def median_ci(values):
    n = len(values)
    k = max(k for k in range(n // 2) if 2 * sum(math.comb(n, j) for j in range(k + 1)) / 2**n <= .05)
    s = sorted(values)
    return {"confidence": .95, "familywise": False, "lower": s[k], "upper": s[n - k - 1],
            "order_statistics_1_based": [k + 1, n - k]}


def summarize(path, destination):
    result = json.loads(Path(path).read_text())
    if result["status"] != "Pass":
        raise AssertionError("incomplete series")
    if result.get("contract_sha256") != sha(HERE / "contract.json"):
        raise AssertionError("summary contract mismatch")
    expected = [dict(row, language=language) for row in schedule() for language in row["order"]]
    fields = ("kind", "scenario", "variant", "pair", "warmup", "order", "language")
    if len(result["samples"]) != len(expected):
        raise AssertionError("missing or duplicate registered samples")
    for actual, registered in zip(result["samples"], expected):
        if any(actual.get(key) != registered[key] for key in fields):
            raise AssertionError("sample identity/order differs from registered schedule")
        process = actual["process"]
        if process["exit"] != 0 or process["timeout"] or type(process["wall_ns"]) is not int or process["wall_ns"] <= 0:
            raise AssertionError("invalid process in claimed successful series")
    cells = {}
    for sample in result["samples"]:
        if sample["warmup"]:
            continue
        key = f"{sample['kind']}/{sample['scenario']}/{sample['variant']}"
        cells.setdefault(key, {}).setdefault(sample["pair"], {})[sample["language"]] = sample["process"]
    report = {"status": "Pass", "timed_result_sha256": sha(path), "cells": {}}
    for key, pairs in cells.items():
        if len(pairs) != CONTRACT["retained_pairs"] or any(set(p) != set(LANGS) for p in pairs.values()):
            raise AssertionError("missing pair")
        times = {lang: [p[lang]["wall_ns"] for p in pairs.values()] for lang in LANGS}
        ratios = [p["zil"]["wall_ns"] / p["rust"]["wall_ns"] for p in pairs.values()]
        cell = {"wall_ns": {lang: describe(times[lang]) for lang in LANGS},
                "peak_rss_kib": {lang: describe([p[lang]["peak_rss_kib"] for p in pairs.values()]) for lang in LANGS},
                "zil_over_rust_ratio_of_medians": statistics.median(times["zil"]) / statistics.median(times["rust"]),
                "zil_over_rust_paired_ratio": describe(ratios), "paired_median_ci": median_ci(ratios)}
        cell["rust_over_zil_efficiency"] = 1 / cell["zil_over_rust_ratio_of_medians"]
        report["cells"][key] = cell
    dump(destination, report)
    return report


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="mode", required=True)
    for name in ("validate", "freeze", "timed"):
        p = sub.add_parser(name)
        p.add_argument("--out", required=True)
        if name != "timed":
            p.add_argument("--zil", required=True)
        if name == "freeze":
            p.add_argument("--validation", required=True)
        if name == "timed":
            p.add_argument("--identity", required=True)
    p = sub.add_parser("summarize")
    p.add_argument("result")
    p.add_argument("--out", required=True)
    args = parser.parse_args()
    if args.mode == "summarize":
        summarize(args.result, args.out)
    else:
        try:
            globals()[args.mode](args)
        except Exception as error:
            destination = Path(args.out).resolve()
            if OWNED_OUTPUT == destination:
                dump(destination / "failure.json", {"status": "Failed", "type": type(error).__name__,
                                                     "message": str(error)})
                if (destination / "result.json").exists():
                    result = json.loads((destination / "result.json").read_text())
                    result["status"] = "Failed"
                    dump(destination / "result.json", result)
            raise


if __name__ == "__main__":
    main()
