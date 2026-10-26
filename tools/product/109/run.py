#!/usr/bin/env python3
"""One frozen BASE/N/S/NS/Rust runtime series; source and raw files stay auditable."""
import argparse
import collections
import copy
import gc
import hashlib
import io
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
import tarfile
import time

if not __debug__:
    raise SystemExit("109 evidence validation requires assertions; Python -O/-OO is refused")

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SPEC = importlib.util.spec_from_file_location("reference104", HERE / "reference104/run.py")
REF = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REF)
REF.ROOT = ROOT
BUILD_SPEC = importlib.util.spec_from_file_location("compiler109", HERE / "build_compilers.py")
BUILD = importlib.util.module_from_spec(BUILD_SPEC)
BUILD_SPEC.loader.exec_module(BUILD)
CONTRACT = json.loads((HERE / "contract.json").read_text())
ARMS = tuple(CONTRACT["arms"])
OWNED = None
EMPTY_SHA = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
ENVIRONMENT = ("PATH", "LD_LIBRARY_PATH", "LANG", "LC_ALL", "TMPDIR", "LD_PRELOAD",
               "LD_AUDIT", "LD_BIND_NOW", "LD_BIND_NOT", "ZIL_PARITY_FAIL_AT")
SOURCE_PREFIX = "benches/rust-parity/109"
CANONICAL_SEQUENCE = "examples/mainline/byte_buffer/sequence.zil"
CAPTURE_FIELDS = ("invocation", "invocation_path", "invocation_sha256", "process_sha256")


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
    return Path(root) / f"{row['group']}-{row['scenario']}-{row['size']}-{row['round']:02d}-{row['arm']}"


def memory_directory(root, row):
    return Path(root) / f"{row['group']}-{row['scenario']}-{row['size']}-{row['repetition']}-{row['arm']}"


def verify_compiler_labels(compilers):
    assert compilers["status"] == "Pass"
    assert compilers["base"] == CONTRACT["production_baseline"]
    assert compilers["candidate"] == CONTRACT["production_candidate"]
    assert set(compilers["arms"]) == {"base", "candidate"}
    for arm in ("base", "candidate"):
        assert compilers["arms"][arm]["commit"] == compilers[arm]


def git_bytes(commit, path):
    return subprocess.check_output(["git", "show", commit + ":" + path], cwd=ROOT)


def git_files(commit, *paths):
    """Hash the actual complete Git input closure, not a manifest's own labels."""
    archived = subprocess.check_output(["git", "archive", commit, *paths], cwd=ROOT)
    result = {}
    with tarfile.open(fileobj=io.BytesIO(archived)) as archive:
        for member in archive:
            if member.isdir():
                continue
            assert member.isfile() and member.name not in result
            assert not Path(member.name).is_absolute() and ".." not in Path(member.name).parts
            result[member.name] = hashlib.sha256(archive.extractfile(member).read()).hexdigest()
    return result


def verify_compilers(path, embedded=None):
    path = Path(path).resolve()
    compilers = json.loads(path.read_text())
    if embedded is not None:
        assert equal_json(compilers, embedded), "compiler receipt differs from sealed identity"
    verify_compiler_labels(compilers)
    assert compilers["target"] == BUILD.TARGET
    assert compilers["builder_sha256"] == REF.sha(HERE / "build_compilers.py")
    assert equal_json(compilers["build_contract"], {key: CONTRACT[key] for key in
        ("production_baseline", "production_candidate", "binding_source_commit", "binding_file_sha256")})
    assert compilers["binding_source_sha256"] == CONTRACT["binding_file_sha256"]
    assert equal_json(compilers["consumer_arms"], {arm: CONTRACT["compiler_for_arm"][arm] for arm in ARMS if arm != "Rust"})
    assert equal_json(compilers["environment"], BUILD.controlled_environment(
        compilers["environment"], path.parent, compilers["rustc"]["path"], compilers["linker"]["path"]))
    expected_source = git_files(CONTRACT["production_baseline"], "Cargo.toml", "Cargo.lock", "crates")
    binding = git_bytes(CONTRACT["binding_source_commit"], BUILD.FOREIGN)
    assert hashlib.sha256(binding).hexdigest() == CONTRACT["binding_file_sha256"]
    assert git_bytes(CONTRACT["production_candidate"], BUILD.FOREIGN) == binding
    for arm in ("base", "candidate"):
        item = compilers["arms"][arm]
        assert item["path"] == str(path.parent / ("zil-" + arm))
        assert item["source_manifest"] == str(path.parent / (arm + "-source-files.json"))
        assert item["command_receipt"] == str(path.parent / (arm + ".command.json"))
        assert REF.sha(item["path"]) == item["sha256"]
        assert REF.sha(item["source_manifest"]) == item["source_manifest_sha256"]
        if arm == "candidate":
            expected_source[BUILD.FOREIGN] = CONTRACT["binding_file_sha256"]
        assert equal_json(json.loads(Path(item["source_manifest"]).read_text()), expected_source), "compiler source closure differs from Git"
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
    assert ARMS == ("BASE", "N", "S", "NS", "Rust") and CONTRACT["admission_arm"] == "NS"
    assert equal_json(CONTRACT["williams_base_indices"], [0, 1, 4, 2, 3])
    forward = [tuple(ARMS[(index + shift) % len(ARMS)] for index in CONTRACT["williams_base_indices"])
               for shift in range(len(ARMS))]
    sequences = forward + [tuple(reversed(order)) for order in forward]
    assert len(set(sequences)) == CONTRACT["williams_sequences"] == 10
    assert CONTRACT["retained_rounds"] == 30 and CONTRACT["warmup_rounds"] == 2 and CONTRACT["williams_repetitions"] == 3
    rows = []
    for group, scenario, size in cells():
        orders = sequences * CONTRACT["williams_repetitions"]
        rng.shuffle(orders)
        assert equal_json(dict(collections.Counter(orders)), {order: 3 for order in sequences})
        assert all(collections.Counter(order[position] for order in orders) == {arm: 6 for arm in ARMS}
                   for position in range(len(ARMS)))
        adjacent = collections.Counter(pair for order in orders for pair in zip(order, order[1:]))
        assert adjacent == {pair: 6 for pair in itertools.permutations(ARMS, 2)}
        for round, order in enumerate([ARMS, tuple(reversed(ARMS))] + orders):
            rows.append({"group": group, "scenario": scenario, "size": size,
                         "round": round, "warmup": round < 2, "order": list(order)})
    assert len(rows) == 384
    assert len(rows) * len(ARMS) == CONTRACT["time_observations"] == 1920
    return rows


def files():
    return {str(p.relative_to(ROOT)): REF.sha(p) for p in sorted(HERE.rglob("*"))
            if p.is_file() and "__pycache__" not in p.parts}


def environment(values):
    assert set(values) == set(ENVIRONMENT)
    assert all(value is None or type(value) is str for value in values.values())
    return {key: value for key, value in values.items() if value is not None}


def verify_reference(path):
    assert REF.sha(path) == CONTRACT["reference_identity_sha256"]
    reference = json.loads(Path(path).read_text())
    assert reference["source_commit"] == CONTRACT["reference_source_commit"]
    assert equal_json(reference["contract"], REF.CONTRACT)
    provenance = json.loads((HERE / "tool-provenance.json").read_text())
    assert len(provenance["files"]) == 9 and sum(file["unchanged_reference"] for file in provenance["files"]) == 6
    for file in provenance["files"]:
        original = git_bytes(file["source_commit"], file["source_path"])
        assert hashlib.sha256(original).hexdigest() == file["initial_copy_sha256"]
        if file["unchanged_reference"]:
            assert (ROOT / file["destination"]).read_bytes() == original
    for tool in reference["versions"].values():
        assert REF.sha(tool["executable"]) == tool["sha256"]
    for key, program in reference["programs"].items():
        if "-rust-" in key:
            assert REF.sha(program["path"]) == program["sha256"]
    return reference


def source_data():
    commit = CONTRACT["production_candidate"]
    provenance_path = SOURCE_PREFIX + "/provenance.json"
    provenance_bytes = git_bytes(commit, provenance_path)
    assert hashlib.sha256(provenance_bytes).hexdigest() == CONTRACT["source_provenance_sha256"]
    provenance = json.loads(provenance_bytes)
    assert provenance["original_source_commit"] == CONTRACT["reference_source_commit"]
    assert equal_json(provenance["original_file_count"], 22) and equal_json(provenance["mapped_file_count"], 4)
    assert provenance["canonical_sequence"] == CANONICAL_SEQUENCE
    data = {provenance_path: provenance_bytes, CANONICAL_SEQUENCE: git_bytes(commit, CANONICAL_SEQUENCE)}
    assert hashlib.sha256(data[CANONICAL_SEQUENCE]).hexdigest() == CONTRACT["canonical_sequence_sha256"]
    assert len(provenance["files"]) == 26 and len({item["path"] for item in provenance["files"]}) == 26
    for item in provenance["files"]:
        path = item["path"]
        assert path.startswith(SOURCE_PREFIX + "/") and ".." not in Path(path).parts
        original = git_bytes(commit, path)
        assert hashlib.sha256(original).hexdigest() == item["sha256"] and equal_json(len(original), item["bytes"])
        origin_commit = item.get("source_commit", commit)
        if "/original/" in path:
            assert origin_commit == CONTRACT["reference_source_commit"]
        assert original == git_bytes(origin_commit, item["source_path"]), path
        data[path] = original
    # Require the entire benchmark tree, including provenance, not just listed entries.
    assert equal_json(git_files(commit, SOURCE_PREFIX), {path: hashlib.sha256(raw).hexdigest()
                      for path, raw in data.items() if path.startswith(SOURCE_PREFIX + "/")})
    return data


def freeze_sources(directory):
    directory.mkdir(parents=True, exist_ok=False)
    data = source_data()
    for path, raw in data.items():
        destination = directory / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(raw)
    return {"commit": CONTRACT["production_candidate"], "root": str(directory),
            "files": {path: {"sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)} for path, raw in data.items()}}


def verify_sources(snapshot):
    assert snapshot["commit"] == CONTRACT["production_candidate"]
    root = Path(snapshot["root"])
    assert root == root.resolve() and not root.is_symlink()
    data = source_data()
    assert equal_json(snapshot["files"], {path: {"sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)} for path, raw in data.items()})
    actual = {str(path.relative_to(root)): path.read_bytes() for path in root.rglob("*") if path.is_file()}
    assert actual == data
    assert not any(path.is_symlink() for path in root.rglob("*"))


def source_for(snapshot, arm, scenario):
    assert arm in ARMS and scenario in CONTRACT["scenario_order"]
    language = "rust" if arm == "Rust" else "zil"
    factor = "mapped" if scenario == "buffer" and arm in ("S", "NS") else "original"
    return Path(snapshot["root"]) / SOURCE_PREFIX / factor / scenario / language / ("main.rs" if language == "rust" else "main.zil")


def input_descriptor(path):
    if path is None:
        return {"kind": "devnull", "path": os.devnull, "bytes": 0, "sha256": EMPTY_SHA}
    path = Path(path)
    assert path == path.resolve() and not path.is_symlink()
    return {"kind": "file", "path": str(path), "bytes": path.stat().st_size, "sha256": REF.sha(path)}


def invocation(argv, stdin, env, cwd):
    argv = list(map(str, argv))
    assert argv and Path(argv[0]).is_absolute()
    assert set(env) <= set(ENVIRONMENT) and all(type(value) is str for value in env.values())
    return {"schema": "zil109-invocation/1", "argv": argv,
            "executable": {"path": argv[0], "sha256": REF.sha(argv[0])},
            "stdin": input_descriptor(stdin), "environment": dict(env), "cwd": str(cwd),
            "timeout_seconds": CONTRACT["process_timeout_seconds"]}


def execute(argv, directory, *, stdin=None, env):
    """Wrap unchanged104 timer; record exact call context outside its measured region."""
    directory = Path(directory).resolve()
    require_new(directory)
    directory.parent.mkdir(parents=True, exist_ok=True)
    sidecar = directory.parent / (directory.name + ".invocation.json")
    context = invocation(argv, stdin, env, ROOT)
    dump_new(sidecar, context)
    old_cwd = Path.cwd()
    try:
        os.chdir(ROOT)
        assert Path.cwd() == Path(context["cwd"])
        process = REF.execute(context["argv"], directory, stdin=stdin, env=dict(env),
                              timeout=CONTRACT["process_timeout_seconds"])
    finally:
        os.chdir(old_cwd)
    capture = dict(process, invocation=context, invocation_path=str(sidecar),
                   invocation_sha256=REF.sha(sidecar), process_sha256=REF.sha(directory / "process.json"))
    dump_new(directory / "capture.json", capture)
    assert equal_json(context, invocation(argv, stdin, env, ROOT)), "input or executable changed during execution"
    return capture


def checked(argv, directory, **kwargs):
    result = execute(argv, directory, **kwargs)
    if result["exit"] or result["timeout"]:
        raise RuntimeError(f"command failed: {directory}: {result['exit']}")
    return result


def compile_program(language, source, binary, optimization, compiler, directory,
                    tools, env, observer=None, proxy=None, emit=None):
    command = REF.compile_args(language, source, binary, optimization, compiler,
                               tools=tools, observer=observer, clang=proxy)
    if language == "zil":
        command += ["--json"]
        if emit:
            command += ["--emit-dir", str(emit)]
    result = checked(command, directory, env=env)
    if language == "zil":
        report = json.loads((Path(result["directory"]) / "stdout").read_text())
        assert report["status"] == "Success" and report["phase"] == "link"
        assert equal_json(report["verification_runs"], 1)
    return result


def instrument_text(text, language):
    if language == "zil":
        needle = "fn main() -> i32 { return run() }"
        replacement = "fn main() -> i32 {\nunsafe { trace_start() }\nlet code = run()\nunsafe { trace_end() }\nreturn code\n}"
        prefix = 'extern "C" fn trace_start();\nextern "C" fn trace_end();\n'
    else:
        needle = "fn main() { let code = run();"
        replacement = "fn main() { unsafe { trace_start(); } let code = run(); unsafe { trace_end(); }"
        prefix = 'unsafe extern "C" { fn trace_start(); fn trace_end(); }\n'
    assert text.count(needle) == 1
    return prefix + text.replace(needle, replacement)


def instrument(source, destination, language):
    shutil.copytree(source.parent, destination)
    main = destination / source.name
    main.write_text(instrument_text(main.read_text(), language))
    return main


def native_check(binary, inp, directory, env, expected, code, events=None):
    process = execute([binary], directory, stdin=inp, env=env)
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
    outer = execute([launcher, receipt, "--", *argv], directory / "outer", stdin=stdin, env=env)
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
        build = checked([tools["clang"]["executable"], "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
                         HERE / "reference104" / (name + ".c"), "-o", binary], out / (name + "-build"), env=env)
        binaries[name] = {"path": str(binary), "sha256": REF.sha(binary), "build": build}
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
    sources = freeze_sources(out / "frozen-source")
    values = {key: reference["execution_environment"].get(key) for key in ENVIRONMENT}
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
            inputs[name] = {"path": str(inp), "sha256": REF.sha(inp), "bytes": len(data)}
            expected_outputs[name] = {"path": str(exp), "sha256": REF.sha(exp), "exit": code}
    observer = out / "observer.o"
    observer_build = checked([clang, "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c",
                              HERE / "reference104/observer.c", "-o", observer], out / "observer-build", env=env)
    proxy = out / "clang-observer"
    proxy.write_text("#!/usr/bin/env python3\nimport os,sys\nos.execv(" + repr(clang) + ", [" + repr(clang) + "] + sys.argv[1:] + " + repr(["-x", "none", str(observer)] + REF.WRAP) + ")\n")
    proxy.chmod(0o755)
    result = {"status": "Running", "scope": "fresh five-arm behavior and allocation validation",
              "programs": {}, "traces": {}, "checks": []}
    for scenario in CONTRACT["scenario_order"]:
        for arm in ARMS:
            language = "rust" if arm == "Rust" else "zil"
            source = source_for(sources, arm, scenario)
            compiler = compilers["arms"].get(CONTRACT["compiler_for_arm"][arm], {}).get("path")
            for opt in (0, 2):
                key = f"{scenario}-{arm}-O{opt}"
                binary = out / key
                if arm == "Rust":
                    original = reference["programs"][f"{scenario}-rust-O{opt}"]
                    shutil.copy2(original["path"], binary)
                    assert REF.sha(binary) == original["sha256"]
                    provenance = {"original_program": original, "kind": "byte-identical 104 Rust binary, freshly re-executed"}
                else:
                    provenance = compile_program(language, source, binary, opt, compiler,
                                                 out / ("build-" + key), tools, env, emit=out / ("phases-" + key))
                result["programs"][key] = {"path": str(binary), "sha256": REF.sha(binary), "provenance": provenance,
                                           "source": str(source), "compiler_flavor": CONTRACT["compiler_for_arm"][arm]}
                for size in CONTRACT["validation_sizes"]:
                    name = f"{scenario}-{size}"
                    check = native_check(binary, inputs[name]["path"], out / f"check-{key}-{size}", env,
                                         Path(expected_outputs[name]["path"]).read_bytes(), 0)
                    result["checks"].append(dict(check, kind="plain", arm=arm, scenario=scenario, optimization=opt, size=size))
                trace_source = instrument(source, out / "trace-source" / key, language)
                trace = out / (key + "-trace")
                trace_build = compile_program(language, trace_source, trace, opt, compiler, out / ("trace-build-" + key),
                                              tools, env, observer=observer if arm == "Rust" else None,
                                              proxy=str(proxy) if arm != "Rust" else None)
                result["traces"][key] = {"path": str(trace), "sha256": REF.sha(trace), "source": str(trace_source),
                                        "provenance": trace_build, "compiler_flavor": CONTRACT["compiler_for_arm"][arm]}
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
    assert len(result["checks"]) == 450 and len(result["programs"]) == 30
    result["status"] = "Pass"
    REF.dump(out / "validation.json", result)
    machine = {}
    for scenario in CONTRACT["scenario_order"]:
        for arm in ARMS:
            binary = result["programs"][f"{scenario}-{arm}-O2"]["path"]
            for tool, options in [("objdump", ["-d", "-Mintel"]), ("readelf", ["-Wr"])]:
                machine[f"{scenario}-{arm}-{tool}"] = checked(
                    [str(Path(shutil.which(tool)).resolve()), *options, binary],
                    out / f"machine-{scenario}-{arm}-{tool}", env=env)
    memory = memory_controls(out, tools, env)
    identity = {"schema": "zil-byte-map-binding/109/freeze/1", "status": "Pass", "contract": CONTRACT,
                "schedule": schedule(), "memory_schedule": memory_schedule(), "files": files(), "created_utc": utc(),
                "sources": sources, "execution_cwd": str(ROOT), "machine": machine,
                "observer": {"path": str(observer), "sha256": REF.sha(observer), "build": observer_build},
                "clang_proxy": {"path": str(proxy), "sha256": REF.sha(proxy)},
                "compiler_receipt": str(build_path), "compiler_receipt_sha256": REF.sha(build_path), "compilers": compilers,
                "reference_identity": str(reference_path), "reference_identity_sha256": REF.sha(reference_path),
                "tools": tools, "environment": values, "profile_sha256": REF.sha(ROOT / CONTRACT["profile"]),
                "programs": result["programs"], "traces": result["traces"], "inputs": inputs, "expected": expected_outputs,
                "validation": str(out / "validation.json"), "validation_sha256": REF.sha(out / "validation.json"),
                "memory_binaries": memory, "memory_controls": str(out / "memory-controls.json"),
                "memory_controls_sha256": REF.sha(out / "memory-controls.json")}
    dump_new(out / "identity-candidate.json", identity)
    verify_identity(out / "identity-candidate.json")
    dump_new(out / "identity.json", identity)
    print(json.dumps({"status": "Pass", "checks": 450, "time_observations": len(schedule()) * len(ARMS),
                      "identity": str(out / "identity.json"), "sha256": REF.sha(out / "identity.json")}))


def verify_identity(path):
    path = Path(path).resolve()
    identity = json.loads(Path(path).read_text())
    assert identity["status"] == "Pass" and equal_json(identity["contract"], CONTRACT)
    assert equal_json(identity["schedule"], schedule()) and equal_json(identity["memory_schedule"], memory_schedule())
    assert equal_json(identity["files"], files())
    assert identity["sources"]["root"] == str(path.parent / "frozen-source")
    assert identity["execution_cwd"] == str(Path(identity["execution_cwd"]).resolve())
    verify_sources(identity["sources"])
    assert REF.sha(ROOT / CONTRACT["profile"]) == identity["profile_sha256"]
    assert git_bytes(CONTRACT["reference_source_commit"], CONTRACT["profile"]) == (ROOT / CONTRACT["profile"]).read_bytes()
    for path_key, sha_key in [("compiler_receipt", "compiler_receipt_sha256"), ("reference_identity", "reference_identity_sha256"),
                              ("validation", "validation_sha256"), ("memory_controls", "memory_controls_sha256")]:
        assert REF.sha(identity[path_key]) == identity[sha_key]
    validation = json.loads(Path(identity["validation"]).read_text())
    assert identity["validation"] == str(path.parent / "validation.json")
    assert identity["memory_controls"] == str(path.parent / "memory-controls.json")
    assert validation["status"] == "Pass" and len(validation["checks"]) == 450
    assert equal_json(validation["programs"], identity["programs"]) and equal_json(validation["traces"], identity["traces"])
    for group in (identity["programs"], identity["traces"], identity["inputs"], identity["expected"], identity["memory_binaries"]):
        for item in group.values():
            assert REF.sha(item["path"]) == item["sha256"]
    for item in identity["tools"].values():
        assert REF.sha(item["executable"]) == item["sha256"]
    verify_compilers(identity["compiler_receipt"], identity["compilers"])
    reference = verify_reference(identity["reference_identity"])
    assert equal_json(identity["tools"], reference["versions"])
    values = {key: reference["execution_environment"].get(key) for key in ENVIRONMENT}
    for key in ("LD_PRELOAD", "LD_AUDIT", "LD_BIND_NOW", "LD_BIND_NOT", "ZIL_PARITY_FAIL_AT"):
        values[key] = None
    assert equal_json(identity["environment"], values)
    verify_programs(identity, reference)
    verify_memory_controls(identity)
    verify_validation(validation, identity)
    return identity


def verify_process(process, expected_directory=None, seen=None):
    directory = Path(process["directory"])
    if expected_directory is not None:
        bind_path(process["directory"], expected_directory, seen if seen is not None else set())
    receipt = directory / "process.json"
    assert not receipt.is_symlink() and REF.sha(receipt) == process["process_sha256"]
    assert equal_json(json.loads(receipt.read_text()), {key: value for key, value in process.items() if key not in CAPTURE_FIELDS})
    capture = directory / "capture.json"
    assert not capture.is_symlink() and equal_json(json.loads(capture.read_text()), process)
    sidecar = directory.parent / (directory.name + ".invocation.json")
    assert process["invocation_path"] == str(sidecar) and not sidecar.is_symlink()
    assert REF.sha(sidecar) == process["invocation_sha256"]
    assert equal_json(json.loads(sidecar.read_text()), process["invocation"])
    assert equal_json(process["argv"], process["invocation"]["argv"])
    assert REF.sha(directory / "stdout") == process["stdout_sha256"]
    assert REF.sha(directory / "stderr") == process["stderr_sha256"]
    assert type(process["exit"]) is int and type(process["timeout"]) is bool
    assert not process["timeout"]
    assert type(process["wall_ns"]) is int and process["wall_ns"] > 0
    assert type(process["peak_rss_kib"]) is int and process["peak_rss_kib"] >= 0
    for key in ("user_seconds", "system_seconds"):
        assert type(process[key]) is float and math.isfinite(process[key]) and process[key] >= 0


def verify_invocation(process, argv, stdin, env, cwd):
    assert equal_json(process["invocation"], invocation(argv, stdin, env, cwd)), "actual input/env/cwd differs from frozen call"


def verify_programs(identity, reference):
    root = Path(identity["validation"]).parent
    env, cwd = environment(identity["environment"]), identity["execution_cwd"]
    expected_keys = {f"{scenario}-{arm}-O{opt}" for scenario in CONTRACT["scenario_order"] for arm in ARMS for opt in (0, 2)}
    assert set(identity["programs"]) == set(identity["traces"]) == expected_keys and len(expected_keys) == 30
    observer, proxy = identity["observer"], identity["clang_proxy"]
    assert observer["path"] == str(root / "observer.o") and proxy["path"] == str(root / "clang-observer")
    assert REF.sha(observer["path"]) == observer["sha256"] and REF.sha(proxy["path"]) == proxy["sha256"]
    clang = identity["tools"]["clang"]["executable"]
    proxy_text = "#!/usr/bin/env python3\nimport os,sys\nos.execv(" + repr(clang) + ", [" + repr(clang) + "] + sys.argv[1:] + " + repr(["-x", "none", observer["path"]] + REF.WRAP) + ")\n"
    assert Path(proxy["path"]).read_text() == proxy_text
    seen = set()
    verify_process(observer["build"], root / "observer-build", seen)
    verify_invocation(observer["build"], [clang, "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c",
                      str(Path(cwd) / "tools/product/109/reference104/observer.c"), "-o", observer["path"]], None, env, cwd)
    assert equal_json(observer["build"]["exit"], 0)
    for scenario in CONTRACT["scenario_order"]:
        for arm in ARMS:
            language = "rust" if arm == "Rust" else "zil"
            flavor = CONTRACT["compiler_for_arm"][arm]
            compiler = identity["compilers"]["arms"].get(flavor, {}).get("path")
            source = source_for(identity["sources"], arm, scenario)
            for opt in (0, 2):
                key = f"{scenario}-{arm}-O{opt}"
                normal, trace = identity["programs"][key], identity["traces"][key]
                assert normal["path"] == str(root / key) and normal["source"] == str(source)
                assert trace["path"] == str(root / (key + "-trace"))
                assert equal_json(normal["compiler_flavor"], flavor) and equal_json(trace["compiler_flavor"], flavor)
                trace_source = root / "trace-source" / key / source.name
                assert trace["source"] == str(trace_source)
                expected_trace = {p.name: p.read_bytes() for p in source.parent.iterdir() if p.is_file()}
                expected_trace[source.name] = instrument_text(source.read_text(), language).encode()
                actual_trace = {p.name: p.read_bytes() for p in trace_source.parent.iterdir() if p.is_file()}
                assert actual_trace == expected_trace and not any(p.is_symlink() for p in trace_source.parent.iterdir())
                for item, is_trace in ((normal, False), (trace, True)):
                    assert REF.sha(item["path"]) == item["sha256"]
                    if arm == "Rust" and not is_trace:
                        original = reference["programs"][f"{scenario}-rust-O{opt}"]
                        assert equal_json(item["provenance"], {"original_program": original, "kind": "byte-identical 104 Rust binary, freshly re-executed"})
                        assert item["sha256"] == original["sha256"]
                        continue
                    command = REF.compile_args(language, trace_source if is_trace else source, item["path"], opt, compiler,
                                               tools=identity["tools"], observer=observer["path"] if is_trace and arm == "Rust" else None,
                                               clang=proxy["path"] if is_trace and arm != "Rust" else None)
                    if language == "zil":
                        command += ["--json"]
                        if not is_trace:
                            command += ["--emit-dir", str(root / ("phases-" + key))]
                    process = item["provenance"]
                    verify_process(process, root / (("trace-build-" if is_trace else "build-") + key), seen)
                    verify_invocation(process, command, None, env, cwd)
                    assert equal_json(process["exit"], 0)
                    if language == "zil":
                        report = json.loads((Path(process["directory"]) / "stdout").read_text())
                        assert report["status"] == "Success" and report["phase"] == "link" and equal_json(report["verification_runs"], 1)
    expected_inputs = {f"{s}-{n}" for s in CONTRACT["scenario_order"] for n in CONTRACT["validation_sizes"]}
    assert set(identity["inputs"]) == set(identity["expected"]) == expected_inputs
    for scenario in CONTRACT["scenario_order"]:
        for size in CONTRACT["validation_sizes"]:
            name = f"{scenario}-{size}"
            inp, output = identity["inputs"][name], identity["expected"][name]
            assert inp["path"] == str(root / "inputs" / (name + ".bin")) and equal_json(inp["bytes"], size)
            assert output["path"] == str(root / "expected" / (name + ".bin")) and equal_json(output["exit"], 0)
            data = REF.input_bytes(scenario, size)
            assert Path(inp["path"]).read_bytes() == data
            expected, code, _ = REF.model(scenario, data)
            assert code == 0 and Path(output["path"]).read_bytes() == expected
    expected_machine = {f"{scenario}-{arm}-{tool}" for scenario in CONTRACT["scenario_order"] for arm in ARMS for tool in ("objdump", "readelf")}
    assert set(identity["machine"]) == expected_machine
    for scenario in CONTRACT["scenario_order"]:
        for arm in ARMS:
            binary = identity["programs"][f"{scenario}-{arm}-O2"]["path"]
            for tool, options in (("objdump", ["-d", "-Mintel"]), ("readelf", ["-Wr"])):
                process = identity["machine"][f"{scenario}-{arm}-{tool}"]
                verify_process(process, root / f"machine-{scenario}-{arm}-{tool}", seen)
                executable = process["invocation"]["executable"]["path"]
                assert Path(executable).name in (tool, "x86_64-linux-gnu-" + tool)
                verify_invocation(process, [executable, *options, binary], None, env, cwd)
                assert equal_json(process["exit"], 0)


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
    assert len(rows) == 450
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
        env = environment(identity["environment"])
        assert type(actual["expected_exit"]) is int and type(actual["failure_triggered"]) is bool
        if actual["kind"] == "plain":
            verify_invocation(process, [identity["programs"][program_key]["path"]], identity["inputs"][name]["path"], env, identity["execution_cwd"])
            assert process["exit"] == actual["expected_exit"] == 0
            assert process["stdout_sha256"] == identity["expected"][name]["sha256"]
            assert process["stderr_sha256"] == EMPTY_SHA
            assert actual["failure_triggered"] is False and actual["allocation_end"] is None
        else:
            env["ZIL_PARITY_FAIL_AT"] = str(actual["failure_index"])
            verify_invocation(process, [identity["traces"][program_key]["path"]], identity["inputs"][name]["path"], env, identity["execution_cwd"])
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
    assert equal_json(result["cpu_affinity"], [CONTRACT["cpu"]])
    assert result["scope"] == "runtime-measurement" and type(result["exclusive_window"]) is str and result["exclusive_window"].strip()
    expected = registered_samples()
    assert len(result["samples"]) == len(expected) == 1920
    seen = set()
    for actual, registered in zip(result["samples"], expected):
        assert equal_json({key: actual.get(key) for key in registered}, registered)
        assert type(actual["warmup"]) is bool and type(actual["round"]) is int
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
                    process = execute([binary], directory, stdin=identity["inputs"][name]["path"], env=env)
                    sample = dict(row, arm=arm, process=process)
                    result["samples"].append(sample)
                    progress.write(json.dumps(sample, sort_keys=True) + "\n")
                    progress.flush()
                    assert not process["timeout"] and process["exit"] == 0
                    assert process["stdout_sha256"] == identity["expected"][name]["sha256"]
                    assert process["stderr_sha256"] == EMPTY_SHA
                print(json.dumps({"completed_round": f"{group}-{name}-{row['round']}"}), flush=True)
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
        verify_invocation(process, [identity["programs"][f"{sample['scenario']}-{sample['arm']}-O2"]["path"]],
                          identity["inputs"][f"{sample['scenario']}-{sample['size']}"]["path"],
                          environment(identity["environment"]), identity["execution_cwd"])
        assert process["stdout_sha256"] == identity["expected"][f"{sample['scenario']}-{sample['size']}"]["sha256"]
        assert process["stderr_sha256"] == EMPTY_SHA
        if sample["warmup"]:
            continue
        key = f"{sample['group']}/{sample['scenario']}/{sample['size']}"
        pairs.setdefault(key, {}).setdefault(sample["round"], {})[sample["arm"]] = process
    report = {"status": "Pass", "scope": "finite-runtime-evidence", "result_sha256": REF.sha(path),
              "identity_sha256": result["identity_sha256"], "cells": {},
              "rss_note": "direct Python-parent wait4 RSS retained in raw records but excluded from target-memory conclusions",
              "interval_note": "descriptive per-cell exact binomial median intervals; no simultaneous family coverage",
              "product": "NotRun"}
    for key, rounds in pairs.items():
        assert len(rounds) == 30 and all(set(row) == set(ARMS) for row in rounds.values())
        times = {arm: [row[arm]["wall_ns"] for row in rounds.values()] for arm in ARMS}
        cell = {"wall_ns": {arm: REF.describe(values) for arm, values in times.items()}, "paired_factors": {},
                "NS_over_BASE_ratio_of_medians": statistics.median(times["NS"]) / statistics.median(times["BASE"]),
                "NS_over_BASE_ratio_of_p95": REF.describe(times["NS"])["p95"] / REF.describe(times["BASE"])["p95"],
                "Rust_over_NS_efficiency": statistics.median(times["Rust"]) / statistics.median(times["NS"])}
        for numerator, denominator in (("N", "BASE"), ("S", "BASE"), ("NS", "BASE"), ("NS", "N"), ("NS", "S"), ("Rust", "NS")):
            ratios = [row[numerator]["wall_ns"] / row[denominator]["wall_ns"] for row in rounds.values()]
            cell["paired_factors"][numerator + "/" + denominator] = {
                "paired_ratios": ratios, "paired": REF.describe(ratios), "median_ci": REF.median_ci(ratios),
                "ratio_of_medians": statistics.median(times[numerator]) / statistics.median(times[denominator]),
                "ratio_of_p95": REF.describe(times[numerator])["p95"] / REF.describe(times[denominator])["p95"]}
        interaction = [(row["NS"]["wall_ns"] * row["BASE"]["wall_ns"]) / (row["N"]["wall_ns"] * row["S"]["wall_ns"])
                       for row in rounds.values()]
        cell["interaction_NS_BASE_over_N_S"] = {"paired_ratios": interaction, "paired": REF.describe(interaction),
                                               "median_ci": REF.median_ci(interaction)}
        report["cells"][key] = cell
    assert len(report["cells"]) == 12
    profile = json.loads((ROOT / CONTRACT["profile"]).read_text())["runtime"]
    groups = {}
    for group in ("runtime", "startup", "all"):
        selected = {key: cell for key, cell in report["cells"].items() if group == "all" or key.startswith(group + "/")}
        efficiencies = [cell["Rust_over_NS_efficiency"] for cell in selected.values()]
        geomean = math.exp(sum(math.log(x) for x in efficiencies) / len(efficiencies))
        failed = [key for key, cell in selected.items() if cell["Rust_over_NS_efficiency"] < profile["min_per_case"]]
        groups[group] = {"cells": len(selected), "geomean_efficiency": geomean,
                         "failed_per_case": failed, "runtime_thresholds_pass": not failed and geomean >= profile["min_geomean"]}
    report["runtime_thresholds"] = groups
    report["admission_arm"] = "NS"
    report["baseline_median_regressions"] = {arm: [key for key, cell in report["cells"].items()
        if cell["wall_ns"][arm]["median"] > cell["wall_ns"]["BASE"]["median"]] for arm in ARMS if arm != "BASE"}
    report["baseline_p95_regressions"] = {arm: [key for key, cell in report["cells"].items()
        if cell["wall_ns"][arm]["p95"] > cell["wall_ns"]["BASE"]["p95"]] for arm in ARMS if arm != "BASE"}
    report["production_admission"] = "PendingReview" if all(x["runtime_thresholds_pass"] for x in groups.values()) else "NoGo"
    report["admission_boundary"] = "NS is the sole preregistered candidate; Rust numeric thresholds, all five-arm factor costs and adverse tails remain distinct for independent production review"
    report["binding_boundary"] = "Linux eager external-symbol resolution; IFUNC timing differs (focused control 152 versus 52); no claim of universal loader equivalence"
    dump_new(destination, report)
    return report


def memory_schedule():
    assert CONTRACT["memory_repetitions"] == len(ARMS) == 5
    rows = []
    for group, scenario, size in cells():
        for repetition in range(CONTRACT["memory_repetitions"]):
            offset = repetition % len(ARMS)
            order = list(ARMS[offset:] + ARMS[:offset])
            rows.append(dict(group=group, scenario=scenario, size=size, repetition=repetition, order=order))
    assert len(rows) * len(ARMS) == CONTRACT["memory_observations"] == 300
    return rows


def validate_memory_rows(result, identity_sha, root):
    assert result["status"] == "Pass" and result["identity_sha256"] == identity_sha
    assert result["contract_sha256"] == REF.sha(HERE / "contract.json")
    assert equal_json(result["cpu_affinity"], [CONTRACT["cpu"]])
    assert result["scope"] == "target-memory-only" and result["timing_conclusions"] == "None"
    assert type(result["exclusive_window"]) is str and result["exclusive_window"].strip()
    assert equal_json(result["schedule"], memory_schedule())
    expected = [dict(row, arm=arm) for row in memory_schedule() for arm in row["order"]]
    assert len(result["samples"]) == len(expected) == 300
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


def verify_observation(observation, directory, launcher, argv, seen, stdin, env, cwd):
    receipt = directory / "target-memory.json"
    bind_path(observation["target_receipt"], receipt, seen)
    assert REF.sha(receipt) == observation["target_receipt_sha256"]
    assert equal_json(json.loads(receipt.read_text()), observation["target"])
    process = observation["outer_not_target_memory_or_timing"]
    verify_process(process, directory / "outer", seen)
    verify_invocation(process, [launcher, str(receipt), "--", *argv], stdin, env, cwd)
    assert equal_json(process["exit"], 0)
    target = observation["target"]
    assert equal_json(target["exit"], 0) and equal_json(target["signal"], 0)
    assert type(target["peak_rss_kib"]) is int and target["peak_rss_kib"] >= 0
    assert set(target) == {"exit", "signal", "peak_rss_kib", "user_seconds", "system_seconds"}
    for key in ("user_seconds", "system_seconds"):
        assert type(target[key]) is float and math.isfinite(target[key]) and target[key] >= 0


def verify_memory_controls(identity):
    path = Path(identity["memory_controls"]).resolve()
    result = json.loads(path.read_text())
    assert result["status"] == "Pass"
    cfg = json.loads((HERE / "reference104/memory-contract.json").read_text())["controls"]
    expected = [dict(padding=p, target_bytes=n, repetition=r) for p in cfg["parent_padding_bytes"]
                for n in cfg["target_touched_bytes"] for r in range(cfg["repetitions"])]
    assert len(result["samples"]) == len(expected) == 12
    seen = set()
    env, cwd = environment(identity["environment"]), identity["execution_cwd"]
    assert set(identity["memory_binaries"]) == {"memory_launcher", "memory_control"}
    for name, item in identity["memory_binaries"].items():
        assert item["path"] == str(path.parent / name)
        process = item["build"]
        verify_process(process, path.parent / (name + "-build"), seen)
        verify_invocation(process, [identity["tools"]["clang"]["executable"], "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
                                   str(Path(cwd) / "tools/product/109/reference104" / (name + ".c")), "-o", item["path"]], None, env, cwd)
        assert equal_json(process["exit"], 0)
    for sample, row in zip(result["samples"], expected):
        assert equal_json({key: sample.get(key) for key in row}, row)
        directory = path.parent / f"control-{row['padding']}-{row['target_bytes']}-{row['repetition']}"
        verify_observation(sample["observation"], directory, identity["memory_binaries"]["memory_launcher"]["path"],
                           [identity["memory_binaries"]["memory_control"]["path"], str(row["target_bytes"])], seen, None, env, cwd)
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
        assert len(result["samples"]) == 300
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
                           identity["memory_binaries"]["memory_launcher"]["path"], [binary], seen,
                           identity["inputs"][f"{row['scenario']}-{row['size']}"]["path"], environment(identity["environment"]), identity["execution_cwd"])
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
            "scope": "runtime-measurement", "exclusive_window": "synthetic-only", "cpu_affinity": [CONTRACT["cpu"]],
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
                 "float-header", "nonzero", "timeout", "zero-time", "wrong-contract", "wrong-identity", "float-affinity", "incomplete"):
        bad = copy.deepcopy(base)
        if name == "missing-cell": bad["samples"] = bad["samples"][160:]
        elif name == "duplicate": bad["samples"][1] = copy.deepcopy(bad["samples"][0])
        elif name == "same-header-process-reuse":
            # The later slot is the same cell and arm; ONLY its payload changes.
            later = next(row for row in bad["samples"] if row["round"] == 1 and row["arm"] == ARMS[0])
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
        elif name == "float-affinity": bad["cpu_affinity"] = [float(CONTRACT["cpu"])]
        else: bad["status"] = "Running"
        reject(name, lambda: validate_rows(bad, "synthetic-row-control", root))

    journal = copy.deepcopy(base["samples"])
    journal[0]["process"]["wall_ns"] = float(journal[0]["process"]["wall_ns"])
    journal_path = out / "progress-only-float.jsonl"
    journal_path.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in journal))
    REF.dump(out / "unchanged-result.json", base)
    reject("only-progress-integer-to-float", lambda: verify_journal(journal_path, base["samples"]))
    # Exercise109's capture plumbing with a substituted recorder, never a benchmark/native target.
    original_execute = REF.execute
    def synthetic_execute(argv, directory, **kwargs):
        directory.mkdir(parents=True, exist_ok=False)
        (directory / "stdout").write_bytes(b"")
        (directory / "stderr").write_bytes(b"")
        process = {"argv": list(argv), "directory": str(directory), "exit": 0, "timeout": False, "wall_ns": 100,
                   "peak_rss_kib": 100, "user_seconds": 0.0, "system_seconds": 0.0,
                   "stdout_sha256": EMPTY_SHA, "stderr_sha256": EMPTY_SHA}
        dump_new(directory / "process.json", process)
        return process
    capture_env = environment({key: "/bin" if key == "PATH" else None for key in ENVIRONMENT})
    executable = str(Path(sys.executable).resolve())
    first_input, other_input = out / "input-a", out / "input-b"
    first_input.write_bytes(b"abc")
    other_input.write_bytes(b"ABC")  # Same uppercase output would not prove the original input.
    try:
        REF.execute = synthetic_execute
        process_root = out / "process-only-float"
        original_process = execute([executable], process_root, stdin=first_input, env=capture_env)
        verify_process(original_process, process_root, set())
        raw = json.loads((process_root / "process.json").read_text())
        raw["wall_ns"] = 100.0
        REF.dump(process_root / "process.json", raw)
        dump_new(process_root / "unchanged-record.json", original_process)
        reject("only-raw-process-integer-to-float", lambda: verify_process(original_process, process_root, set()))
        for name in ("wrong-input-path", "wrong-input-bytes", "wrong-failure-env", "wrong-loader-env", "wrong-cwd", "float-deadline"):
            directory = out / name
            capture = execute([executable], directory, stdin=first_input, env=capture_env)
            verify_process(capture, directory, set())
            verify_invocation(capture, [executable], first_input, capture_env, ROOT)
            context = capture["invocation"]
            if name == "wrong-input-path": context["stdin"] = input_descriptor(other_input)
            elif name == "wrong-input-bytes": context["stdin"]["bytes"] = float(context["stdin"]["bytes"])
            elif name == "wrong-failure-env": context["environment"]["ZIL_PARITY_FAIL_AT"] = "1"
            elif name == "wrong-loader-env": context["environment"]["LD_BIND_NOW"] = "1"
            elif name == "wrong-cwd": context["cwd"] = str(out)
            else: context["timeout_seconds"] = float(context["timeout_seconds"])
            # Deliberately reseal all self-consistent metadata: the frozen call join must reject it.
            REF.dump(capture["invocation_path"], context)
            capture["invocation_sha256"] = REF.sha(capture["invocation_path"])
            REF.dump(directory / "capture.json", capture)
            verify_process(capture, directory, set())
            reject(name, lambda: verify_invocation(capture, [executable], first_input, capture_env, ROOT))
    finally:
        REF.execute = original_execute

    memory_root = out / "synthetic-memory"
    memory_base = {"status": "Pass", "identity_sha256": "synthetic-memory-control", "contract_sha256": REF.sha(HERE / "contract.json"),
                   "scope": "target-memory-only", "timing_conclusions": "None", "exclusive_window": "synthetic-only", "cpu_affinity": [CONTRACT["cpu"]],
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

    snapshot = {"root": str(out / "synthetic-source")}
    for scenario in ("stream", "retained"):
        assert source_for(snapshot, "BASE", scenario) == source_for(snapshot, "S", scenario)
        assert source_for(snapshot, "N", scenario) == source_for(snapshot, "NS", scenario)
    assert source_for(snapshot, "BASE", "buffer") == source_for(snapshot, "N", "buffer")
    assert source_for(snapshot, "S", "buffer") == source_for(snapshot, "NS", "buffer")
    assert source_for(snapshot, "BASE", "buffer") != source_for(snapshot, "S", "buffer")
    controls.append({"name": "factorial-source-aliases-and-Williams-balance-positive", "rejected": False, "passed": True})
    old_admission = CONTRACT["admission_arm"]
    try:
        CONTRACT["admission_arm"] = "N"
        reject("cannot-switch-admission-to-another-arm", schedule)
    finally:
        CONTRACT["admission_arm"] = old_admission

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
    REF.dump(out / "result.json", {"status": "Pass", "scope": "synthetic-harness-integrity-controls-only", "controls": controls,
                                  "target_processes": 0, "python_rejection_processes": 4,
                                  "time_positive_rows": len(base["samples"]), "memory_positive_rows": len(memory_base["samples"])})
    print(json.dumps({"status": "Pass", "controls": len(controls), "rejection_controls": sum(item["rejected"] for item in controls)}))


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
