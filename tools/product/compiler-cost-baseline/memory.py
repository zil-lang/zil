#!/usr/bin/env python3
"""Separate target-memory profile. Never substitutes for the frozen time series."""
import argparse
import gc
import importlib.util
import json
import os
from pathlib import Path
import shutil
import statistics
import sys
import time
import traceback

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("parity104", HERE / "run.py")
BASE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BASE)
CONTRACT = json.loads((HERE / "memory-contract.json").read_text())
OWNED_OUTPUT = None


def fresh(path):
    global OWNED_OUTPUT
    path = Path(path).resolve()
    path.mkdir(parents=True, exist_ok=False)
    OWNED_OUTPUT = path
    return path


def validate_parent(path):
    identity = json.loads(Path(path).read_text())
    if identity["contract"] != BASE.CONTRACT or identity["schedule"] != BASE.schedule():
        raise AssertionError("original frozen contract/schedule differs")
    for rel, digest in identity["files"].items():
        assert BASE.sha(BASE.ROOT / rel) == digest, rel
    for rel, digest in identity["input_hashes"].items():
        assert BASE.sha(Path(identity["validation"]) / rel) == digest, rel
    for program in identity["programs"].values():
        assert BASE.sha(program["path"]) == program["sha256"]
    for tool in identity["versions"].values():
        assert BASE.sha(tool["executable"]) == tool["sha256"]
    assert BASE.sha(identity["zil"]["path"]) == identity["zil"]["sha256"]
    assert BASE.sha(BASE.ROOT / BASE.CONTRACT["profile"]) == identity["profile_sha256"]
    return identity


def environment(identity):
    env = dict(os.environ)
    for key, value in identity["execution_environment"].items():
        if value is None:
            env.pop(key, None)
        else:
            env[key] = value
    env["ZIL_CLANG"] = identity["versions"]["clang"]["executable"]
    return env


def observe(launcher, argv, directory, env, stdin=None):
    directory.mkdir(parents=True, exist_ok=False)
    receipt = directory / "target-memory.json"
    outer = BASE.execute([str(launcher), str(receipt), "--", *map(str, argv)],
                         directory / "outer", stdin=stdin, env=env)
    if outer["exit"] or outer["timeout"]:
        raise AssertionError("target or memory collector failed: " + str(directory))
    target = json.loads(receipt.read_text())
    assert target["exit"] == 0 and target["signal"] == 0
    assert type(target["peak_rss_kib"]) is int and target["peak_rss_kib"] >= 0
    return {"argv": list(map(str, argv)), "target": target,
            "outer_observation_not_target_memory_or_timing": outer,
            "stdout": str(directory / "outer/stdout"),
            "stderr": str(directory / "outer/stderr")}


def schedule():
    cells = []
    for row in BASE.schedule():
        key = (row["kind"], row["scenario"], row["variant"])
        if key not in cells:
            cells.append(key)
    return [{"kind": kind, "scenario": scenario, "variant": variant,
             "repetition": i, "order": ["zil", "rust"] if i % 2 == 0 else ["rust", "zil"]}
            for kind, scenario, variant in cells for i in range(CONTRACT["repetitions"])]


def prepare(args):
    parent_path = Path(args.identity).resolve()
    parent = validate_parent(parent_path)
    out = fresh(args.out)
    env = environment(parent)
    clang = parent["versions"]["clang"]["executable"]
    binaries = {}
    for name in ("memory_launcher", "memory_control"):
        binary = out / name
        BASE.checked([clang, "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
                      str(HERE / (name + ".c")), "-o", str(binary)], out / (name + "-build"), env=env)
        binaries[name] = {"path": str(binary), "sha256": BASE.sha(binary)}
    controls = []
    cfg = CONTRACT["controls"]
    for padding in cfg["parent_padding_bytes"]:
        pad = bytearray(padding)
        for offset in range(0, padding, 4096):
            pad[offset] = 1
        for size in cfg["target_touched_bytes"]:
            for rep in range(cfg["repetitions"]):
                name = f"control-parent{padding}-target{size}-{rep}"
                observation = observe(binaries["memory_launcher"]["path"],
                                      [binaries["memory_control"]["path"], str(size)], out / name, env)
                controls.append({"parent_padding_bytes": padding, "target_bytes": size,
                                 "repetition": rep, "observation": observation})
                BASE.dump(out / "controls.json", {"status": "Running", "samples": controls})
        del pad
        gc.collect()
    for size in cfg["target_touched_bytes"]:
        low = [x["observation"]["target"]["peak_rss_kib"] for x in controls
               if x["target_bytes"] == size and x["parent_padding_bytes"] == 0]
        high = [x["observation"]["target"]["peak_rss_kib"] for x in controls
                if x["target_bytes"] == size and x["parent_padding_bytes"] != 0]
        assert abs(statistics.median(high) - statistics.median(low)) <= cfg["max_parent_effect_kib"]
        if size == min(cfg["target_touched_bytes"]):
            assert max(low + high) <= cfg["small_target_max_kib"]
        else:
            assert min(low + high) >= cfg["large_target_min_kib"]
    BASE.dump(out / "controls.json", {"status": "Pass", "samples": controls})
    files = {name: BASE.sha(HERE / name) for name in
             ("memory.py", "memory_launcher.c", "memory_control.c", "memory-contract.json")}
    identity = {"parent_identity": str(parent_path), "parent_sha256": BASE.sha(parent_path),
                "contract": CONTRACT, "schedule": schedule(), "files": files, "binaries": binaries,
                "controls": str(out / "controls.json"), "controls_sha256": BASE.sha(out / "controls.json"),
                "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    BASE.dump(out / "identity.json", identity)
    print(json.dumps({"status": "Pass", "controls": len(controls), "identity": str(out / "identity.json"),
                      "sha256": BASE.sha(out / "identity.json")}))


def run(args):
    identity_path = Path(args.identity).resolve()
    identity = json.loads(identity_path.read_text())
    assert identity["contract"] == CONTRACT and identity["schedule"] == schedule()
    assert BASE.sha(identity["parent_identity"]) == identity["parent_sha256"]
    assert BASE.sha(identity["controls"]) == identity["controls_sha256"]
    assert json.loads(Path(identity["controls"]).read_text())["status"] == "Pass"
    for name, digest in identity["files"].items():
        assert BASE.sha(HERE / name) == digest
    for binary in identity["binaries"].values():
        assert BASE.sha(binary["path"]) == binary["sha256"]
    parent = validate_parent(identity["parent_identity"])
    env = environment(parent)
    out = fresh(args.out)
    validation = Path(parent["validation"])
    launcher = identity["binaries"]["memory_launcher"]["path"]
    result = {"status": "Running", "identity_sha256": BASE.sha(identity_path),
              "identity": str(identity_path), "samples": [], "timing_conclusions": "None"}
    for row in identity["schedule"]:
        scenario, variant = row["scenario"], row["variant"]
        key = f"{row['kind']}-{scenario}-{variant}-{row['repetition']}"
        for language in row["order"]:
            directory = out / key / language
            sample = dict(row, language=language)
            if row["kind"] == "runtime":
                binary = parent["programs"][f"{scenario}-{language}-O2"]["path"]
                inp = validation / "inputs" / f"{scenario}-{variant}.bin"
                observation = observe(launcher, [binary], directory / "observed", env, stdin=inp)
                expected, code, _ = BASE.model(scenario, inp.read_bytes())
                assert code == 0 and Path(observation["stdout"]).read_bytes() == expected
            else:
                source_dir = directory / "source"
                shutil.copytree(BASE.SOURCES / scenario / language, source_dir)
                source = source_dir / ("main.zil" if language == "zil" else "main.rs")
                output = directory / "program"
                cache = directory / "cache" if variant == "private_edit" else None
                cmd = BASE.compile_args(language, source, output, 0, parent["zil"]["path"],
                                        metadata=variant == "cold_check", cache=cache, tools=parent["versions"])
                if variant == "private_edit":
                    sample["prime"] = BASE.checked(cmd, directory / "prime", env=env)
                    sample["edit"] = BASE.private_edit(source_dir, scenario, language)
                observation = observe(launcher, cmd, directory / "observed", env)
                if variant != "cold_check":
                    inp = validation / "inputs" / f"{scenario}-4096.bin"
                    sample["native_check"] = BASE.checked([output], directory / "native-check", stdin=inp, env=env)
                    expected, code, _ = BASE.model(scenario, inp.read_bytes())
                    assert code == 0 and (directory / "native-check/stdout").read_bytes() == expected
            sample["observation"] = observation
            result["samples"].append(sample)
            BASE.dump(out / "result.json", result)
        print(json.dumps({"complete_pair": key}), flush=True)
    assert len(result["samples"]) == len(schedule()) * 2 == 150
    result["status"] = "Pass"
    BASE.dump(out / "result.json", result)
    cells = {}
    for sample in result["samples"]:
        key = f"{sample['kind']}/{sample['scenario']}/{sample['variant']}"
        cells.setdefault(key, {}).setdefault(sample["language"], []).append(sample["observation"]["target"]["peak_rss_kib"])
    summary = {"status": "Pass", "result_sha256": BASE.sha(out / "result.json"),
               "metric": "target maximum individual-process RSS KiB, post-exec small launcher",
               "timing_conclusions": "None", "cells": {}}
    for key, languages in cells.items():
        assert set(languages) == {"zil", "rust"}
        assert all(len(values) == CONTRACT["repetitions"] for values in languages.values())
        summary["cells"][key] = {language: {"median": statistics.median(values), "max": max(values),
                                           "min": min(values), "n": len(values), "samples": values}
                                 for language, values in languages.items()}
    BASE.dump(out / "summary.json", summary)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["prepare", "run"])
    parser.add_argument("--identity", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    try:
        (prepare if args.action == "prepare" else run)(args)
    except Exception as error:
        if OWNED_OUTPUT is not None:
            BASE.dump(OWNED_OUTPUT / "failure.json", {"status": "Failed", "error": repr(error)})
        traceback.print_exc()
        sys.exit(1)
