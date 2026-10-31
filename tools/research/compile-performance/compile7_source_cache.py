#!/usr/bin/env python3
"""Separate source-candidate-cache experiment; historical runners stay unchanged.

prepare accepts --engine zil|rust and --source-cache-a/--source-cache-b on|off.
All remaining arguments are those of bench.py or rust_bench.py. Later actions
read the engine and modes from the frozen plan. The ZIL engine can compare the
same compiler with A=on/B=off; that measures configuration cost, not a code gain.
The Rust engine changes only the ZIL B arm. All original output, native-oracle,
Verify, identity, ordering, CPU and complete-record checks remain in force.
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve()
SCHEMA_SUFFIX = "+compile7-source-cache/1"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def configure(engine, modes):
    if engine not in ("zil", "rust") or set(modes) != {"A", "B"}:
        raise ValueError("Invalid explicit engine/source-cache arm configuration")
    if any(type(x) is not bool for x in modes.values()):
        raise ValueError("Source cache modes must be explicit booleans")
    filename = "bench.py" if engine == "zil" else "rust_bench.py"
    spec = importlib.util.spec_from_file_location("compile7_" + engine, HERE.with_name(filename))
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.SCHEMA += SCHEMA_SUFFIX
    original_command = runner.command
    original_prepare = runner.prepare
    original_load = runner.load

    def command(plan, seq, work):
        argv = original_command(plan, seq, work)
        arm = seq["arm"] if engine == "zil" else seq
        if engine == "rust" and arm == "A":
            return argv
        enabled = plan["compile7_source_cache"]["modes"][arm]
        if not enabled and "--cache-dir" in argv:
            index = argv.index("--cache-dir")
            del argv[index:index + 2]
        return argv

    def prepare(args):
        original_prepare(args)
        path = args.out.resolve() / "plan.json"
        plan = json.loads(path.read_text())
        plan["compile7_source_cache"] = {
            "engine": engine, "modes": modes,
            "extension_sha256": digest(HERE),
            "scope": "Explicit source-candidate configuration experiment; historical runner defaults unchanged; not a compiler implementation speedup",
        }
        # Preparation is not complete until both the original plan and this
        # extension are frozen. No observation has occurred at this point.
        temporary = path.with_name("plan.compile7.tmp")
        temporary.write_text(json.dumps(plan, ensure_ascii=False, indent=2) + "\n")
        temporary.replace(path)

    def load(path, check_environment=True):
        plan = json.loads(Path(path).read_text())
        check_extension(plan, engine, modes)
        return original_load(path, check_environment=check_environment)

    runner.command = command
    runner.prepare = prepare
    runner.load = load
    return runner


def check_extension(plan, engine, modes):
    extension = plan.get("compile7_source_cache", {})
    if (extension.get("engine") != engine or extension.get("modes") != modes
            or extension.get("extension_sha256") != digest(HERE)
            or not str(plan.get("schema", "")).endswith(SCHEMA_SUFFIX)):
        raise SystemExit("Changed or incomplete compile7 source-cache plan")


def take_option(argv, name, default=None):
    if name not in argv:
        return default
    index = argv.index(name)
    if index + 1 == len(argv) or argv.count(name) != 1:
        raise SystemExit("Expected one value for " + name)
    value = argv[index + 1]
    del argv[index:index + 2]
    return value


def main():
    argv = sys.argv[1:]
    if not argv:
        raise SystemExit(__doc__)
    if argv[0] == "prepare":
        engine = take_option(argv, "--engine", "zil")
        values = {arm: take_option(argv, "--source-cache-" + arm.lower(), "on") for arm in ("A", "B")}
        if any(value not in ("on", "off") for value in values.values()):
            raise SystemExit("Source-cache mode must be on or off")
        modes = {arm: value == "on" for arm, value in values.items()}
    else:
        if "--plan" not in argv:
            raise SystemExit("--plan is required")
        path = Path(argv[argv.index("--plan") + 1])
        plan = json.loads(path.read_text())
        extension = plan.get("compile7_source_cache", {})
        engine, modes = extension.get("engine"), extension.get("modes")
        if not isinstance(modes, dict):
            raise SystemExit("Missing frozen source-cache modes")
        check_extension(plan, engine, modes)
    runner = configure(engine, modes)
    sys.argv = [str(HERE), *argv]
    runner.main()


if __name__ == "__main__":
    main()
