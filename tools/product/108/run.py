#!/usr/bin/env python3
"""One frozen BASE/candidate/Rust runtime series; source and raw files stay auditable."""
import argparse
import collections
import copy
import gc
import importlib.util
import itertools
import json
import math
import os
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import sys
import time

if not __debug__:
    raise SystemExit("108 evidence validation requires assertions; Python -O/-OO is refused")

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SPEC = importlib.util.spec_from_file_location("reference104", HERE / "reference104/run.py")
REF = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REF)
REF.ROOT = ROOT
REF.SOURCES = HERE / "programs104"
BUILD_SPEC = importlib.util.spec_from_file_location("compiler108", HERE / "build_compilers.py")
BUILD = importlib.util.module_from_spec(BUILD_SPEC)
BUILD_SPEC.loader.exec_module(BUILD)
CONTRACT = json.loads((HERE / "contract.json").read_text())
ARMS = tuple(CONTRACT["arms"])
OWNED = None
EMPTY_SHA = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
ENVIRONMENT = ("PATH", "LD_LIBRARY_PATH", "LANG", "LC_ALL", "TMPDIR", "LD_PRELOAD",
               "LD_AUDIT", "LD_BIND_NOW", "LD_BIND_NOT", "ZIL_PARITY_FAIL_AT")


def equal_json(left, right):
    """Sealed JSON joins must not equate bool/int or int/float values."""
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(equal_json(left[k], right[k]) for k in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(equal_json(a, b) for a, b in zip(left, right))
    return left == right


def require_new(path):
    path = Path(path)
    if path.exists() or path.is_symlink():
        raise FileExistsError("refusing to overwrite evidence: " + str(path))


def dump_new(path, value):
    with Path(path).open("x") as output:
        output.write(json.dumps(value, indent=2, sort_keys=True) + "\n")


def bind_path(actual, expected, seen):
    """Bind an observation to its registered slot, not a self-declared path."""
    expected = Path(expected)
    assert type(actual) is str and actual == str(expected), (actual, str(expected))
    assert expected == expected.resolve() and not expected.is_symlink(), str(expected)
    assert actual not in seen, "reused observation: " + actual
    seen.add(actual)


def time_directory(root, row):
    return Path(root) / f"{row['group']}-{row['scenario']}-{row['size']}-{row['triplet']:02d}-{row['arm']}"


def memory_directory(root, row):
    return Path(root) / f"{row['group']}-{row['scenario']}-{row['size']}-{row['repetition']}-{row['arm']}"


def verify_compiler_labels(compilers):
    assert compilers["status"] == "Pass"
    assert compilers["base"] == CONTRACT["production_baseline"]
    assert compilers["candidate"] == CONTRACT["production_candidate"]
    assert set(compilers["arms"]) == {"base", "candidate"}
    for arm in ("base", "candidate"):
        assert compilers["arms"][arm]["commit"] == compilers[arm]


def verify_compilers(path, embedded=None):
    path = Path(path).resolve()
    compilers = json.loads(path.read_text())
    if embedded is not None:
        assert equal_json(compilers, embedded), "compiler receipt differs from sealed identity"
    verify_compiler_labels(compilers)
    assert compilers["target"] == BUILD.TARGET
    assert equal_json(compilers["environment"], BUILD.controlled_environment(
        compilers["environment"], path.parent, compilers["rustc"]["path"], compilers["linker"]["path"]))
    for arm in ("base", "candidate"):
        item = compilers["arms"][arm]
        assert REF.sha(item["path"]) == item["sha256"]
        assert REF.sha(item["source_manifest"]) == item["source_manifest_sha256"]
        assert REF.sha(item["command_receipt"]) == item["command_receipt_sha256"]
        command = json.loads(Path(item["command_receipt"]).read_text())
        assert command["source_commit"] == compilers[arm]
        assert command["source_manifest_sha256"] == item["source_manifest_sha256"]
        assert equal_json(command["exit"], 0) and equal_json(command["environment"], compilers["environment"])
        assert equal_json(command["argv"], [compilers["cargo"]["path"], "build", "--release", "--locked",
                                           "--offline", "--target", BUILD.TARGET, "-p", "zil-cli"])
        assert command["cwd"] == str(path.parent / "source")
        for stream in ("stdout", "stderr"):
            assert REF.sha(path.parent / (arm + "." + stream)) == command[stream + "_sha256"]
    for tool in ("cargo", "rustc", "linker"):
        assert REF.sha(compilers[tool]["path"]) == compilers[tool]["sha256"]
    return compilers


def fresh(path):
    global OWNED
    path = Path(path).resolve()
    path.mkdir(parents=True, exist_ok=False)
    OWNED = path
    return path


def utc():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def cells():
    return [("runtime", s, n) for s in CONTRACT["scenario_order"] for n in CONTRACT["runtime_sizes"]] + \
           [("startup", s, n) for s in CONTRACT["scenario_order"] for n in CONTRACT["startup_sizes"]]


def schedule():
    rng = random.Random(CONTRACT["order_seed"])
    permutations = list(itertools.permutations(ARMS))
    assert CONTRACT["retained_triplets"] == 30 and CONTRACT["warmup_triplets"] == 2
    rows = []
    for group, scenario, size in cells():
        orders = permutations * 5
        rng.shuffle(orders)
        assert set(collections.Counter(orders).values()) == {5}
        for triplet, order in enumerate([ARMS, tuple(reversed(ARMS))] + orders):
            rows.append({"group": group, "scenario": scenario, "size": size,
                         "triplet": triplet, "warmup": triplet < 2, "order": list(order)})
    assert len(rows) == 384
    return rows


def files():
    return {str(p.relative_to(ROOT)): REF.sha(p) for p in sorted(HERE.rglob("*"))
            if p.is_file() and "__pycache__" not in p.parts}


def environment(values):
    env = dict(os.environ)
    for key, value in values.items():
        if value is None:
            env.pop(key, None)
        else:
            env[key] = value
    return env


def verify_reference(path):
    assert REF.sha(path) == CONTRACT["reference_identity_sha256"]
    reference = json.loads(Path(path).read_text())
    assert reference["source_commit"] == CONTRACT["reference_source_commit"]
    assert equal_json(reference["contract"], REF.CONTRACT)
    provenance = json.loads((HERE / "reference-provenance.json").read_text())
    for file in provenance["files"]:
        assert REF.sha(ROOT / file["local_path"]) == file["sha256"]
        original = subprocess.check_output(["git", "show", file["commit"] + ":" + file["original_path"]], cwd=ROOT)
        import hashlib
        assert hashlib.sha256(original).hexdigest() == file["sha256"]
        if file["original_path"] in reference["files"]:
            assert reference["files"][file["original_path"]] == file["sha256"]
    for tool in reference["versions"].values():
        assert REF.sha(tool["executable"]) == tool["sha256"]
    for key, program in reference["programs"].items():
        if "-rust-" in key:
            assert REF.sha(program["path"]) == program["sha256"]
    return reference


def compile_program(language, source, binary, optimization, compiler, directory,
                    tools, env, observer=None, proxy=None, emit=None):
    command = REF.compile_args(language, source, binary, optimization, compiler,
                               tools=tools, observer=observer, clang=proxy)
    if language == "zil":
        command += ["--json"]
        if emit:
            command += ["--emit-dir", str(emit)]
    result = REF.checked(command, directory, env=env)
    if language == "zil":
        report = json.loads((Path(result["directory"]) / "stdout").read_text())
        assert report["status"] == "Success" and report["phase"] == "link"
        assert equal_json(report["verification_runs"], 1)
    return result


def instrument(source, destination, language):
    shutil.copytree(source.parent, destination)
    main = destination / source.name
    text = main.read_text()
    if language == "zil":
        needle = "fn main() -> i32 { return run() }"
        replacement = "fn main() -> i32 {\nunsafe { trace_start() }\nlet code = run()\nunsafe { trace_end() }\nreturn code\n}"
        prefix = 'extern "C" fn trace_start();\nextern "C" fn trace_end();\n'
    else:
        needle = "fn main() { let code = run();"
        replacement = "fn main() { unsafe { trace_start(); } let code = run(); unsafe { trace_end(); }"
        prefix = 'unsafe extern "C" { fn trace_start(); fn trace_end(); }\n'
    assert text.count(needle) == 1
    main.write_text(prefix + text.replace(needle, replacement))
    return main


def native_check(binary, inp, directory, env, expected, code, events=None):
    process = REF.execute([binary], directory, stdin=inp, env=env)
    raw = Path(process["directory"])
    assert not process["timeout"] and process["exit"] == code, directory
    assert (raw / "stdout").read_bytes() == expected, directory
    if events is None:
        assert process["stderr_sha256"] == EMPTY_SHA, directory
        actual = None
    else:
        actual = [json.loads(line) for line in (raw / "stderr").read_text().splitlines()]
        if not equal_json(actual, events):
            REF.dump(raw / "expected-events.json", events)
        assert equal_json(actual, events), directory
    return {"process": process, "expected_exit": code,
            "failure_triggered": any(e.get("success") is False for e in actual) if actual else False,
            "allocation_end": actual[-1] if actual else None}


def observe(launcher, argv, directory, env, stdin=None):
    directory.mkdir(parents=True, exist_ok=False)
    receipt = directory / "target-memory.json"
    outer = REF.execute([launcher, receipt, "--", *argv], directory / "outer", stdin=stdin, env=env)
    assert outer["exit"] == 0 and not outer["timeout"]
    target = json.loads(receipt.read_text())
    assert equal_json(target["exit"], 0) and equal_json(target["signal"], 0)
    assert type(target["peak_rss_kib"]) is int and target["peak_rss_kib"] >= 0
    return {"target": target, "target_receipt": str(receipt),
            "target_receipt_sha256": REF.sha(receipt),
            "outer_not_target_memory_or_timing": outer}


def memory_controls(out, tools, env):
    binaries = {}
    for name in ("memory_launcher", "memory_control"):
        binary = out / name
        REF.checked([tools["clang"]["executable"], "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
                     HERE / "reference104" / (name + ".c"), "-o", binary], out / (name + "-build"), env=env)
        binaries[name] = {"path": str(binary), "sha256": REF.sha(binary)}
    cfg = json.loads((HERE / "reference104/memory-contract.json").read_text())["controls"]
    observations = []
    for padding in cfg["parent_padding_bytes"]:
        pad = bytearray(padding)
        for offset in range(0, padding, 4096):
            pad[offset] = 1
        for size in cfg["target_touched_bytes"]:
            for repetition in range(cfg["repetitions"]):
                name = f"control-{padding}-{size}-{repetition}"
                record = observe(binaries["memory_launcher"]["path"],
                                 [binaries["memory_control"]["path"], str(size)], out / name, env)
                observations.append({"padding": padding, "target_bytes": size,
                                     "repetition": repetition, "observation": record})
                REF.dump(out / "memory-controls.json", {"status": "Running", "samples": observations})
        del pad
        gc.collect()
    for size in cfg["target_touched_bytes"]:
        small = [x["observation"]["target"]["peak_rss_kib"] for x in observations if x["target_bytes"] == size and x["padding"] == 0]
        large = [x["observation"]["target"]["peak_rss_kib"] for x in observations if x["target_bytes"] == size and x["padding"] != 0]
        assert abs(statistics.median(large) - statistics.median(small)) <= cfg["max_parent_effect_kib"]
        if size == min(cfg["target_touched_bytes"]):
            assert max(small + large) <= cfg["small_target_max_kib"]
        else:
            assert min(small + large) >= cfg["large_target_min_kib"]
    REF.dump(out / "memory-controls.json", {"status": "Pass", "samples": observations})
    return binaries


def prepare(args):
    out = fresh(args.out)
    reference_path = Path(args.reference).resolve()
    reference = verify_reference(reference_path)
    build_path = Path(args.compilers).resolve()
    compilers = verify_compilers(build_path)
    values = dict(reference["execution_environment"])
    for key in ENVIRONMENT:
        values.setdefault(key, None)
    for key in ("LD_PRELOAD", "LD_AUDIT", "LD_BIND_NOW", "LD_BIND_NOT", "ZIL_PARITY_FAIL_AT"):
        values[key] = None
    env = environment(values)
    tools = reference["versions"]
    clang = tools["clang"]["executable"]
    inputs, expected_outputs = {}, {}
    (out / "inputs").mkdir()
    (out / "expected").mkdir()
    for scenario in CONTRACT["scenario_order"]:
        for size in CONTRACT["validation_sizes"]:
            name = f"{scenario}-{size}"
            data = REF.input_bytes(scenario, size)
            inp = out / "inputs" / (name + ".bin")
            inp.write_bytes(data)
            reference_input = Path(reference["validation"]) / "inputs" / (name + ".bin")
            assert REF.sha(inp) == reference["input_hashes"][str(reference_input.relative_to(reference["validation"]))]
            expected, code, _ = REF.model(scenario, data)
            assert code == 0
            exp = out / "expected" / (name + ".bin")
            exp.write_bytes(expected)
            inputs[name] = {"path": str(inp), "sha256": REF.sha(inp)}
            expected_outputs[name] = {"path": str(exp), "sha256": REF.sha(exp), "exit": code}
    observer = out / "observer.o"
    REF.checked([clang, "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c",
                 HERE / "reference104/observer.c", "-o", observer], out / "observer-build", env=env)
    proxy = out / "clang-observer"
    proxy.write_text("#!/usr/bin/env python3\nimport os,sys\nos.execv(" + repr(clang) + ", [" + repr(clang) + "] + sys.argv[1:] + " + repr(["-x", "none", str(observer)] + REF.WRAP) + ")\n")
    proxy.chmod(0o755)
    result = {"status": "Running", "scope": "fresh three-arm behavior and allocation validation",
              "programs": {}, "traces": {}, "checks": []}
    for scenario in CONTRACT["scenario_order"]:
        for arm in ARMS:
            language = "rust" if arm == "rust" else "zil"
            source = REF.SOURCES / scenario / language / ("main.rs" if language == "rust" else "main.zil")
            compiler = compilers["arms"].get(arm, {}).get("path")
            for opt in (0, 2):
                key = f"{scenario}-{arm}-O{opt}"
                binary = out / key
                if arm == "rust":
                    original = reference["programs"][f"{scenario}-rust-O{opt}"]
                    shutil.copy2(original["path"], binary)
                    assert REF.sha(binary) == original["sha256"]
                    provenance = {"original_program": original, "kind": "byte-identical 104 Rust binary, freshly re-executed"}
                else:
                    provenance = compile_program(language, source, binary, opt, compiler,
                                                 out / ("build-" + key), tools, env, emit=out / ("phases-" + key))
                result["programs"][key] = {"path": str(binary), "sha256": REF.sha(binary), "provenance": provenance}
                for size in CONTRACT["validation_sizes"]:
                    name = f"{scenario}-{size}"
                    check = native_check(binary, inputs[name]["path"], out / f"check-{key}-{size}", env,
                                         Path(expected_outputs[name]["path"]).read_bytes(), 0)
                    result["checks"].append(dict(check, kind="plain", arm=arm, scenario=scenario, optimization=opt, size=size))
                trace_source = instrument(source, out / "trace-source" / key, language)
                trace = out / (key + "-trace")
                compile_program(language, trace_source, trace, opt, compiler, out / ("trace-build-" + key),
                                tools, env, observer=observer if arm == "rust" else None,
                                proxy=str(proxy) if arm != "rust" else None)
                result["traces"][key] = {"path": str(trace), "sha256": REF.sha(trace)}
                size = CONTRACT["trace_sizes"][scenario]
                inp = inputs[f"{scenario}-{size}"]["path"]
                data = Path(inp).read_bytes()
                for failure in CONTRACT["allocation_failure_indices"]:
                    expected, code, events = REF.model(scenario, data, failure)
                    check = native_check(trace, inp, out / f"trace-{key}-failure{failure}",
                                         dict(env, ZIL_PARITY_FAIL_AT=str(failure)), expected, code, events)
                    result["checks"].append(dict(check, kind="trace", arm=arm, scenario=scenario,
                                                  optimization=opt, failure_index=failure, size=size))
                REF.dump(out / "validation.json", result)
                print(json.dumps({"validated": key}), flush=True)
    assert len(result["checks"]) == 270 and len(result["programs"]) == 18
    result["status"] = "Pass"
    REF.dump(out / "validation.json", result)
    for scenario in CONTRACT["scenario_order"]:
        for arm in ARMS:
            binary = result["programs"][f"{scenario}-{arm}-O2"]["path"]
            for tool, options in [("objdump", ["-d", "-Mintel"]), ("readelf", ["-Wr"])]:
                REF.checked([shutil.which(tool), *options, binary], out / f"machine-{scenario}-{arm}-{tool}", env=env)
    memory = memory_controls(out, tools, env)
    identity = {"schema": "zil-native-binding/108/freeze/1", "status": "Pass", "contract": CONTRACT,
                "schedule": schedule(), "files": files(), "created_utc": utc(),
                "compiler_receipt": str(build_path), "compiler_receipt_sha256": REF.sha(build_path), "compilers": compilers,
                "reference_identity": str(reference_path), "reference_identity_sha256": REF.sha(reference_path),
                "tools": tools, "environment": values, "profile_sha256": REF.sha(ROOT / CONTRACT["profile"]),
                "programs": result["programs"], "traces": result["traces"], "inputs": inputs, "expected": expected_outputs,
                "validation": str(out / "validation.json"), "validation_sha256": REF.sha(out / "validation.json"),
                "memory_binaries": memory, "memory_controls": str(out / "memory-controls.json"),
                "memory_controls_sha256": REF.sha(out / "memory-controls.json")}
    REF.dump(out / "identity.json", identity)
    print(json.dumps({"status": "Pass", "checks": 270, "time_observations": len(schedule()) * 3,
                      "identity": str(out / "identity.json"), "sha256": REF.sha(out / "identity.json")}))


def verify_identity(path):
    identity = json.loads(Path(path).read_text())
    assert identity["status"] == "Pass" and equal_json(identity["contract"], CONTRACT)
    assert equal_json(identity["schedule"], schedule()) and equal_json(identity["files"], files())
    assert REF.sha(ROOT / CONTRACT["profile"]) == identity["profile_sha256"]
    for path_key, sha_key in [("compiler_receipt", "compiler_receipt_sha256"), ("reference_identity", "reference_identity_sha256"),
                              ("validation", "validation_sha256"), ("memory_controls", "memory_controls_sha256")]:
        assert REF.sha(identity[path_key]) == identity[sha_key]
    validation = json.loads(Path(identity["validation"]).read_text())
    assert validation["status"] == "Pass" and len(validation["checks"]) == 270
    assert equal_json(validation["programs"], identity["programs"]) and equal_json(validation["traces"], identity["traces"])
    for group in (identity["programs"], identity["traces"], identity["inputs"], identity["expected"], identity["memory_binaries"]):
        for item in group.values():
            assert REF.sha(item["path"]) == item["sha256"]
    for item in identity["tools"].values():
        assert REF.sha(item["executable"]) == item["sha256"]
    verify_compilers(identity["compiler_receipt"], identity["compilers"])
    verify_memory_controls(identity)
    verify_validation(validation, identity)
    return identity


def verify_process(process, expected_directory=None, seen=None):
    directory = Path(process["directory"])
    if expected_directory is not None:
        bind_path(process["directory"], expected_directory, seen if seen is not None else set())
    receipt = directory / "process.json"
    assert not receipt.is_symlink()
    assert equal_json(json.loads(receipt.read_text()), process)
    assert REF.sha(directory / "stdout") == process["stdout_sha256"]
    assert REF.sha(directory / "stderr") == process["stderr_sha256"]
    assert type(process["exit"]) is int and type(process["timeout"]) is bool
    assert not process["timeout"]
    assert type(process["wall_ns"]) is int and process["wall_ns"] > 0


def verify_journal(path, samples):
    progress = [json.loads(line) for line in Path(path).read_text().splitlines()]
    assert equal_json(progress, samples), "progress differs from result (including JSON types)"


def validation_rows():
    rows = []
    for scenario in CONTRACT["scenario_order"]:
        for arm in ARMS:
            for opt in (0, 2):
                rows += [dict(kind="plain", arm=arm, scenario=scenario, optimization=opt, size=n)
                         for n in CONTRACT["validation_sizes"]]
                rows += [dict(kind="trace", arm=arm, scenario=scenario, optimization=opt,
                              size=CONTRACT["trace_sizes"][scenario], failure_index=n)
                         for n in CONTRACT["allocation_failure_indices"]]
    assert len(rows) == 270
    return rows


def verify_validation(validation, identity):
    registered = validation_rows()
    assert len(validation["checks"]) == len(registered)
    cache, seen = {}, set()
    root = Path(identity["validation"]).resolve().parent
    for actual, expected in zip(validation["checks"], registered):
        assert equal_json({key: actual.get(key) for key in expected}, expected)
        process = actual["process"]
        program_key = f"{actual['scenario']}-{actual['arm']}-O{actual['optimization']}"
        name = f"{actual['scenario']}-{actual['size']}"
        directory = root / (f"check-{program_key}-{actual['size']}" if actual["kind"] == "plain"
                            else f"trace-{program_key}-failure{actual['failure_index']}")
        verify_process(process, directory, seen)
        assert type(actual["expected_exit"]) is int and type(actual["failure_triggered"]) is bool
        if actual["kind"] == "plain":
            assert equal_json(process["argv"], [identity["programs"][program_key]["path"]])
            assert process["exit"] == actual["expected_exit"] == 0
            assert process["stdout_sha256"] == identity["expected"][name]["sha256"]
            assert process["stderr_sha256"] == EMPTY_SHA
        else:
            assert equal_json(process["argv"], [identity["traces"][program_key]["path"]])
            key = (actual["scenario"], actual["size"], actual["failure_index"])
            if key not in cache:
                cache[key] = REF.model(key[0], Path(identity["inputs"][name]["path"]).read_bytes(), key[2])
            output, code, events = cache[key]
            assert process["exit"] == actual["expected_exit"] == code
            directory = Path(process["directory"])
            assert (directory / "stdout").read_bytes() == output
            assert equal_json([json.loads(line) for line in (directory / "stderr").read_text().splitlines()], events)
            assert equal_json(actual["allocation_end"], events[-1])
            assert equal_json(actual["failure_triggered"], any(e.get("success") is False for e in events))


def registered_samples():
    return [dict(row, arm=arm) for row in schedule() for arm in row["order"]]


def validate_rows(result, identity_sha, root):
    assert result["status"] == "Pass" and result["identity_sha256"] == identity_sha
    assert result["contract_sha256"] == REF.sha(HERE / "contract.json")
    expected = registered_samples()
    assert len(result["samples"]) == len(expected) == 1152
    seen = set()
    for actual, registered in zip(result["samples"], expected):
        assert equal_json({key: actual.get(key) for key in registered}, registered)
        assert type(actual["warmup"]) is bool and type(actual["triplet"]) is int
        assert type(actual["size"]) is int
        process = actual["process"]
        bind_path(process["directory"], time_directory(root, registered), seen)
        assert type(process["exit"]) is int and type(process["timeout"]) is bool
        assert process["exit"] == 0 and not process["timeout"]
        assert type(process["wall_ns"]) is int and process["wall_ns"] > 0


def timed(args):
    assert args.window.strip()
    identity_path = Path(args.identity).resolve()
    identity = verify_identity(identity_path)
    out = fresh(args.out)
    os.sched_setaffinity(0, {CONTRACT["cpu"]})
    env = environment(identity["environment"])
    result = {"status": "Running", "scope": "runtime-measurement", "identity": str(identity_path),
              "identity_sha256": REF.sha(identity_path), "contract_sha256": REF.sha(HERE / "contract.json"),
              "exclusive_window": args.window, "cpu_affinity": sorted(os.sched_getaffinity(0)),
              "started_utc": utc(), "samples": []}
    REF.dump(out / "result.json", result)
    try:
        with (out / "progress.jsonl").open("x") as progress:
            for row in schedule():
                group, scenario, size = row["group"], row["scenario"], row["size"]
                name = f"{scenario}-{size}"
                for arm in row["order"]:
                    binary = identity["programs"][f"{scenario}-{arm}-O2"]["path"]
                    directory = time_directory(out, dict(row, arm=arm))
                    process = REF.execute([binary], directory, stdin=identity["inputs"][name]["path"], env=env)
                    sample = dict(row, arm=arm, process=process)
                    result["samples"].append(sample)
                    progress.write(json.dumps(sample, sort_keys=True) + "\n")
                    progress.flush()
                    assert not process["timeout"] and process["exit"] == 0
                    assert process["stdout_sha256"] == identity["expected"][name]["sha256"]
                    assert process["stderr_sha256"] == EMPTY_SHA
                print(json.dumps({"completed_triplet": f"{group}-{name}-{row['triplet']}"}), flush=True)
        result["ended_utc"] = utc()
        result["status"] = "Pass"
        validate_rows(result, result["identity_sha256"], out)
        REF.dump(out / "result.json", result)
    except BaseException:
        result["status"] = "Failed"
        result["ended_utc"] = utc()
        REF.dump(out / "result.json", result)
        raise
    summarize(out / "result.json", out / "summary.json")


def summarize(path, destination):
    require_new(destination)
    path = Path(path).resolve()
    result = json.loads(path.read_text())
    identity = verify_identity(result["identity"])
    validate_rows(result, REF.sha(result["identity"]), path.parent)
    verify_journal(path.parent / "progress.jsonl", result["samples"])
    pairs = {}
    seen = set()
    for sample in result["samples"]:
        process = sample["process"]
        verify_process(process, time_directory(path.parent, sample), seen)
        assert equal_json(process["argv"], [identity["programs"][f"{sample['scenario']}-{sample['arm']}-O2"]["path"]])
        assert process["stdout_sha256"] == identity["expected"][f"{sample['scenario']}-{sample['size']}"]["sha256"]
        assert process["stderr_sha256"] == EMPTY_SHA
        if sample["warmup"]:
            continue
        key = f"{sample['group']}/{sample['scenario']}/{sample['size']}"
        pairs.setdefault(key, {}).setdefault(sample["triplet"], {})[sample["arm"]] = process
    report = {"status": "Pass", "scope": "finite-runtime-evidence", "result_sha256": REF.sha(path),
              "identity_sha256": result["identity_sha256"], "cells": {},
              "rss_note": "direct Python-parent wait4 RSS retained in raw records but excluded from target-memory conclusions",
              "interval_note": "descriptive per-cell exact binomial median intervals; no simultaneous family coverage",
              "product": "NotRun"}
    for key, triplets in pairs.items():
        assert len(triplets) == 30 and all(set(row) == set(ARMS) for row in triplets.values())
        times = {arm: [row[arm]["wall_ns"] for row in triplets.values()] for arm in ARMS}
        candidate_over_base = [row["candidate"]["wall_ns"] / row["base"]["wall_ns"] for row in triplets.values()]
        rust_over_candidate = [row["rust"]["wall_ns"] / row["candidate"]["wall_ns"] for row in triplets.values()]
        cell = {"wall_ns": {arm: REF.describe(values) for arm, values in times.items()},
                "candidate_over_base_ratio_of_medians": statistics.median(times["candidate"]) / statistics.median(times["base"]),
                "candidate_over_base_ratio_of_p95": REF.describe(times["candidate"])["p95"] / REF.describe(times["base"])["p95"],
                "rust_over_candidate_efficiency": statistics.median(times["rust"]) / statistics.median(times["candidate"]),
                "candidate_over_base_paired": REF.describe(candidate_over_base),
                "candidate_over_base_median_ci": REF.median_ci(candidate_over_base),
                "rust_over_candidate_paired": REF.describe(rust_over_candidate),
                "rust_over_candidate_median_ci": REF.median_ci(rust_over_candidate)}
        report["cells"][key] = cell
    assert len(report["cells"]) == 12
    profile = json.loads((ROOT / CONTRACT["profile"]).read_text())["runtime"]
    groups = {}
    for group in ("runtime", "startup", "all"):
        selected = {key: cell for key, cell in report["cells"].items() if group == "all" or key.startswith(group + "/")}
        efficiencies = [cell["rust_over_candidate_efficiency"] for cell in selected.values()]
        geomean = math.exp(sum(math.log(x) for x in efficiencies) / len(efficiencies))
        failed = [key for key, cell in selected.items() if cell["rust_over_candidate_efficiency"] < profile["min_per_case"]]
        groups[group] = {"cells": len(selected), "geomean_efficiency": geomean,
                         "failed_per_case": failed, "runtime_thresholds_pass": not failed and geomean >= profile["min_geomean"]}
    report["runtime_thresholds"] = groups
    report["baseline_median_regressions"] = [key for key, cell in report["cells"].items() if cell["candidate_over_base_ratio_of_medians"] > 1]
    report["baseline_p95_regressions"] = [key for key, cell in report["cells"].items() if cell["wall_ns"]["candidate"]["p95"] > cell["wall_ns"]["base"]["p95"]]
    report["production_admission"] = "PendingReview" if all(x["runtime_thresholds_pass"] for x in groups.values()) else "NoGo"
    report["admission_boundary"] = "Rust numeric thresholds and candidate/BASE costs are distinct; all startup and runtime regressions remain visible for independent production review"
    report["binding_boundary"] = "Linux eager external-symbol resolution; IFUNC timing differs (focused control 152 versus 52); no claim of universal loader equivalence"
    dump_new(destination, report)
    return report


def memory_schedule():
    rows = []
    for group, scenario, size in cells():
        for repetition in range(CONTRACT["memory_repetitions"]):
            offset = repetition % len(ARMS)
            order = list(ARMS[offset:] + ARMS[:offset])
            rows.append(dict(group=group, scenario=scenario, size=size, repetition=repetition, order=order))
    return rows


def validate_memory_rows(result, identity_sha, root):
    assert result["status"] == "Pass" and result["identity_sha256"] == identity_sha
    assert result["contract_sha256"] == REF.sha(HERE / "contract.json")
    assert equal_json(result["schedule"], memory_schedule())
    expected = [dict(row, arm=arm) for row in memory_schedule() for arm in row["order"]]
    assert len(result["samples"]) == len(expected) == 180
    seen = set()
    for sample, row in zip(result["samples"], expected):
        assert equal_json({key: sample.get(key) for key in row}, row)
        observation = sample["observation"]
        directory = memory_directory(root, row)
        bind_path(observation["target_receipt"], directory / "target-memory.json", seen)
        process = observation["outer_not_target_memory_or_timing"]
        bind_path(process["directory"], directory / "outer", seen)
        assert equal_json(process["exit"], 0) and equal_json(process["timeout"], False)
        target = observation["target"]
        assert equal_json(target["exit"], 0) and equal_json(target["signal"], 0)
        assert type(target["peak_rss_kib"]) is int and target["peak_rss_kib"] >= 0


def verify_observation(observation, directory, launcher, argv, seen):
    receipt = directory / "target-memory.json"
    bind_path(observation["target_receipt"], receipt, seen)
    assert REF.sha(receipt) == observation["target_receipt_sha256"]
    assert equal_json(json.loads(receipt.read_text()), observation["target"])
    process = observation["outer_not_target_memory_or_timing"]
    verify_process(process, directory / "outer", seen)
    assert equal_json(process["argv"], [launcher, str(receipt), "--", *argv])
    assert equal_json(process["exit"], 0)
    target = observation["target"]
    assert equal_json(target["exit"], 0) and equal_json(target["signal"], 0)
    assert type(target["peak_rss_kib"]) is int and target["peak_rss_kib"] >= 0


def verify_memory_controls(identity):
    path = Path(identity["memory_controls"]).resolve()
    result = json.loads(path.read_text())
    assert result["status"] == "Pass"
    cfg = json.loads((HERE / "reference104/memory-contract.json").read_text())["controls"]
    expected = [dict(padding=p, target_bytes=n, repetition=r) for p in cfg["parent_padding_bytes"]
                for n in cfg["target_touched_bytes"] for r in range(cfg["repetitions"])]
    assert len(result["samples"]) == len(expected) == 12
    seen = set()
    for sample, row in zip(result["samples"], expected):
        assert equal_json({key: sample.get(key) for key in row}, row)
        directory = path.parent / f"control-{row['padding']}-{row['target_bytes']}-{row['repetition']}"
        verify_observation(sample["observation"], directory, identity["memory_binaries"]["memory_launcher"]["path"],
                           [identity["memory_binaries"]["memory_control"]["path"], str(row["target_bytes"])], seen)
        process = sample["observation"]["outer_not_target_memory_or_timing"]
        assert process["stdout_sha256"] == process["stderr_sha256"] == EMPTY_SHA
    for size in cfg["target_touched_bytes"]:
        small = [x["observation"]["target"]["peak_rss_kib"] for x in result["samples"] if x["target_bytes"] == size and x["padding"] == 0]
        large = [x["observation"]["target"]["peak_rss_kib"] for x in result["samples"] if x["target_bytes"] == size and x["padding"] != 0]
        assert abs(statistics.median(large) - statistics.median(small)) <= cfg["max_parent_effect_kib"]
        if size == min(cfg["target_touched_bytes"]):
            assert max(small + large) <= cfg["small_target_max_kib"]
        else:
            assert min(small + large) >= cfg["large_target_min_kib"]


def memory(args):
    assert args.window.strip()
    identity_path = Path(args.identity).resolve()
    identity = verify_identity(identity_path)
    out = fresh(args.out)
    os.sched_setaffinity(0, {CONTRACT["cpu"]})
    env = environment(identity["environment"])
    launcher = identity["memory_binaries"]["memory_launcher"]["path"]
    result = {"status": "Running", "scope": "target-memory-only", "identity": str(identity_path),
              "identity_sha256": REF.sha(identity_path), "contract_sha256": REF.sha(HERE / "contract.json"),
              "exclusive_window": args.window, "cpu_affinity": sorted(os.sched_getaffinity(0)),
              "schedule": memory_schedule(), "started_utc": utc(), "samples": [], "timing_conclusions": "None"}
    REF.dump(out / "result.json", result)
    try:
        for row in memory_schedule():
            for arm in row["order"]:
                name = f"{row['scenario']}-{row['size']}"
                binary = identity["programs"][f"{row['scenario']}-{arm}-O2"]["path"]
                directory = memory_directory(out, dict(row, arm=arm))
                observation = observe(launcher, [binary], directory, env, stdin=identity["inputs"][name]["path"])
                process = observation["outer_not_target_memory_or_timing"]
                assert process["stdout_sha256"] == identity["expected"][name]["sha256"]
                assert process["stderr_sha256"] == EMPTY_SHA
                result["samples"].append(dict(row, arm=arm, observation=observation))
                REF.dump(out / "result.json", result)
            print(json.dumps({"memory_completed": f"{row['group']}-{name}-{row['repetition']}"}), flush=True)
        assert len(result["samples"]) == 180
        result["status"] = "Pass"
        result["ended_utc"] = utc()
        validate_memory_rows(result, result["identity_sha256"], out)
        REF.dump(out / "result.json", result)
    except BaseException:
        result["status"] = "Failed"
        REF.dump(out / "result.json", result)
        raise
    memory_summary(out / "result.json", out / "summary.json")


def memory_summary(path, destination):
    require_new(destination)
    path = Path(path).resolve()
    result = json.loads(path.read_text())
    identity = verify_identity(result["identity"])
    validate_memory_rows(result, REF.sha(result["identity"]), path.parent)
    expected = [dict(row, arm=arm) for row in memory_schedule() for arm in row["order"]]
    values, seen = {}, set()
    for sample, row in zip(result["samples"], expected):
        observation = sample["observation"]
        process = observation["outer_not_target_memory_or_timing"]
        binary = identity["programs"][f"{row['scenario']}-{row['arm']}-O2"]["path"]
        verify_observation(observation, memory_directory(path.parent, row),
                           identity["memory_binaries"]["memory_launcher"]["path"], [binary], seen)
        assert process["stdout_sha256"] == identity["expected"][f"{row['scenario']}-{row['size']}"]["sha256"]
        assert process["stderr_sha256"] == EMPTY_SHA
        key = f"{row['group']}/{row['scenario']}/{row['size']}"
        values.setdefault(key, {}).setdefault(row["arm"], []).append(observation["target"]["peak_rss_kib"])
    report = {"status": "Pass", "scope": "target-memory-only", "result_sha256": REF.sha(path),
              "metric": "maximum individual target process RSS KiB after native exec/fork launcher",
              "timing_conclusions": "None", "cells": {}}
    for key, arms in values.items():
        assert set(arms) == set(ARMS) and all(len(v) == 5 for v in arms.values())
        report["cells"][key] = {arm: dict(REF.describe(v), samples=v) for arm, v in arms.items()}
    assert len(report["cells"]) == 12
    dump_new(destination, report)
    return report


def selftest(args):
    out = fresh(args.out)
    root = out / "synthetic-time"
    base = {"status": "Pass", "identity_sha256": "synthetic-row-control", "contract_sha256": REF.sha(HERE / "contract.json"),
            "samples": [dict(row, process={"exit": 0, "timeout": False, "wall_ns": 100,
                                           "directory": str(time_directory(root, row))}) for row in registered_samples()]}
    validate_rows(base, "synthetic-row-control", root)
    controls = []

    def reject(name, check, exception=AssertionError):
        try:
            check()
        except exception as error:
            controls.append({"name": name, "rejected": True, "exception": type(error).__name__, "message": str(error)})
        else:
            raise AssertionError("accepted malformed evidence: " + name)

    for name in ("missing-cell", "duplicate", "same-header-process-reuse", "wrong-arm", "wrong-order", "wrong-warmup",
                 "float-header", "nonzero", "timeout", "zero-time", "wrong-contract", "wrong-identity", "incomplete"):
        bad = copy.deepcopy(base)
        if name == "missing-cell": bad["samples"] = bad["samples"][96:]
        elif name == "duplicate": bad["samples"][1] = copy.deepcopy(bad["samples"][0])
        elif name == "same-header-process-reuse":
            # The later slot is the same cell and arm; ONLY its payload changes.
            later = next(row for row in bad["samples"] if row["triplet"] == 1 and row["arm"] == ARMS[0])
            original_header = {key: value for key, value in later.items() if key != "process"}
            later["process"] = copy.deepcopy(bad["samples"][0]["process"])
            assert equal_json(original_header, {key: value for key, value in later.items() if key != "process"})
            REF.dump(out / "same-header-process-reuse.json", {"original_header": original_header, "mutated_later_row": later})
        elif name == "wrong-arm": bad["samples"][0]["arm"] = "unregistered"
        elif name == "wrong-order": bad["samples"][0]["order"].reverse()
        elif name == "wrong-warmup": bad["samples"][0]["warmup"] = False
        elif name == "float-header": bad["samples"][0]["size"] = float(bad["samples"][0]["size"])
        elif name == "nonzero": bad["samples"][0]["process"]["exit"] = 2
        elif name == "timeout": bad["samples"][0]["process"]["timeout"] = True
        elif name == "zero-time": bad["samples"][0]["process"]["wall_ns"] = 0
        elif name == "wrong-contract": bad["contract_sha256"] = "changed"
        elif name == "wrong-identity": bad["identity_sha256"] = "changed"
        else: bad["status"] = "Running"
        reject(name, lambda: validate_rows(bad, "synthetic-row-control", root))

    journal = copy.deepcopy(base["samples"])
    journal[0]["process"]["wall_ns"] = float(journal[0]["process"]["wall_ns"])
    journal_path = out / "progress-only-float.jsonl"
    journal_path.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in journal))
    REF.dump(out / "unchanged-result.json", base)
    reject("only-progress-integer-to-float", lambda: verify_journal(journal_path, base["samples"]))
    process_root = out / "process-only-float"
    process_root.mkdir()
    (process_root / "stdout").write_bytes(b"")
    (process_root / "stderr").write_bytes(b"")
    original_process = {"directory": str(process_root), "exit": 0, "timeout": False, "wall_ns": 100,
                        "stdout_sha256": EMPTY_SHA, "stderr_sha256": EMPTY_SHA}
    bad_process = dict(original_process, wall_ns=100.0)
    REF.dump(process_root / "process.json", bad_process)
    REF.dump(process_root / "unchanged-record.json", original_process)
    reject("only-raw-process-integer-to-float", lambda: verify_process(original_process, process_root, set()))

    memory_root = out / "synthetic-memory"
    memory_base = {"status": "Pass", "identity_sha256": "synthetic-memory-control", "contract_sha256": REF.sha(HERE / "contract.json"),
                   "schedule": memory_schedule(), "samples": []}
    for row in (dict(r, arm=a) for r in memory_schedule() for a in r["order"]):
        directory = memory_directory(memory_root, row)
        memory_base["samples"].append(dict(row, observation={"target_receipt": str(directory / "target-memory.json"),
            "target": {"exit": 0, "signal": 0, "peak_rss_kib": 100},
            "outer_not_target_memory_or_timing": {"directory": str(directory / "outer"), "exit": 0, "timeout": False}}))
    validate_memory_rows(memory_base, "synthetic-memory-control", memory_root)
    for name in ("same-header-memory-reuse", "float-memory-header", "float-memory-exit", "float-memory-rss", "missing-memory-slot"):
        bad = copy.deepcopy(memory_base)
        if name == "same-header-memory-reuse":
            later = next(row for row in bad["samples"] if row["repetition"] == 1 and row["arm"] == ARMS[0])
            original_header = {key: value for key, value in later.items() if key != "observation"}
            later["observation"] = copy.deepcopy(bad["samples"][0]["observation"])
            assert equal_json(original_header, {key: value for key, value in later.items() if key != "observation"})
            REF.dump(out / "same-header-memory-reuse.json", {"original_header": original_header, "mutated_later_row": later})
        elif name == "float-memory-header": bad["samples"][0]["repetition"] = 0.0
        elif name == "float-memory-exit": bad["samples"][0]["observation"]["target"]["exit"] = 0.0
        elif name == "float-memory-rss": bad["samples"][0]["observation"]["target"]["peak_rss_kib"] = 100.0
        else: bad["samples"].pop()
        reject(name, lambda: validate_memory_rows(bad, "synthetic-memory-control", memory_root))

    labels = {"status": "Pass", "base": CONTRACT["production_baseline"], "candidate": CONTRACT["production_candidate"],
              "arms": {arm: {"commit": CONTRACT["production_" + ("baseline" if arm == "base" else "candidate")]} for arm in ("base", "candidate")}}
    verify_compiler_labels(labels)
    receipt = out / "compiler-label-control.json"
    REF.dump(receipt, labels)
    bad = copy.deepcopy(labels)
    bad["arms"]["candidate"]["commit"] = labels["base"]
    reject("compiler-arm-label-not-joined", lambda: verify_compiler_labels(bad))
    reject("embedded-compiler-label-only", lambda: verify_compilers(receipt, bad))

    inherited = {"PATH": "/test/bin", "CARGO_HOME": "/test/cache", "RUSTFLAGS": "forbidden", "CARGO_ENCODED_RUSTFLAGS": "forbidden",
                 "RUSTC_WRAPPER": "/forbidden", "RUSTC_WORKSPACE_WRAPPER": "/forbidden", "RUSTC_BOOTSTRAP": "1",
                 "CARGO_BUILD_RUSTFLAGS": "forbidden", "CARGO_BUILD_RUSTC_WRAPPER": "/forbidden",
                 "CARGO_PROFILE_RELEASE_OPT_LEVEL": "0", "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS": "forbidden",
                 "LD_PRELOAD": "/forbidden"}
    env = BUILD.controlled_environment(inherited, out, "/test/rustc", "/test/linker")
    assert all(env.get(key) != value for key, value in inherited.items() if key not in BUILD.INHERITED_ENVIRONMENT)
    assert env["CARGO_ENCODED_RUSTFLAGS"] == env["RUSTFLAGS"] == env["RUSTC_WRAPPER"] == env["RUSTC_WORKSPACE_WRAPPER"] == ""
    controls.append({"name": "encoded-flags-wrappers-profile-and-loader-cleared", "rejected": True})

    protected = out / "protected-result.json"
    protected.write_text('{"retained_original":true}\n')
    original = protected.read_bytes()
    reject("summary-cannot-overwrite-result", lambda: summarize(protected, protected), FileExistsError)
    reject("memory-summary-cannot-overwrite-result", lambda: memory_summary(protected, protected), FileExistsError)
    reject("exclusive-writer-cannot-overwrite-result", lambda: dump_new(protected, {}), FileExistsError)
    assert protected.read_bytes() == original
    for script in ("run.py", "build_compilers.py"):
        for flag in ("-O", "-OO"):
            command = [sys.executable, flag, str(HERE / script), "--help"]
            process = subprocess.run(command, capture_output=True, check=False)
            name = script.removesuffix(".py") + flag
            (out / (name + ".stdout")).write_bytes(process.stdout)
            (out / (name + ".stderr")).write_bytes(process.stderr)
            assert process.returncode != 0 and b"Python -O/-OO is refused" in process.stderr
            controls.append({"name": name, "rejected": True, "argv": command, "exit": process.returncode})
    REF.dump(out / "result.json", {"status": "Pass", "scope": "synthetic-harness-integrity-controls-only", "controls": controls})
    print(json.dumps({"status": "Pass", "rejection_controls": len(controls)}))


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="mode", required=True)
    p = sub.add_parser("prepare")
    p.add_argument("--reference", required=True)
    p.add_argument("--compilers", required=True)
    p.add_argument("--out", required=True)
    for mode in ("timed", "memory"):
        p = sub.add_parser(mode)
        p.add_argument("--identity", required=True)
        p.add_argument("--window", required=True)
        p.add_argument("--out", required=True)
    for mode in ("summarize", "memory-summary"):
        p = sub.add_parser(mode)
        p.add_argument("result")
        p.add_argument("--out", required=True)
    p = sub.add_parser("selftest")
    p.add_argument("--out", required=True)
    p = sub.add_parser("verify")
    p.add_argument("--identity", required=True)
    args = parser.parse_args()
    try:
        if args.mode == "summarize": summarize(args.result, args.out)
        elif args.mode == "memory-summary": memory_summary(args.result, args.out)
        elif args.mode == "verify":
            verify_identity(args.identity)
            print(json.dumps({"status": "Pass", "identity_sha256": REF.sha(args.identity)}))
        else: globals()[args.mode](args)
    except BaseException as error:
        if OWNED is not None:
            REF.dump(OWNED / "failure.json", {"status": "Failed", "type": type(error).__name__, "message": str(error)})
        raise


if __name__ == "__main__":
    main()
