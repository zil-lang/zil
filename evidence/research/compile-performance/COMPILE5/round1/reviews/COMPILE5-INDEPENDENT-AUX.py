#!/usr/bin/env python3
"""Read-only independent validation of the frozen compile5 auxiliary v2 window.

Does not import a runner/generator, launch compilers, or execute ELF products.
"""
import argparse
from collections import defaultdict
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess

CASES = ("unchanged-wide", "multiple-rewrites", "repeated-hints")
BASE = "a715f828261ff4b6ef9c8125623064360745aae4"
CANDIDATE = "951471d5a6e26e15de96e0bfaf91cd3e537a3dd9"
RUNNER_SHA = "bd317478a54fee5ee99de2a21a6b5bdfb15a2df85b8a31492f4c6968234bf835"
checks = numeric_checks = 0
maximum_difference = 0.


def require(value, where):
    global checks
    checks += 1
    if not value:
        raise ValueError(where)


def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def equal_number(actual, expected, where):
    global numeric_checks, maximum_difference
    require(type(actual) in (int, float) and math.isfinite(actual)
            and math.isclose(actual, expected, rel_tol=2e-12, abs_tol=1e-12), where)
    numeric_checks += 1
    maximum_difference = max(maximum_difference, abs(actual - expected))


def parse_type(text):
    tokens = re.findall(r"i64|CellA|CellB|[(),]", text)
    require("".join(tokens) == text, "Unexpected source type token")
    index = 0

    def parse():
        nonlocal index
        token = tokens[index]
        index += 1
        if token != "(":
            return token
        children = [parse()]
        while tokens[index] == ",":
            index += 1
            children.append(parse())
        require(tokens[index] == ")", "Malformed tuple type")
        index += 1
        return children

    result = parse()
    require(index == len(tokens), "Unconsumed type tokens")
    return result


def type_dimensions(node, path=()):
    if isinstance(node, str):
        return 1, 1, 0, ({path: node} if node != "i64" else {})
    children = [type_dimensions(child, path + (i,)) for i, child in enumerate(node)]
    names = {key: value for child in children for key, value in child[3].items()}
    return 1 + sum(child[0] for child in children), sum(child[1] for child in children), max(len(node), *(child[2] for child in children)), names


def all_one_value(node):
    if isinstance(node, str):
        return "i64(1)" if node == "i64" else node + " { value:i64(1) }"
    return "(" + ",".join(all_one_value(child) for child in node) + ")"


def audit(root, repo):
    probes = root.parent
    planpath = root / "plan/plan.json"
    plan, result = read(planpath), read(root / "result.json")
    meta_path = probes / "frozen-v2/meta.json"
    meta, parent = read(meta_path), read(probes / "frozen-v1/meta.json")
    require(plan["schema"] == "zil-compile5-auxiliary-execution/1" and result["status"] == "Complete_AuxiliaryOnly", "Incomplete/unknown probe")
    require(plan["meta"] == meta and plan["meta_sha256"] == sha(meta_path), "Embedded metadata drift")
    require(meta["blocks"] == 3 and meta["formal_compile_rows"] == 36 and meta["preflight_compilations"] == 6, "Frozen counts changed")
    require(tuple(case["id"] for case in meta["cases"]) == CASES and tuple(case["id"] for case in plan["cases"]) == CASES, "Case set/order changed")
    require(sha(repo / "docs/research/compile-performance/COMPILE5-SUPPLEMENTAL-PLAN.md") == meta["design_sha256"], "Design digest mismatch")
    require(sha(probes / "generate_frozen_probe.py") == meta["generator_sha256"], "Generator digest mismatch")
    require(sha(probes / "run_frozen_probe.py") == plan["runner_sha256"] == RUNNER_SHA, "Execution script differs from reviewed freeze")
    revision = meta["environment_revision"]
    require(revision["parent_meta_sha256"] == sha(probes / "frozen-v1/meta.json")
            and revision["revision_note_sha256"] == sha(probes / "v2-environment-revision.md")
            and revision["main_summaries_read_before_revision"] is False, "Environment revision binding drift")
    require(parent["cases"] == meta["cases"] and parent["files"] == meta["files"] and parent["order"] == meta["order"], "v2 changed source/oracle/order")
    require({k: v for k, v in parent["common_config"].items() if k != "host_affinity"}
            == {k: v for k, v in meta["common_config"].items() if k != "host_affinity"}, "v2 changed non-environment configuration")
    for name, info in meta["files"].items():
        for version in ("frozen-v1", "frozen-v2"):
            path = probes / version / name
            require(sha(path) == info["sha256"] and path.stat().st_size == info["bytes"], "Frozen source/oracle drift: " + version + "/" + name)
    for helper, expected in {"tools/research/compile-performance/bench.py": plan["common_runner_sha256"], **plan["dependencies_sha256"]}.items():
        require(sha(repo / helper) == expected, "Current helper drift: " + helper)
        data = subprocess.check_output(["git", "-C", str(repo), "show", CANDIDATE + ":" + helper])
        require(hashlib.sha256(data).hexdigest() == expected, "Helper differs from measured source: " + helper)
    require(plan["source_identity"] == {
        "A": {"commit": BASE, "crates_tree": "0221fa705e8dc926c54f6b85f2dc9ef3af98559a"},
        "B": {"commit": CANDIDATE, "crates_tree": "b583bc62e4aa39b206afb27f76f2f8262120fe9d"}}, "Compiler source identity drift")
    builds = read(probes / "compiler-builds.json")
    require(builds == plan["compiler_build_receipt"] and sha(probes / "compiler-builds.json") == plan["compiler_build_receipt_sha256"], "CI compiler receipt drift")
    tool_results = []
    for tool in [*plan["binaries"].values(), *plan["extra_tools"]]:
        path = Path(tool["path"])
        stat = path.stat()
        require(sha(path) == tool["sha256"] and stat.st_size == tool["size"], "Compiler/tool bytes drift: " + str(path))
        tool_results.append({"path": str(path), "sha256": tool["sha256"], "size": tool["size"],
                             "current_metadata_matches": stat.st_mtime_ns == tool["mtime_ns"] and stat.st_ctime_ns == tool["ctime_ns"]})
    for arm, label, commit in (("A", "baseline", BASE), ("B", "candidate", CANDIDATE)):
        build, binary = builds[label], plan["binaries"][arm]
        require(build["source_commit"] == commit and build["sha256"] == binary["sha256"] and build["bytes"] == binary["size"], "CI binary/source binding mismatch")
        loader = plan["loader_receipts"][arm]
        paths = [line.split("=>", 1)[1].split()[0] for line in loader["stdout"].splitlines() if line.lstrip().startswith("libLLVM") and "=>" in line]
        require(loader["exit"] == 0 and paths == [plan["extra_tools"][0]["path"]], "Recorded loaded LLVM differs")
    require(Path(plan["linker_invocation"]).resolve() == Path(plan["extra_tools"][1]["path"]), "Linker invocation alias changed")
    require(all(version["exit"] == 0 for version in plan["versions"].values())
            and plan["versions"]["llvm"]["stdout"].strip() == "23.1.4", "Version evidence failed")
    require(plan["cpus"] == [0, 1] and plan["profile"] == "O0" and plan["units"] == 8 and plan["jobs"] == 4
            and plan["object_cache"] is True, "Execution configuration drift")
    case_orders = (CASES, (CASES[1], CASES[2], CASES[0]), (CASES[2], CASES[0], CASES[1]))
    expected_order = [{"block": block, "case": case, "position": pos, "arm": arm}
                      for block, names in enumerate(case_orders) for case in names
                      for pos, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB")]
    require(meta["order"] == expected_order and plan["order"] == [{**item, "phase": "build"} for item in expected_order], "Frozen full order differs")
    corpus = root / "plan/corpus"
    observed_fixtures = {str(path.relative_to(corpus)): sha(path) for path in corpus.rglob("*") if path.is_file()}
    require(observed_fixtures == plan["fixtures_sha256"], "Execution fixture set/bytes drift")
    oracles, source_proofs = {}, {}
    for case in CASES:
        folder = corpus / case
        for name in ("main.zil", "work.zil", "stdin.bin", "stdout.bin", "stderr.bin"):
            path = folder / "base/zil" / name if name.endswith(".zil") else folder / name
            require(sha(path) == meta["files"][case + "/" + name]["sha256"], "Executed fixture differs from frozen design")
        main = (folder / "base/zil/main.zil").read_text()
        source = (folder / "base/zil/work.zil").read_text()
        require("work.run()" in main and "putchar(i32(result%251))" in main and "return 0;" in main
                and "#" not in main + source, "Wrong source/oracle boundary")
        if case == "repeated-hints":
            first = source.splitlines()[0]
            raw_type = first.removeprefix("fn consume(p:").split(")->i64", 1)[0]
            node = parse_type(raw_type)
            require("return p.0.0.0+p.1.3.1+p.3.3.1;" in first, "Repeated hint oracle selects wrong leaves")
            require(source.count("total += consume(p);") == 128
                    and f"let p:{raw_type}={all_one_value(node)};" in source, "Repeated hint count/value drift")
            count = 128
        else:
            functions = re.findall(r"^fn f(\d+)\(\)->(.+?) \{ return (.+); \}$", source, re.M)
            require([int(f[0]) for f in functions] == list(range(64)), "Return function count/order drift")
            node = parse_type(functions[0][1])
            require(all(t == functions[0][1] and v == all_one_value(node) for _, t, v in functions), "Return type/value shape drift")
            suffix = ".value" if case == "multiple-rewrites" else ""
            for index in range(64):
                require(f"let v{index}=f{index}();" in source and
                        f"total += v{index}.0.0.0+v{index}.1.3.1{suffix}+v{index}.3.3.1{suffix};" in source,
                        "Return value not routed into oracle")
            count = 64
        nodes, leaves, arity, nominal = type_dimensions(node)
        require((nodes, leaves, arity) == (53, 32, 4), "Compound type dimensions changed")
        require(nominal == ({(1, 3, 1): "CellA", (3, 3, 1): "CellB"} if case == "multiple-rewrites" else {}), "Nominal rewrite positions differ")
        expected = bytes([(count * 3) % 251])
        require((folder / "stdout.bin").read_bytes() == expected and (folder / "stdin.bin").read_bytes() == b""
                and (folder / "stderr.bin").read_bytes() == b"", "Independent mathematical oracle mismatch")
        oracles[case] = expected
        source_proofs[case] = {"raw_type_nodes": nodes, "resolved_type_nodes": nodes + len(nominal), "leaves": leaves,
                              "max_tuple_arity": arity, "selected_value_sum": count * 3, "expected_stdout_bytes": list(expected)}
    measured = []
    elf_sets, llvm_sets, llvm_body_sets, units = defaultdict(set), defaultdict(set), defaultdict(set), defaultdict(set)
    all_rows = 0
    for section in ("preflight", "measure"):
        folder = root / section
        records_path = folder / "records.jsonl"
        rows = [json.loads(line) for line in records_path.read_text().splitlines()]
        receipt = read(folder / "complete.json")
        order = ([{"block": 0, "case": case, "position": pos, "arm": arm, "phase": "build"}
                  for case in CASES for pos, arm in enumerate("AB")] if section == "preflight" else plan["order"])
        require(receipt["pass"] is True and receipt["failures"] == [] and receipt["rows"] == len(order) == len(rows)
                and receipt["records_sha256"] == sha(records_path) and receipt["plan_sha256"] == sha(planpath), "Completion/order coverage mismatch: " + section)
        if section == "measure":
            require(result["measure"] == receipt, "Result is not bound to completion receipt")
            measured = rows
        for index, (row, scheduled) in enumerate(zip(rows, order)):
            all_rows += 1
            require(all(row[key] == value for key, value in scheduled.items()) and row["index"] == index
                    and row["plan_sha256"] == sha(planpath), "Raw row ordering/index/plan mismatch")
            require(row["pass"] is True and row["exit"] == 0 and row["timeout"] is False
                    and row["request"] == "cold_process" and row["snapshot"] == "base", "Failed/noncold sample")
            require(all(type(row[key]) in (int, float) and math.isfinite(row[key]) and row[key] >= 0
                        for key in ("wall_seconds", "cpu_user", "cpu_system", "max_process_rss_kib")) and row["wall_seconds"] > 0, "Invalid metric")
            equal_number(row["cpu_seconds"], row["cpu_user"] + row["cpu_system"], "CPU sum differs")
            work = root / section / ("work-" + str(index))
            argv = [plan["binaries"][row["arm"]]["path"], "build", str(work / "src/main.zil"), "--json", "--cache-dir", str(work / "source-candidates"),
                    "--clang", plan["binaries"]["clang"]["path"], "--opt", "O0", "--codegen-units", "8", "--codegen-jobs", "4",
                    "-o", str(work / "program"), "--object-cache", str(work / "objects")]
            require(row["argv"] == argv and row["work"] == str(work), "Compiler command differs")
            source = corpus / row["case"] / "base/zil"
            source_hashes = {path.name: sha(path) for path in source.iterdir() if path.is_file()}
            require(row["source_before"] == row["source_after"] == source_hashes, "Source changed within sample")
            for stream in ("stdout", "stderr"):
                require(sha(folder / "logs" / row[stream]) == row[stream + "_sha256"], "Raw compiler log digest mismatch")
            report = read(folder / "logs" / row["stdout"])
            for key, value in row.items():
                if key.startswith("reported_"):
                    require(value == report.get(key.removeprefix("reported_")), "Stored CLI excerpt differs")
            native, query = report["native_codegen"], report["query_cache"]
            cache = native["object_cache"]
            require(report["status"] == "Success" and report["verification_runs"] == 1 and report["timings"] is None
                    and report["diagnostics"] == [] and report["disk_cache"] == "missing", "Current verification/cache contract differs")
            require(native["backend"] == "llvm-api" and native["requested_units"] == 8 and native["closed_executable"] is False
                    and cache["requested"] is True and cache["enabled"] is True and cache["hits"] == cache["corrupt"] == cache["write_failures"] == 0
                    and cache["misses"] == cache["stored"] == native["actual_units"] and cache["disabled_reason"] is None, "Unexpected native/object-cache mode")
            require(query["requested"] is False and query["state"] == "disabled" and query["stored"] is False, "Unexpected query cache")
            require(len(native["object_reports"]) == native["actual_units"] and all(obj["llvm_version"] == "23.1.4" and obj["triple"] == "x86_64-unknown-linux-gnu"
                        and obj["cpu"] == "x86-64" and obj["features"] == "" and obj["pipeline"] == "default<O0>"
                        and obj["input_kind"] == "builder" and obj["verify_before_after"] is True for obj in native["object_reports"]), "Codegen target/pipeline/verification differs")
            commands = report["commands"]
            require(len(commands) == 1 and commands[0][0] == plan["binaries"]["clang"]["path"]
                    and "--ld-path=" + plan["linker_invocation"] in commands[0]
                    and row["actual_linker_contract"] is True, "Actual linker mismatch")
            require(len(row["checks"]) == 1, "Missing or excess oracle rows")
            oracle = row["checks"][0]
            input_path = "corpus/" + row["case"] + "/stdin.bin"
            output_path = "corpus/" + row["case"] + "/stdout.bin"
            require(oracle["matches"] is True and oracle["exit"] == 0 and oracle["timeout"] is False
                    and oracle["input"] == input_path and oracle["expected"] == output_path, "Native oracle failed")
            require(oracle["input_sha256"] == oracle["input_after_sha256"] == sha(root / "plan" / input_path)
                    and oracle["expected_stdout_sha256"] == oracle["stdout_sha256"] == sha(root / "plan" / output_path), "Oracle binding mismatch")
            for stream in ("stdout", "stderr"):
                require(sha(folder / "logs" / oracle[stream]) == oracle[stream + "_sha256"], "Native log digest mismatch")
            require((folder / "logs" / oracle["stdout"]).read_bytes() == oracles[row["case"]]
                    and (folder / "logs" / oracle["stderr"]).stat().st_size == 0, "Native output differs from independent oracle")
            require(row["retained_elf"] == f"{section}/products/{index:03d}" and row["retained_llvm"] == f"{section}/products/{index:03d}.ll", "Product retention index mismatch")
            elf, llvm = root / row["retained_elf"], root / row["retained_llvm"]
            require(sha(elf) == row["executable_sha256"] == row["executable_after_sha256"]
                    and elf.stat().st_size == row["executable_bytes"] and sha(llvm) == row["llvm_sha256"], "Retained ELF/LLVM bytes differ")
            elf_sets[row["case"]].add((sha(elf), elf.stat().st_size))
            llvm_sets[row["case"]].add(sha(llvm))
            raw_ir = llvm.read_bytes()
            # Preserve each raw hash above. Only the emitted verification
            # fingerprint comment is excluded from this separate body check.
            head, fingerprint, body = raw_ir.split(b"\n", 2)
            require(head == b"; ZIL verified LLVM module" and re.fullmatch(rb"; verified-fingerprint: [0-9a-f]{64}", fingerprint), "Unexpected LLVM provenance header")
            llvm_body_sets[row["case"]].add(hashlib.sha256(head + b"\n" + body).hexdigest())
            units[row["case"]].add((native["actual_units"], native["jobs"]))
    by_case = {}
    for case in CASES:
        metrics = {}
        for metric in ("wall_seconds", "cpu_seconds", "max_process_rss_kib"):
            means = {arm: [] for arm in "AB"}
            for block in range(3):
                for arm in "AB":
                    selected = [row for row in measured if (row["case"], row["block"], row["arm"]) == (case, block, arm)]
                    require(len(selected) == 2, "Incorrect per-block arm sample count")
                    values = [row["cpu_user"] + row["cpu_system"] if metric == "cpu_seconds" else row[metric] for row in selected]
                    means[arm].append(math.fsum(values) / 2)
            a, b = sorted(means["A"])[1], sorted(means["B"])[1]
            ratio = b / a
            observed = result["by_case"][case][metric]
            for key, value in (("A", a), ("B", b), ("B_over_A", ratio)):
                equal_number(observed[key], value, "Per-case statistic differs")
            for arm in "AB":
                for actual, expected in zip(observed["block_means"][arm], means[arm]):
                    equal_number(actual, expected, "Block mean differs")
            block_ratios = [means["B"][i] / means["A"][i] for i in range(3)]
            metrics[metric] = {"A": a, "B": b, "B_over_A": ratio, "block_means": means, "block_B_over_A": block_ratios,
                               "block_directions": ["B_higher" if value > 1 else "B_lower" if value < 1 else "equal" for value in block_ratios]}
        require(len(elf_sets[case]) == len(llvm_body_sets[case]) == 1, "Per-case ELF or LLVM body changed across arms/samples")
        by_case[case] = {**metrics, "retained_elf_sha256": next(iter(elf_sets[case]))[0],
                         "retained_elf_bytes": next(iter(elf_sets[case]))[1], "raw_llvm_distinct_sha256_count": len(llvm_sets[case]),
                         "llvm_body_without_verification_comment_sha256": next(iter(llvm_body_sets[case])),
                         "actual_units_and_jobs": sorted(units[case])}
    return {"status": "IndependentAuxiliaryEvidenceAndArithmeticPass_RiskSignalOnly", "base_revision": BASE,
            "candidate_revision": CANDIDATE, "plan_sha256": sha(planpath), "meta_sha256": sha(meta_path),
            "result_sha256": sha(root / "result.json"), "runner_sha256": RUNNER_SHA,
            "preflight_rows": 6, "measurement_rows": 36, "native_oracles_verified": all_rows,
            "retained_elf_verified": all_rows, "retained_llvm_verified": all_rows,
            "checks": checks, "numeric_comparisons": numeric_checks, "maximum_absolute_numeric_difference": maximum_difference,
            "host": plan["host"], "selected_cpus": plan["cpus"], "tools": tool_results, "source_oracles": source_proofs,
            "by_case": by_case,
            "estimator": "Within each case, mean two observations per arm/block, median the three arm means, then B/A; no case weights or pooled score",
            "limitations": ["Only three independent blocks; each case has mixed wall-time directions",
                            "Raw .ll hashes differ only at the exact verified-fingerprint provenance comment; each retained raw file was hashed against its own record, then all remaining bytes were compared",
                            "No measured memo-hit count or isolated CP18/CP19 ablation; repeated-hints does not establish memo speedup",
                            "Local EPYC 9V74 auxiliary host differs from CI EPYC 7763; never combine absolute times or scores",
                            "max_process_rss_kib is wait4 maximum individual-process RSS, not simultaneous whole-process-tree peak",
                            "Main CI improvement cannot discharge the compound-type coverage risk; no retry/round selection was performed by this reviewer"],
            "reviewer_script_sha256": sha(Path(__file__))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = audit(args.root.resolve(), args.repo.resolve())
    except (OSError, ValueError, KeyError, IndexError, TypeError, subprocess.SubprocessError) as error:
        result = {"status": "IndependentAuxiliaryAuditFailed_NoScore", "error": repr(error)}
    args.out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k not in ("by_case", "tools", "source_oracles")}, ensure_ascii=False, indent=2))
    return int(result["status"] != "IndependentAuxiliaryEvidenceAndArithmeticPass_RiskSignalOnly")


if __name__ == "__main__":
    raise SystemExit(main())
