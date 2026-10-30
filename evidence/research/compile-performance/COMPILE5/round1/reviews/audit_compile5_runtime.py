#!/usr/bin/env python3
"""Independent, read-only audit of the four compile5 native runtime windows.

Never imports the measured runners, recompiles, executes a retained executable,
or writes within the evidence tree. Source identities are mandatory arguments;
actual codegen unit counts are read from receipts, never copied from compile4.
Transport/tar authenticity and compiler-build statistical review are separate.
"""
from __future__ import annotations

import argparse
import collections
import datetime
import hashlib
import json
import math
from pathlib import Path
import random
import statistics
import struct
import subprocess
import sys


WINDOWS = ("rust-o0-cold", "rust-o0-incremental", "rust-o2-closed", "o2-direct-quality")
CASES = {
    "tree4x2": (.05, 1048576, 4, 2),
    "tree32x8": (.15, 32768, 32, 8),
    "tree256x8": (.30, 4096, 256, 8),
    "tree1024x8": (.30, 1024, 1024, 8),
    "retained": (.20, 262144, None, None),
}
SNAPSHOTS = {"cold_process": "base", "unchanged": "base", "private_edit": "private", "public_edit": "public"}
REQUESTS = {"cold_process": .10, "unchanged": .35, "private_edit": .40, "public_edit": .15}


def strict_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate JSON key: " + key)
        result[key] = value
    return result


def parse(text):
    def invalid(value):
        raise ValueError("Non-finite JSON value: " + value)
    return json.loads(text, object_pairs_hook=strict_pairs, parse_constant=invalid)


def load(path):
    return parse(path.read_text())


def rows(path):
    return [parse(line) for line in path.read_text().splitlines()]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def elf_details(path):
    """Parse ELF64 LE metadata and section bytes without invoking an executable."""
    data = path.read_bytes()
    if len(data) < 64 or data[:7] != b"\x7fELF\x02\x01\x01":
        raise ValueError("Expected an ELF64 little-endian version-1 executable")
    h = struct.unpack_from("<16sHHIQQQIHHHHHH", data)
    _, kind, machine, version, entry, phoff, shoff, _, ehsize, phsize, phnum, shsize, shnum, names_index = h
    if kind not in (2, 3) or machine != 62 or version != 1 or ehsize != 64:
        raise ValueError("ELF is not an x86-64 executable/PIE")
    if phsize < 56 or phnum == 0 or shsize < 64 or shnum == 0 or names_index >= shnum:
        raise ValueError("Unsupported or missing ELF program/section table")

    def region(offset, size):
        if offset < 0 or size < 0 or offset > len(data) or size > len(data) - offset:
            raise ValueError("ELF table/section exceeds retained bytes")
        return data[offset:offset + size]

    region(phoff, phsize * phnum)
    region(shoff, shsize * shnum)
    segments = []
    for i in range(phnum):
        ptype, flags, offset, virtual, _, size, memory, align = struct.unpack_from("<IIQQQQQQ", data, phoff + i * phsize)
        payload = region(offset, size)
        if ptype == 1:
            if size > memory:
                raise ValueError("ELF load segment file size exceeds memory size")
            segments.append({"flags": flags, "vaddr": virtual, "file_bytes": size,
                             "memory_bytes": memory, "align": align, "sha256": digest(payload)})
    if not any(s["flags"] & 1 and s["vaddr"] <= entry < s["vaddr"] + s["memory_bytes"] for s in segments):
        raise ValueError("ELF entry is outside all executable load segments")
    sections = [struct.unpack_from("<IIQQQQIIQQ", data, shoff + i * shsize) for i in range(shnum)]
    string_table = region(sections[names_index][4], sections[names_index][5])
    executable, data_sections = [], []
    for name_index, stype, flags, _, offset, size, _, _, align, _ in sections:
        if name_index >= len(string_table):
            raise ValueError("ELF section name outside string table")
        end = string_table.find(b"\0", name_index)
        if end < 0:
            raise ValueError("Unterminated ELF section name")
        name = string_table[name_index:end].decode("utf-8", "strict")
        payload = None if stype == 8 else region(offset, size)
        info = {"name": name, "type": stype, "flags": flags, "size": size, "align": align,
                "sha256": None if payload is None else digest(payload)}
        if flags & 4:
            executable.append(info)
        if name in (".rodata", ".data", ".bss"):
            data_sections.append(info)
    if not any(s["name"] == ".text" and s["size"] > 0 for s in executable):
        raise ValueError("ELF has no nonempty executable .text section")
    return {"kind": "ET_EXEC" if kind == 2 else "ET_DYN", "machine": "x86-64", "entry": entry,
            "load_segments": segments, "executable_sections": executable, "data_sections": data_sections}


class Audit:
    def __init__(self, args):
        self.args = args
        self.root = args.evidence.resolve(strict=True)
        self.repo = args.repo.resolve(strict=True)
        self.receipt = load(self.root / "result.json")
        self.original = Path(self.receipt["original_evidence_root"])
        self.index = load(self.root / "files.json")
        self.failures, self.checks, self.hashes, self.source_cache = [], 0, {}, {}
        self.native_counts, self.elfs, self.window_results = collections.Counter(), {}, {}
        self.lookups = {}

    def check(self, condition, label):
        self.checks += 1
        if not condition:
            self.failures.append(label)

    def path(self, base, relative):
        relative = Path(relative)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("Expected contained relative evidence path")
        path = base / relative
        if not path.resolve().is_relative_to(self.root) or path.is_symlink():
            raise ValueError("Evidence path escapes retained regular-file tree")
        return path

    def mapped(self, original):
        return self.path(self.root, Path(original).relative_to(self.original))

    def sha(self, path):
        if path not in self.hashes:
            h = hashlib.sha256()
            with path.open("rb") as stream:
                while chunk := stream.read(1024 * 1024):
                    h.update(chunk)
            self.hashes[path] = h.hexdigest()
        return self.hashes[path]

    def source(self, relative):
        if relative not in self.source_cache:
            self.source_cache[relative] = subprocess.check_output(
                ["git", "show", self.args.expect_candidate + ":" + relative], cwd=self.repo)
        return self.source_cache[relative]

    def indexed(self, prefix):
        selected = {p for p in self.index if p.startswith(prefix + "/")}
        folder = self.root / prefix
        actual = {str(p.relative_to(self.root)) for p in folder.rglob("*") if p.is_file()}
        self.check(selected == actual, prefix + ": exact indexed retained file set")
        for relative in sorted(selected):
            path, entry = self.path(self.root, relative), self.index[relative]
            self.check(path.is_file() and path.stat().st_size == entry["bytes"] and self.sha(path) == entry["sha256"],
                       relative + ": indexed bytes/hash")
        return len(selected)

    def inventory(self, entry, label):
        path = self.mapped(entry["path"])
        self.check(path.is_file() and self.sha(path) == entry["sha256"] and path.stat().st_size == entry["size"],
                   label + ": retained executable bytes/hash")
        self.check(bool(path.stat().st_mode & 0o111), label + ": retained executable mode")
        if path not in self.elfs:
            self.elfs[path] = elf_details(path)
        return self.elfs[path]

    def native(self, row, plan_root, logs, executable, input_name, expected_name, label):
        source, expected = self.path(plan_root, input_name), self.path(plan_root, expected_name)
        out, err = (self.path(logs, row[name]) for name in ("stdout", "stderr"))
        self.check(row["pass"] is True and row["exit"] == 0 and row["timeout"] is False, label + ": native exit")
        self.check(row["input"] == input_name and row["expected"] == expected_name, label + ": fixed oracle paths")
        self.check(row["executable"] == row["executable_after"] == executable, label + ": recorded executable identity")
        self.check(row["input_sha256"] == row["input_after_sha256"] == self.sha(source), label + ": input identity")
        self.check(row["stdout_sha256"] == row["expected_stdout_sha256"] == self.sha(out) == self.sha(expected),
                   label + ": stdout hashes")
        self.check(out.read_bytes() == expected.read_bytes() and row["stdout_bytes"] == out.stat().st_size,
                   label + ": exact stdout bytes/length")
        self.check(err.stat().st_size == 0 and row["stderr_sha256"] == self.sha(err), label + ": empty stderr/hash")
        numbers = [row[k] for k in ("wall_seconds", "cpu_user", "cpu_system")]
        self.check(all(type(v) in (int, float) and math.isfinite(v) and v >= 0 for v in numbers)
                   and numbers[0] > 0 and numbers[1] + numbers[2] > 0, label + ": valid process times")
        self.native_counts[label.split(":")[0]] += 1

    def oracle(self, plan_root, name, input_name, expected_name, size, snapshot="base"):
        data = self.path(plan_root, input_name).read_bytes()
        if name == "retained":
            generated = bytes(0 if i % 257 == 256 else (73 * i + 19) % 255 + 1 for i in range(size))
            history, total, result = bytearray(), 0, bytearray()
            for byte in data:
                if byte == 0:
                    history.clear(); total = 0; result.append(0)
                else:
                    history.append(byte); total += byte
                    n = len(history)
                    result.append((total + n + history[(total + n) % n]) % 251)
            expected = bytes(result)
        else:
            _, _, modules, width = CASES[name]
            generated = bytes((73 * i + 19) % 256 for i in range(size))
            key = (name, snapshot)
            if key not in self.lookups:
                # Interpret the scalar call tree, independently of affine-map helpers.
                def node(index, value):
                    for k in range(width):
                        value = (value * (2 + (index * width + k) % 11) + 1 + (index * 7 + k * 3) % 29) % 251
                    for child in (2 * index + 1, 2 * index + 2):
                        if child < modules:
                            value = node(child, value)
                    return value
                bias = 7 if snapshot == "base" else 11
                extra = 3 if snapshot == "public" else 0
                lookup = []
                for byte in range(256):
                    value = byte
                    for i in range(3):
                        value = (value + i + bias + extra) % 251
                    lookup.append(node(0, value))
                self.lookups[key] = bytes(lookup)
            expected = data.translate(self.lookups[key])
        label = name + "/" + snapshot + "/" + str(size)
        self.check(data == generated, label + ": independently reconstructed input")
        self.check(self.path(plan_root, expected_name).read_bytes() == expected, label + ": independent output oracle")

    def stream_hashes(self, row, logs, label):
        for name in ("stdout", "stderr"):
            self.check(self.sha(self.path(logs, row[name])) == row[name + "_sha256"], label + ": " + name + " hash")

    def codegen(self, report, config, label):
        self.check(report["status"] == "Success" and report["verification_runs"] == 1 and report["timings"] is None,
                   label + ": current Verify and successful unprofiled compile")
        native = report["native_codegen"]
        requested, actual, jobs = native["requested_units"], native["actual_units"], native["jobs"]
        self.check(native["backend"] == "llvm-api" and native["closed_executable"] is config["closed_executable"]
                   and requested == config["units"], label + ": requested codegen policy")
        self.check(type(actual) is int and 1 <= actual <= requested and jobs == min(config["jobs"], actual),
                   label + ": actual codegen units/jobs within requested policy")
        objects = native["object_reports"]
        self.check(len(objects) == actual, label + ": one object receipt per actual unit")
        cache_hits = 0
        for i, obj in enumerate(objects):
            if obj["input_kind"] == "object-cache":
                cache_hits += 1
                self.check(obj["backend"] == "llvm-api" and obj["triple"] == "x86_64-unknown-linux-gnu"
                           and obj["llvm_version"] == self.manifest["llvm_version"]
                           and obj["cache"]["state"] == "hit" and obj["cache"]["corrupt"] is False
                           and obj["llvm_verification_this_request"] is False
                           and obj["producer_verified_before_after"] is True
                           and all(obj[k] == 0 for k in ("builder_ns", "parse_ns", "optimize_ns", "emit_ns")),
                           label + "/object-" + str(i) + ": truthful cache-hit producer/current-preflight receipt")
                continue
            self.check(obj["backend"] == "llvm-api" and obj["triple"] == "x86_64-unknown-linux-gnu"
                       and obj["cpu"] == "x86-64" and obj["features"] == ""
                       and obj["pipeline"] == "default<" + config["profile"] + ">"
                       and obj["verify_before_after"] is True and obj["input_kind"] == "builder"
                       and obj["parse_ns"] == 0 and obj["llvm_version"] == self.manifest["llvm_version"],
                       label + "/object-" + str(i) + ": actual target/optimization/verification")
        self.check(cache_hits == native["object_cache"]["hits"], label + ": object-hit receipt count")
        return {k: native.get(k) for k in ("requested_units", "actual_units", "jobs", "fallback", "closed_executable", "internalized_functions")}

    def plan_checks(self, window, plan, rust):
        folder = self.root / "windows" / window
        plan_root = folder / "plan"
        cases = {c["id"]: c for c in plan["cases"]}
        self.check(list(cases) == list(CASES) and len(plan["cases"]) == 5, window + ": exact five-case order")
        self.check(plan["runner_source_commit"] == self.args.expect_candidate, window + ": runner commit")
        identity = {"B": {"commit": self.args.expect_candidate, "crates_tree": self.args.expect_crates_tree}}
        if not rust:
            identity["A"] = {"commit": self.args.expect_base, "crates_tree": self.receipt["base_crates_tree"]}
        self.check(plan["source_identity"] == identity, window + ": source identities")
        runner = "rust_bench.py" if rust else "runtime_bench.py"
        helpers = {"tools/research/compile-performance/" + runner: plan["runner_sha256"], **plan["helper_sha256"]}
        for relative, sha in helpers.items():
            self.check(digest(self.source(relative)) == sha, window + "/" + relative + ": exact candidate source hash")
        roots = ("corpus", "inputs", "expected") if rust else ("sources", "inputs", "expected")
        real_fixtures = {str(p.relative_to(plan_root)): self.sha(p) for prefix in roots
                         for p in (plan_root / prefix).rglob("*") if p.is_file()}
        self.check(real_fixtures == plan["fixtures_sha256"], window + ": complete fixture bytes/hash set")
        self.check(plan["blocks"] == self.manifest["blocks"] == 3 and plan["seed"] == self.manifest["seed"],
                   window + ": frozen blocks and seed")
        self.check(plan["cpu"] in plan["host"]["allowed_cpus"], window + ": permitted recorded runtime CPU")
        for name, case in cases.items():
            weight, size, modules, width = CASES[name]
            self.check(case["weight"] == weight and case["input_bytes"] == size, window + "/" + name + ": fixed weight/input size")
            if rust:
                self.check(case["modules_requested"] == modules and case["width"] == width, window + "/" + name + ": generator dimensions")
            self.oracle(plan_root, name, case["input"], case["expected"], size)
            if rust:
                for sample in case["inputs"]:
                    for snapshot, expected in sample["expected"].items():
                        self.oracle(plan_root, name, sample["path"], expected, sample["bytes"], snapshot)
        self.check((plan_root / "inputs/empty.bin").read_bytes() == (plan_root / "expected/empty.bin").read_bytes() == b"",
                   window + ": empty startup fixtures")
        rng, order = random.Random(plan["seed"]), []
        for block in range(plan["blocks"]):
            names = list(cases); rng.shuffle(names)
            for name in names:
                order.extend({"block": block, "case": name, "position": position, "arm": arm}
                             for position, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB"))
        self.check(order == plan["order"], window + ": independently reconstructed ABBA/BAAB order")
        return cases, order

    def rust_artifacts(self, window, plan, cases):
        folder = self.root / "windows" / window
        pre = folder / "preflight"
        plan_hash = self.sha(folder / "plan/plan.json")
        receipt = load(pre / "result.json")
        build_rows, artifacts = rows(pre / "records.jsonl"), load(pre / "artifacts.json")
        expected = [(name, arm, request) for name in cases for arm in "AB" for request in plan["request_weights"]]
        self.check([(r["case"], r["arm"], r["request"]) for r in build_rows] == expected, window + ": preflight build coverage/order")
        self.check(receipt["pass"] is True and receipt["warmup"] is True and not receipt["failures"]
                   and receipt["rows"] == len(expected) and receipt["plan_sha256"] == plan_hash
                   and receipt["records_sha256"] == self.sha(pre / "records.jsonl")
                   and receipt["artifacts_sha256"] == self.sha(pre / "artifacts.json"), window + ": complete raw preflight receipt")
        self.check([(r["case"], r["arm"]) for r in artifacts] == [(name, arm) for name in cases for arm in "AB"],
                   window + ": exact retained base executable set")
        cold, policies = {}, {}
        for i, row in enumerate(build_rows):
            label = window + ":preflight/" + str(i)
            case, arm = cases[row["case"]], row["arm"]
            snapshot = SNAPSHOTS[row["request"]]
            self.check(row["pass"] is True and row["exit"] == 0 and row["timeout"] is False and row["index"] == i
                       and row["snapshot"] == snapshot and row["plan_sha256"] == plan_hash
                       and row["sample_kind"] == "preflight-unscored", label + ": successful frozen build")
            prefix = case["root"] + "/" + snapshot + ("/rust/" if arm == "A" else "/zil/")
            sources = {p[len(prefix):]: v for p, v in plan["fixtures_sha256"].items() if p.startswith(prefix)}
            self.check(bool(sources) and row["source_before"] == row["source_after"] == sources, label + ": source snapshot identity")
            self.stream_hashes(row, pre / "logs", label)
            self.check(len(row["checks"]) == len(case["inputs"]), label + ": all native correctness checks")
            for check, sample in zip(row["checks"], case["inputs"]):
                self.native(check, folder / "plan", pre / "logs", row["executable"], sample["path"], sample["expected"][snapshot], label)
            argv = row["argv"]
            self.check("--opt" not in argv if arm == "A" else argv[argv.index("--opt") + 1] == plan["profile"], label + ": optimization command")
            if arm == "A":
                for flag in ("target-cpu=x86-64", "opt-level=" + plan["profile"][1:], "debuginfo=0", "overflow-checks=on", "panic=abort"):
                    self.check(flag in argv, label + ": Rust flag " + flag)
                self.check("codegen-units=" + str(plan["units"]) not in argv and "lto=off" not in argv,
                           label + ": native Rust CGU/LTO policy")
            else:
                report = load(self.path(pre / "logs", row["stdout"]))
                config = {k: plan[k] for k in ("profile", "units", "jobs", "closed_executable")}
                policy = self.codegen(report, config, label)
                self.check(report["query_cache"]["requested"] is plan["query_cache"], label + ": requested query cache")
                if row["request"] == "cold_process":
                    policies[row["case"]] = policy
            if row["request"] == "cold_process":
                cold[(row["case"], arm)] = row
        products = {}
        for row in artifacts:
            name, arm = row["case"], row["arm"]
            label = window + ":artifact/" + name + "/" + arm
            exe = row["executable"]
            self.check(exe["sha256"] == cold[(name, arm)]["executable"]["sha256"]
                       and exe["size"] == cold[(name, arm)]["executable"]["size"], label + ": exact cold base product")
            elf = self.inventory(exe, label)
            for stage, input_name, expected_name in (("startup", "inputs/empty.bin", "expected/empty.bin"),
                                                   ("warmup", cases[name]["input"], cases[name]["expected"])):
                self.native(row[stage], folder / "plan", pre / "logs", exe, input_name, expected_name, window + ":" + stage)
            products[(name, arm)] = {"executable": exe, "elf": elf, "codegen": policies.get(name) if arm == "B" else None}
        return products, pre / "result.json"

    def direct_artifacts(self, window, plan, cases):
        folder = self.root / "windows" / window
        build = folder / "build"
        receipt, build_rows = load(build / "result.json"), rows(build / "builds.jsonl")
        plan_hash = self.sha(folder / "plan/plan.json")
        expected = [(name, arm) for name in cases for arm in "AB"]
        self.check([(r["case"], r["arm"]) for r in build_rows] == expected, window + ": exact build order")
        self.check(receipt["pass"] is True and not receipt["failures"] and receipt["runtime_samples"] == 0
                   and receipt["rows"] == len(expected) and receipt["plan_sha256"] == plan_hash
                   and receipt["builds_sha256"] == self.sha(build / "builds.jsonl"), window + ": complete build receipt")
        products = {}
        for row in build_rows:
            name, arm = row["case"], row["arm"]
            label, case, config = window + ":build/" + name + "/" + arm, cases[name], plan["build_config"][arm]
            self.check(row["pass"] is True and row["plan_sha256"] == plan_hash and row["build_process"]["exit"] == 0
                       and row["build_process"]["timeout"] is False, label + ": successful build")
            self.check(row["source_before"] == row["source_after"] == case["source_sha256"], label + ": original source identity")
            self.stream_hashes(row, build / "logs", label)
            report = load(self.path(build / "logs", row["stdout"]))
            policy = self.codegen(report, config, label)
            argv = [plan["binaries"][arm]["path"], "build", str(self.original / "windows" / window / "plan" / case["source"] / "main.zil"),
                    "-o", row["executable"]["path"], "--json", "--backend", "llvm-api", "--opt", config["profile"],
                    "--codegen-units", str(config["units"]), "--codegen-jobs", str(config["jobs"]), "--clang", plan["binaries"]["clang"]["path"]]
            if config["closed_executable"]:
                argv.append("--closed-executable")
            self.check(row["argv"] == row["build_process"]["argv"] == argv, label + ": exact frozen command")
            self.check(report["query_cache"]["requested"] is False and report["disk_cache"] == "disabled"
                       and report["native_codegen"]["object_cache"]["requested"] is False, label + ": disabled caches")
            exe, elf = row["executable"], self.inventory(row["executable"], label)
            for stage, input_name, expected_name in (("startup", "inputs/empty.bin", "expected/empty.bin"),
                                                   ("warmup", case["input"], case["expected"])):
                self.native(row[stage], folder / "plan", build / "logs", exe, input_name, expected_name, window + ":" + stage)
            products[(name, arm)] = {"executable": exe, "elf": elf, "codegen": policy, "llvm_sha256": row["llvm_sha256"]}
        return products, build / "result.json"

    def window(self, name):
        folder = self.root / "windows" / name
        file_count = self.indexed("windows/" + name)
        rust = name != "o2-direct-quality"
        plan_path, records_path = folder / "plan/plan.json", folder / "runtime/records.jsonl"
        plan, observations = load(plan_path), rows(records_path)
        summary_path = folder / ("runtime-summary.json" if rust else "summary.json")
        summary, complete = load(summary_path), load(folder / "runtime/complete.json")
        cases, order = self.plan_checks(name, plan, rust)
        if rust:
            profile, cache, query, closed = {"rust-o0-cold": ("O0", "cold", False, False),
                "rust-o0-incremental": ("O0", "incremental", True, False), "rust-o2-closed": ("O2", "cold", False, True)}[name]
            self.check((plan["profile"], plan["cache_mode"], plan["query_cache"], plan["closed_executable"]) == (profile, cache, query, closed), name + ": frozen window modes")
            self.check(plan["rust_profile"] == plan["rust_lto"] == "native" and plan["target"] == "x86_64-unknown-linux-gnu", name + ": Rust native target/CGU/LTO")
            self.check(plan["request_weights"] == (REQUESTS if cache == "incremental" else {"cold_process": 1.}), name + ": request contract")
            products, artifact_receipt = self.rust_artifacts(name, plan, cases)
            receipt_key = "preflight_sha256"
        else:
            self.check(plan["profile"] == "O2" and plan["object_cache"] is False and plan["timings"] is False, name + ": direct O2 modes")
            for config in plan["build_config"].values():
                self.check(config["profile"] == "O2" and config["closed_executable"] is True
                           and type(config["units"]) is int and config["units"] >= 1
                           and type(config["jobs"]) is int and config["jobs"] >= 1, name + ": explicit closed O2 arm policy")
            products, artifact_receipt = self.direct_artifacts(name, plan, cases)
            receipt_key = "build_receipt_sha256"
        ph, ah, rh = self.sha(plan_path), self.sha(artifact_receipt), self.sha(records_path)
        self.check([{k: r[k] for k in ("block", "case", "position", "arm")} for r in observations] == order,
                   name + ": complete exact runtime order")
        self.check(complete["plan_sha256"] == ph and complete[receipt_key] == ah
                   and complete["records_sha256"] == rh and complete["rows"] == len(order) and not complete["failures"],
                   name + ": completion receipt")
        if rust:
            self.check(complete["pass"] is True and complete["sample_kind"] == "runtime", name + ": runtime receipt kind")
        cells = collections.defaultdict(list)
        for i, row in enumerate(observations):
            case = cases[row["case"]]
            self.check(row["index"] == i and row["plan_sha256"] == ph and row[receipt_key] == ah, name + ": runtime row identity " + str(i))
            if rust:
                self.check(row["sample_kind"] == "runtime", name + ": native sample kind " + str(i))
            self.native(row, folder / "plan", folder / "runtime/logs", products[(row["case"], row["arm"])]["executable"],
                        case["input"], case["expected"], name + ":runtime")
            cells[(row["case"], row["block"], row["arm"])].append(row)
        self.check(len(cells) == len(cases) * plan["blocks"] * 2 and all(len(v) == 2 for v in cells.values()), name + ": two samples per cell")
        by_case, totals = {}, {metric: {"A": 0., "B": 0.} for metric in ("wall", "cpu")}
        for case in cases:
            result = {"products": {arm: products[(case, arm)] for arm in "AB"}, "blocks": []}
            for block in range(plan["blocks"]):
                item = {"block": block}
                for metric in ("wall", "cpu"):
                    values = {arm: statistics.mean(r["wall_seconds"] if metric == "wall" else r["cpu_user"] + r["cpu_system"]
                              for r in cells[(case, block, arm)]) for arm in "AB"}
                    item[metric] = {**values, "B_over_A": values["B"] / values["A"]}
                result["blocks"].append(item)
            result["point_estimates"] = {}
            for metric in ("wall", "cpu"):
                values = {arm: statistics.median(b[metric][arm] for b in result["blocks"]) for arm in "AB"}
                for arm in "AB":
                    totals[metric][arm] += cases[case]["weight"] * values[arm]
                result["point_estimates"][metric] = {**values, "B_over_A": values["B"] / values["A"]}
                for key, value in result["point_estimates"][metric].items():
                    self.check(math.isclose(value, summary[metric]["by_case"][case][key], rel_tol=1e-12, abs_tol=1e-12), name + "/" + case + "/" + metric + ": recomputed " + key)
            a, b = products[(case, "A")], products[(case, "B")]
            result["artifact_comparison"] = {"same_language": not rust,
                "complete_elf_equal": a["executable"]["sha256"] == b["executable"]["sha256"],
                "executable_sections_equal": a["elf"]["executable_sections"] == b["elf"]["executable_sections"],
                "data_sections_equal": a["elf"]["data_sections"] == b["elf"]["data_sections"]}
            if not rust:
                result["artifact_comparison"]["reported_prepartition_llvm_equal"] = a["llvm_sha256"] == b["llvm_sha256"]
            result["all_blocks_wall_and_cpu_above_one"] = all(b[m]["B_over_A"] > 1 for b in result["blocks"] for m in ("wall", "cpu"))
            by_case[case] = result
        for metric, values in totals.items():
            values["B_over_A"] = values["B"] / values["A"]
            for key, value in values.items():
                self.check(math.isclose(value, summary[metric][key], rel_tol=1e-12, abs_tol=1e-12), name + "/" + metric + ": recomputed weighted " + key)
        self.check(summary["plan_sha256"] == ph and summary[receipt_key] == ah and summary["records_sha256"] == rh
                   and summary["rows"] == len(order) and summary["coverage_complete"] is True
                   and summary["completion_receipt_matches"] is True and not summary["failures"]
                   and summary["product_gates"] == "NotRun", name + ": summary identities/completeness/scope")
        self.check(any(item["sha256"] == self.sha(summary_path) for item in self.receipt["windows"]), name + ": root receipt binds summary")
        return {"indexed_files_checked": file_count, "plan_sha256": ph, "receipt_sha256": ah,
                "records_sha256": rh, "summary_sha256": self.sha(summary_path), "runtime_rows": len(observations),
                "actual_cpu": plan["cpu"], "by_case": by_case, "recomputed_weighted_point_estimates": totals}

    def run(self):
        self.manifest = load(self.root / "manifest.json")
        self.check(self.receipt["base_revision"] == self.args.expect_base
                   and self.receipt["candidate_revision"] == self.args.expect_candidate
                   and self.receipt["candidate_crates_tree"] == self.args.expect_crates_tree, "explicit expected source identities")
        self.check(self.receipt["status"] == "Complete_Exploratory_NotAProductGate", "complete root receipt")
        self.check(self.sha(self.root / "manifest.json") == self.receipt["manifest_sha256"]
                   and self.manifest["base_revision"] == self.args.expect_base
                   and self.manifest["candidate_crates_tree"] == self.args.expect_crates_tree, "manifest binding")
        observed_tree = subprocess.check_output(["git", "rev-parse", self.args.expect_candidate + ":crates"], cwd=self.repo, text=True).strip()
        self.check(observed_tree == self.args.expect_crates_tree, "candidate Git crates tree")
        builds = load(self.root / "compiler-builds.json")
        for name, commit in (("baseline", self.args.expect_base), ("candidate", self.args.expect_candidate)):
            build = builds[name]
            self.check(build["source_commit"] == commit, name + ": compiler build commit")
            self.inventory({"path": build["path"], "sha256": build["sha256"], "size": build["bytes"]}, name + " compiler")
        for window in WINDOWS:
            print("auditing " + window, file=sys.stderr, flush=True)
            try:
                self.window_results[window] = self.window(window)
            except (OSError, ValueError, KeyError, IndexError, TypeError, struct.error, subprocess.SubprocessError) as error:
                self.failures.append(window + ": incomplete audit: " + repr(error))
        self.check(set(self.window_results) == set(WINDOWS), "all four runtime windows fully audited")
        try:
            bridge = self.bridge_product_hashes()
        except (OSError, ValueError, KeyError, IndexError, TypeError) as error:
            self.failures.append("O0 bridge artifact comparison incomplete: " + repr(error))
            bridge = None
        return {"schema": "zil-compile5-independent-runtime/1", "reviewer": "/root/ct_opt",
                "reviewed_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                "evidence_root": str(self.root), "original_evidence_root": str(self.original),
                "source_identity": {"base": self.args.expect_base, "candidate": self.args.expect_candidate,
                                    "candidate_crates_tree": self.args.expect_crates_tree},
                "audit_script_sha256": digest(Path(__file__).read_bytes()), "file_index_sha256": self.sha(self.root / "files.json"),
                "audit_valid": not self.failures, "checks": self.checks, "failures": self.failures,
                "native_checks": dict(self.native_counts), "elf_files_parsed": len(self.elfs), "windows": self.window_results,
                "same_configuration_o0_bridge_products": bridge,
                "independence": {"measured_runner_imported": False, "oracle_independently_reconstructed": True,
                                 "artifact_modified": False, "programs_recompiled": False, "native_programs_reexecuted": False,
                                 "reviewer_authored_cp19_source": True},
                "scope": "Independent artifact/oracle/ELF and runtime point-estimate audit; not a non-author CP19 source review. Direct O2 compares different requested codegen configurations and cannot isolate CP18/CP19. O0 bridge compares equal configurations, but its removed executables permit only retained receipt-hash comparison. Bootstrap, transport authenticity and build-cost statistics require separate review. Changed machine bytes are observations requiring interpretation, not automatic correctness failures; no product or whole-language verdict."}

    def bridge_product_hashes(self):
        folder = self.root / "windows/o0-bridge"
        plan = load(folder / "plan/plan.json")
        observations = rows(folder / "build/records.jsonl")
        complete = load(folder / "build/complete.json")
        self.check(plan["profile"] == "O0" and plan["object_cache"] is True and plan["phases"] == ["build"],
                   "O0 bridge: same-configuration build contract")
        self.check(complete["pass"] is True and not complete["failures"]
                   and complete["records_sha256"] == self.sha(folder / "build/records.jsonl")
                   and complete["plan_sha256"] == self.sha(folder / "plan/plan.json"), "O0 bridge: complete raw receipt")
        keys = ("block", "case", "position", "arm", "phase", "request")
        expected = [{**seq, "request": request} for seq in plan["order"] for request in plan["request_weights"]]
        self.check([tuple(r[k] for k in keys) for r in observations] == [tuple(r[k] for k in keys) for r in expected],
                   "O0 bridge: exact four-request paired coverage")
        cells = collections.defaultdict(list)
        for row in observations:
            label = "O0 bridge/" + row["case"] + "/" + row["request"]
            self.check(row["pass"] is True and row["exit"] == 0 and row["timeout"] is False
                       and row["executable_sha256"] == row["executable_after_sha256"], label + ": successful unchanged produced executable")
            self.stream_hashes(row, folder / "build/logs", label)
            report = load(self.path(folder / "build/logs", row["stdout"]))
            self.codegen(report, {"profile": plan["profile"], "units": plan["units"], "jobs": plan["jobs"], "closed_executable": False}, label)
            self.check(report["query_cache"]["requested"] is False, label + ": query cache disabled")
            cells[(row["case"], row["request"], row["arm"])].append(row)
        result = {}
        for case in plan["cases"]:
            name = case["id"]
            result[name] = {}
            for request in plan["request_weights"]:
                arms = {}
                for arm in "AB":
                    group = cells[(name, request, arm)]
                    self.check(len(group) == plan["blocks"] * 2, name + "/" + request + "/" + arm + ": all product receipts")
                    arms[arm] = {"elf_sha256_and_bytes": sorted({(r["executable_sha256"], r["executable_bytes"]) for r in group}),
                                 "prepartition_llvm_sha256": sorted({r["llvm_sha256"] for r in group})}
                result[name][request] = {"arms": arms,
                    "recorded_elf_hashes_and_sizes_equal": arms["A"]["elf_sha256_and_bytes"] == arms["B"]["elf_sha256_and_bytes"],
                    "recorded_prepartition_llvm_hashes_equal": arms["A"]["prepartition_llvm_sha256"] == arms["B"]["prepartition_llvm_sha256"],
                    "elf_bytes_rechecked": False}
        return {"configuration": {k: plan[k] for k in ("profile", "units", "jobs", "object_cache")},
                "scope": "Recorded hash/size comparison only. Runner deletes per-sequence executables; exact retained native-output logs and broad receipt review are separate. Hash differences may reflect path/layout metadata and are not automatic semantic differences.",
                "by_case_request": result}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("evidence", "repo", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("expect-base", "expect-candidate", "expect-crates-tree"):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    if args.out.exists() or args.out.resolve().is_relative_to(args.evidence.resolve()):
        raise SystemExit("Use a new output file outside original evidence")
    audit = Audit(args)
    try:
        result = audit.run()
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        result = {"audit_valid": False, "checks": audit.checks, "failures": audit.failures + [repr(error)],
                  "scope": "Audit incomplete; no quality verdict"}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("x") as stream:
        json.dump(result, stream, indent=2, ensure_ascii=False, allow_nan=False)
        stream.write("\n")
    print(json.dumps({"audit_valid": result["audit_valid"], "checks": result["checks"], "failures": result["failures"], "out": str(args.out)}))
    raise SystemExit(0 if result["audit_valid"] else 1)


if __name__ == "__main__":
    main()
