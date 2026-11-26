#!/usr/bin/env python3
"""Run and receipt all 8 everyday apps and 12 Alpha functional capability suites.

Executes real dual-backend (`--features llvm-api`, Clang + LLVM API at O0/O2)
Rust/ZIL integration test binaries on the exact frozen commit and emits two
fail-closed JSON receipts:
  - everyday-apps-receipt.json  (all 8 apps declared in validation/language-alpha-profile.json)
  - capabilities-receipt.json   (all 12 capabilities declared in validation/language-release-gates.json)
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


REQUIRED_BACKENDS = ["clang-o0", "clang-o2", "llvmapi-o0", "llvmapi-o2"]

# Authoritative mapping from each declared everyday application ID in
# validation/language-alpha-profile.json to its dual-backend native test binary
# and required test functions (functional + cleanup/negative controls).
EVERYDAY_APP_TESTS: dict[str, tuple[str, str, list[str]]] = {
    "text-stats-and-file-search": (
        "zil-driver",
        "everyday_apps_core_native",
        [
            "everyday_app_text_stats_and_file_search_dual_backend",
            "everyday_apps_oom_injection_cleanup_dual_backend",
        ],
    ),
    "csv-processor": (
        "zil-driver",
        "everyday_apps_core_native",
        [
            "everyday_app_csv_processor_dual_backend",
            "everyday_apps_oom_injection_cleanup_dual_backend",
        ],
    ),
    "json-processor": (
        "zil-driver",
        "everyday_apps_core_native",
        [
            "everyday_app_json_processor_dual_backend",
            "everyday_apps_oom_injection_cleanup_dual_backend",
        ],
    ),
    "cli-todo-app": (
        "zil-driver",
        "everyday_apps_core_native",
        [
            "everyday_app_cli_todo_app_dual_backend",
            "everyday_apps_oom_injection_cleanup_dual_backend",
        ],
    ),
    "http-mini-service-and-client": (
        "zil-driver",
        "everyday_apps_systems_native",
        [
            "http_mini_service_and_client_dual_backend_native",
            "everyday_apps_paired_nearest_safety_negative_controls",
        ],
    ),
    "file-batch-processor": (
        "zil-driver",
        "everyday_apps_systems_native",
        [
            "file_batch_processor_dual_backend_native",
            "everyday_apps_paired_nearest_safety_negative_controls",
        ],
    ),
    "graph-dependency-analyzer": (
        "zil-driver",
        "everyday_apps_systems_native",
        [
            "graph_dependency_analyzer_dual_backend_native",
            "everyday_apps_paired_nearest_safety_negative_controls",
        ],
    ),
    "async-task-scheduler": (
        "zil-driver",
        "everyday_app_async_scheduler_native",
        [
            "async_task_scheduler_demo_batch_and_cancel_stress_dual_backend_native",
            "async_task_scheduler_paired_nearest_safety_negative_controls",
        ],
    ),
}

# Authoritative mapping from each of the 12 declared capability IDs in
# validation/language-release-gates.json to real executable integration suites
# run with `--features llvm-api`.
CAPABILITY_TARGETS: dict[str, list[tuple[str, str]]] = {
    "values-and-patterns": [
        ("zil-driver", "l1_core_composition_native"),
        ("zil-driver", "u2_anonymous_union_and_owned_sum_native"),
        ("zil-driver", "u3_destructuring_and_partial_drop_native"),
        ("zil-driver", "l4_abi_and_float_native"),
    ],
    "generic-libraries": [
        ("zil-driver", "l1_generics_default_methods_native"),
        ("zil-driver", "u4_traits_supertraits_and_assoc_native"),
        ("zil-driver", "l4_iter_and_lending_native"),
    ],
    "ownership-composition": [
        ("zil-driver", "l1_borrowed_ownership_native"),
        ("zil-driver", "u5_field_borrows_and_closure_captures_native"),
        ("zil-driver", "multi_input_borrows_and_generic_lending_native"),
        ("zil-driver", "contract_state_protocol_native"),
        ("zil-driver", "contract_replace_protocol_native"),
        ("zil-driver", "contract_store_protocol_native"),
        ("zil-driver", "contract_managed_and_nested_alias_native"),
        ("zil-driver", "contract_projection_and_effects_native"),
    ],
    "dynamic-abstraction": [
        ("zil-driver", "l2_borrowed_dyn_provider_native"),
        ("zil-driver", "u6_dyn_ct_and_owning_std_native"),
    ],
    "resource-graphs": [
        ("zil-driver", "u6_dyn_ct_and_owning_std_native"),
        ("zil-driver", "everyday_apps_systems_native"),
    ],
    "async-ownership": [
        ("zil-driver", "l3_task_transfer_and_await_native"),
        ("zil-driver", "l3_pending_exit_and_task_closure_native"),
        ("zil-driver", "contract_task_registration_protocol_native"),
        ("zil-driver", "contract_cancel_edge_protocol_native"),
    ],
    "threads-and-synchronization": [
        ("zil-driver", "l3_sync_atomics_and_mutex_native"),
        ("zil-driver", "l3_async_registration_and_thread_native"),
    ],
    "foreign-lifecycle": [
        ("zil-driver", "foreign_registration_native"),
        ("zil-driver", "l3_scoped_foreign_callback_native"),
    ],
    "standard-library": [
        ("zil-driver", "l4_stdlib_native"),
        ("zil-driver", "alpha_everyday_stdlib_native"),
        ("zil-driver", "stdlib_fs_and_path_native"),
        ("zil-driver", "stdlib_net_tcp_native"),
    ],
    "project-workflow": [
        ("zil-cli", "single_pipeline_cli"),
        ("zil-cli", "backend_default"),
        ("zil-cli", "qc"),
    ],
    "native-delivery": [
        ("zil-driver", "alpha_e2e_acceptance"),
        ("zil-driver", "alpha_everyday_ergonomics_native"),
    ],
    "release-acceptance": [
        ("zil-driver", "l3_provenance_closure"),
        ("zil-driver", "alpha_e2e_acceptance"),
    ],
}

# Windows x64 dual-backend (`--features llvm-api`, Clang + LLVM-C.dll at O0/O2)
# mapping covering all 8 everyday applications declared in
# validation/language-alpha-profile.json and all 12 capabilities declared in
# validation/language-release-gates.json without relying on ELF-only linker wraps
# or POSIX pthread headers.
WINDOWS_EVERYDAY_APP_TESTS: dict[str, tuple[str, str, list[str]]] = {
    "text-stats-and-file-search": (
        "zil-driver",
        "everyday_apps_core_native",
        [
            "everyday_app_text_stats_and_file_search_dual_backend",
            "everyday_apps_oom_injection_cleanup_dual_backend",
        ],
    ),
    "csv-processor": (
        "zil-driver",
        "everyday_apps_core_native",
        [
            "everyday_app_csv_processor_dual_backend",
            "everyday_apps_oom_injection_cleanup_dual_backend",
        ],
    ),
    "json-processor": (
        "zil-driver",
        "everyday_apps_core_native",
        [
            "everyday_app_json_processor_dual_backend",
            "everyday_apps_oom_injection_cleanup_dual_backend",
        ],
    ),
    "cli-todo-app": (
        "zil-driver",
        "everyday_apps_core_native",
        [
            "everyday_app_cli_todo_app_dual_backend",
            "everyday_apps_oom_injection_cleanup_dual_backend",
        ],
    ),
    "http-mini-service-and-client": (
        "zil-driver",
        "everyday_apps_systems_native",
        [
            "http_mini_service_and_client_dual_backend_native",
            "everyday_apps_paired_nearest_safety_negative_controls",
        ],
    ),
    "file-batch-processor": (
        "zil-driver",
        "everyday_apps_systems_native",
        [
            "file_batch_processor_dual_backend_native",
            "everyday_apps_paired_nearest_safety_negative_controls",
        ],
    ),
    "graph-dependency-analyzer": (
        "zil-driver",
        "everyday_apps_systems_native",
        [
            "graph_dependency_analyzer_dual_backend_native",
            "everyday_apps_paired_nearest_safety_negative_controls",
        ],
    ),
    "async-task-scheduler": (
        "zil-driver",
        "everyday_app_async_scheduler_native",
        [
            "async_task_scheduler_demo_batch_and_cancel_stress_dual_backend_native",
            "async_task_scheduler_paired_nearest_safety_negative_controls",
        ],
    ),
}

WINDOWS_CAPABILITY_TARGETS: dict[str, list[tuple[str, str]]] = {
    "values-and-patterns": [
        ("zil-driver", "l1_core_composition_native"),
        ("zil-driver", "u2_anonymous_union_and_owned_sum_native"),
        ("zil-driver", "u3_destructuring_and_partial_drop_native"),
        ("zil-driver", "win64_abi_and_runtime_native"),
    ],
    "generic-libraries": [
        ("zil-driver", "l1_generics_default_methods_native"),
        ("zil-driver", "u4_traits_supertraits_and_assoc_native"),
        ("zil-driver", "l4_iter_and_lending_native"),
    ],
    "ownership-composition": [
        ("zil-driver", "l1_borrowed_ownership_native"),
        ("zil-driver", "u5_field_borrows_and_closure_captures_native"),
        ("zil-driver", "multi_input_borrows_and_generic_lending_native"),
        ("zil-driver", "contract_state_protocol_native"),
        ("zil-driver", "contract_replace_protocol_native"),
        ("zil-driver", "contract_store_protocol_native"),
        ("zil-driver", "contract_managed_and_nested_alias_native"),
        ("zil-driver", "contract_projection_and_effects_native"),
    ],
    "dynamic-abstraction": [
        ("zil-driver", "l2_borrowed_dyn_provider_native"),
        ("zil-driver", "u6_dyn_ct_and_owning_std_native"),
    ],
    "resource-graphs": [
        ("zil-driver", "u6_dyn_ct_and_owning_std_native"),
        ("zil-driver", "everyday_apps_systems_native"),
    ],
    "async-ownership": [
        ("zil-driver", "contract_task_registration_protocol_native"),
        ("zil-driver", "contract_cancel_edge_protocol_native"),
        ("zil-driver", "win64_concurrency_and_async_native"),
    ],
    "threads-and-synchronization": [
        ("zil-driver", "l3_sync_atomics_and_mutex_native"),
        ("zil-driver", "win64_concurrency_and_async_native"),
    ],
    "foreign-lifecycle": [
        ("zil-driver", "l3_scoped_foreign_callback_native"),
        ("zil-driver", "win64_abi_and_runtime_native"),
        ("zil-driver", "win64_concurrency_and_async_native"),
    ],
    "standard-library": [
        ("zil-driver", "l4_stdlib_native"),
        ("zil-driver", "stdlib_fs_and_path_native"),
        ("zil-driver", "stdlib_net_tcp_native"),
        ("zil-driver", "win64_stdlib_and_apps_native"),
    ],
    "project-workflow": [
        ("zil-cli", "single_pipeline_cli"),
        ("zil-cli", "backend_default"),
        ("zil-driver", "win64_pipeline_native"),
    ],
    "native-delivery": [
        ("zil-driver", "alpha_everyday_ergonomics_native"),
        ("zil-driver", "win64_pipeline_native"),
        ("zil-driver", "win64_abi_and_runtime_native"),
    ],
    "release-acceptance": [
        ("zil-driver", "l3_provenance_closure"),
        ("zil-driver", "win64_concurrency_and_async_native"),
        ("zil-driver", "win64_stdlib_and_apps_native"),
    ],
}

TEST_LINE_RE = re.compile(r"^test\s+(\S+)\s+\.\.\.\s+(ok|FAILED|ignored)\s*$")
SUMMARY_LINE_RE = re.compile(
    r"^test result:\s+(ok|FAILED)\.\s+(\d+)\s+passed;\s+(\d+)\s+failed;\s+(\d+)\s+ignored;"
)


def parse_libtest_stdout(stdout: str, exit_code: int) -> dict[str, Any]:
    passed_tests: list[str] = []
    failed_tests: list[str] = []
    ignored_tests: list[str] = []
    summary_seen = False
    summary_passed = 0
    summary_failed = 0
    summary_ignored = 0
    summary_ok = False

    for raw_line in stdout.splitlines():
        line = raw_line.strip()
        m = TEST_LINE_RE.match(line)
        if m:
            name, outcome = m.group(1), m.group(2)
            if outcome == "ok":
                passed_tests.append(name)
            elif outcome == "FAILED":
                failed_tests.append(name)
            else:
                ignored_tests.append(name)
            continue
        sm = SUMMARY_LINE_RE.match(line)
        if sm:
            summary_seen = True
            summary_ok = sm.group(1) == "ok"
            summary_passed = int(sm.group(2))
            summary_failed = int(sm.group(3))
            summary_ignored = int(sm.group(4))

    total_parsed = len(passed_tests) + len(failed_tests) + len(ignored_tests)
    counts_match = (
        summary_seen
        and len(passed_tests) == summary_passed
        and len(failed_tests) == summary_failed
        and len(ignored_tests) == summary_ignored
    )
    ok = (
        exit_code == 0
        and summary_ok
        and counts_match
        and summary_passed > 0
        and summary_failed == 0
        and summary_ignored == 0
    )
    return {
        "status": "pass" if ok else "fail",
        "exit_code": exit_code,
        "tests_executed": total_parsed,
        "tests_passed": len(passed_tests),
        "tests_failed": len(failed_tests),
        "tests_ignored": len(ignored_tests),
        "passed_tests": sorted(passed_tests),
        "failed_tests": sorted(failed_tests),
        "ignored_tests": sorted(ignored_tests),
        "summary_seen": summary_seen,
        "counts_match": counts_match,
    }


def _cargo_feature_args(pkg: str, llvm_api: bool) -> list[str]:
    if llvm_api:
        if pkg == "zil-driver":
            return ["--features", "llvm-api"]
        # zil-cli enables llvm-api by default.
        return []
    return ["--no-default-features"]


def _resolve_platform(platform: str) -> str:
    if platform == "auto":
        return "windows" if os.name == "nt" else "linux"
    if platform not in ("linux", "windows"):
        raise ValueError(f"unsupported platform `{platform}`")
    return platform


def _platform_maps(
    platform: str,
) -> tuple[dict[str, tuple[str, str, list[str]]], dict[str, list[tuple[str, str]]]]:
    resolved = _resolve_platform(platform)
    if resolved == "windows":
        return WINDOWS_EVERYDAY_APP_TESTS, WINDOWS_CAPABILITY_TARGETS
    return EVERYDAY_APP_TESTS, CAPABILITY_TARGETS


def _unique_targets_by_package(platform: str = "linux") -> dict[str, list[str]]:
    app_tests, cap_targets = _platform_maps(platform)
    by_pkg: dict[str, set[str]] = {}
    for pkg, target, _ in app_tests.values():
        by_pkg.setdefault(pkg, set()).add(target)
    for targets in cap_targets.values():
        for pkg, target in targets:
            by_pkg.setdefault(pkg, set()).add(target)
    return {pkg: sorted(targets) for pkg, targets in sorted(by_pkg.items())}


def _has_llvm_api_dso(platform: str = "auto") -> bool:
    libdir = os.environ.get("ZIL_LLVM_LIB_DIR", "")
    if not libdir:
        return False
    resolved = _resolve_platform(platform)
    if resolved == "windows":
        lib_ok = (Path(libdir) / "LLVM-C.lib").is_file()
        dll_next = (Path(libdir).parent / "bin" / "LLVM-C.dll").is_file()
        dll_path = shutil.which("LLVM-C.dll") is not None
        return lib_ok and (dll_next or dll_path)
    return (Path(libdir) / "libLLVM-23.so").is_file()


def _prebuild_targets(repo: Path, by_pkg: dict[str, list[str]], llvm_api: bool) -> None:
    for pkg, targets in by_pkg.items():
        cmd = ["cargo", "test", "--locked", "--no-run", "-p", pkg]
        cmd.extend(_cargo_feature_args(pkg, llvm_api))
        for target in targets:
            cmd.extend(["--test", target])
        print(f"+ {' '.join(cmd)}", flush=True)
        subprocess.run(cmd, cwd=repo, check=True)


def _run_target(repo: Path, pkg: str, target: str, llvm_api: bool) -> dict[str, Any]:
    cmd = ["cargo", "test", "--locked", "-p", pkg]
    cmd.extend(_cargo_feature_args(pkg, llvm_api))
    cmd.extend(["--test", target])
    print(f"+ {' '.join(cmd)}", flush=True)
    proc = subprocess.run(cmd, cwd=repo, text=True, capture_output=True, check=False)
    parsed = parse_libtest_stdout(proc.stdout, proc.returncode)
    parsed["package"] = pkg
    parsed["target"] = target
    if parsed["status"] != "pass":
        sys.stderr.write(f"FAILED target {pkg}::{target} (exit {proc.returncode})\n")
        if proc.stdout:
            sys.stderr.write(proc.stdout[-4000:] + "\n")
        if proc.stderr:
            sys.stderr.write(proc.stderr[-4000:] + "\n")
    return parsed


def build_receipts(
    repo: Path,
    source_commit: str,
    source_tree: str,
    llvm_api: bool,
    target_results: dict[tuple[str, str], dict[str, Any]],
    platform: str = "linux",
) -> tuple[dict[str, Any], dict[str, Any]]:
    resolved_platform = _resolve_platform(platform)
    app_tests, cap_targets = _platform_maps(resolved_platform)
    profile = json.loads((repo / "validation/language-alpha-profile.json").read_text(encoding="utf-8"))
    gates = json.loads((repo / "validation/language-release-gates.json").read_text(encoding="utf-8"))

    declared_app_ids = [app["id"] for app in profile["everyday_applications"]["applications"]]
    declared_cap_ids = [cap["id"] for cap in gates["capabilities"]]
    backends = list(REQUIRED_BACKENDS) if llvm_api else ["clang-o0", "clang-o2"]

    app_entries: list[dict[str, Any]] = []
    apps_all_pass = True
    for app_id in declared_app_ids:
        mapping = app_tests.get(app_id)
        if mapping is None:
            apps_all_pass = False
            app_entries.append(
                {
                    "id": app_id,
                    "status": "fail",
                    "error": f"unmapped everyday app `{app_id}`",
                    "backends": backends,
                    "tests_executed": 0,
                    "tests_passed": 0,
                    "tests_failed": 0,
                    "tests_ignored": 0,
                    "required_tests": [],
                }
            )
            continue
        pkg, target, req_tests = mapping
        res = target_results.get((pkg, target))
        if res is None:
            apps_all_pass = False
            app_entries.append(
                {
                    "id": app_id,
                    "status": "fail",
                    "error": f"target `{pkg}::{target}` did not run",
                    "backends": backends,
                    "tests_executed": 0,
                    "tests_passed": 0,
                    "tests_failed": 0,
                    "tests_ignored": 0,
                    "required_tests": req_tests,
                }
            )
            continue

        passed_set = set(res["passed_tests"])
        failed_set = set(res["failed_tests"])
        ignored_set = set(res["ignored_tests"])
        req_passed = [t for t in req_tests if t in passed_set]
        req_failed = [t for t in req_tests if t in failed_set]
        req_ignored = [t for t in req_tests if t in ignored_set]
        missing = [t for t in req_tests if t not in passed_set and t not in failed_set and t not in ignored_set]
        app_ok = (
            res["status"] == "pass"
            and len(req_tests) > 0
            and len(req_passed) == len(req_tests)
            and not req_failed
            and not req_ignored
            and not missing
        )
        if not app_ok:
            apps_all_pass = False
        entry: dict[str, Any] = {
            "id": app_id,
            "status": "pass" if app_ok else "fail",
            "target": f"{pkg}::{target}",
            "backends": backends,
            "tests_executed": len(req_passed) + len(req_failed) + len(req_ignored),
            "tests_passed": len(req_passed),
            "tests_failed": len(req_failed),
            "tests_ignored": len(req_ignored),
            "required_tests": req_tests,
        }
        if missing:
            entry["missing_tests"] = missing
        app_entries.append(entry)

    everyday_apps_receipt = {
        "schema_version": 1,
        "platform": resolved_platform,
        "source_commit": source_commit,
        "source_tree": source_tree,
        "llvm_api_enabled": llvm_api,
        "backends": backends,
        "status": "pass" if (apps_all_pass and llvm_api and len(app_entries) == len(declared_app_ids)) else "fail",
        "required_count": len(declared_app_ids),
        "passed_count": sum(1 for a in app_entries if a["status"] == "pass"),
        "applications": app_entries,
    }

    cap_entries: list[dict[str, Any]] = []
    caps_all_pass = True
    for cap_id in declared_cap_ids:
        targets = cap_targets.get(cap_id, [])
        if not targets:
            caps_all_pass = False
            cap_entries.append(
                {
                    "id": cap_id,
                    "status": "fail",
                    "error": f"unmapped capability `{cap_id}`",
                    "targets": [],
                    "backends": backends,
                    "tests_executed": 0,
                    "tests_passed": 0,
                    "tests_failed": 0,
                    "tests_ignored": 0,
                }
            )
            continue

        t_exec = 0
        t_pass = 0
        t_fail = 0
        t_ign = 0
        cap_ok = True
        target_labels: list[str] = []
        failed_targets: list[str] = []
        for pkg, target in targets:
            label = f"{pkg}::{target}"
            target_labels.append(label)
            res = target_results.get((pkg, target))
            if res is None or res["status"] != "pass":
                cap_ok = False
                failed_targets.append(label)
            if res is not None:
                t_exec += int(res["tests_executed"])
                t_pass += int(res["tests_passed"])
                t_fail += int(res["tests_failed"])
                t_ign += int(res["tests_ignored"])

        if t_exec == 0 or t_pass != t_exec or t_fail != 0 or t_ign != 0:
            cap_ok = False
        if not cap_ok:
            caps_all_pass = False

        entry = {
            "id": cap_id,
            "status": "pass" if cap_ok else "fail",
            "targets": target_labels,
            "backends": backends,
            "tests_executed": t_exec,
            "tests_passed": t_pass,
            "tests_failed": t_fail,
            "tests_ignored": t_ign,
        }
        if failed_targets:
            entry["failed_targets"] = failed_targets
        cap_entries.append(entry)

    capabilities_receipt = {
        "schema_version": 1,
        "platform": resolved_platform,
        "source_commit": source_commit,
        "source_tree": source_tree,
        "llvm_api_enabled": llvm_api,
        "backends": backends,
        "status": "pass" if (caps_all_pass and llvm_api and len(cap_entries) == len(declared_cap_ids)) else "fail",
        "required_count": len(declared_cap_ids),
        "passed_count": sum(1 for c in cap_entries if c["status"] == "pass"),
        "capabilities": cap_entries,
    }
    return everyday_apps_receipt, capabilities_receipt


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--source-tree", required=True)
    parser.add_argument(
        "--platform",
        choices=["auto", "linux", "windows"],
        default="auto",
        help="Target host platform suite selection (default: auto).",
    )
    parser.add_argument(
        "--allow-clang-only",
        action="store_true",
        help="Allow local dry-run without LLVM C-API DSO/DLL (still produces status=fail for acceptance).",
    )
    args = parser.parse_args(argv)

    repo = args.repo.resolve()
    platform = _resolve_platform(args.platform)
    llvm_api = _has_llvm_api_dso(platform)
    if not llvm_api and not args.allow_clang_only:
        expected_lib = (
            "ZIL_LLVM_LIB_DIR/LLVM-C.lib and LLVM-C.dll"
            if platform == "windows"
            else "ZIL_LLVM_LIB_DIR/libLLVM-23.so"
        )
        sys.stderr.write(
            f"ERROR: {expected_lib} is required for dual-backend Alpha acceptance on {platform}.\n"
        )
        return 1

    by_pkg = _unique_targets_by_package(platform)
    _prebuild_targets(repo, by_pkg, llvm_api)

    target_results: dict[tuple[str, str], dict[str, Any]] = {}
    for pkg, targets in by_pkg.items():
        for target in targets:
            target_results[(pkg, target)] = _run_target(repo, pkg, target, llvm_api)

    apps_receipt, caps_receipt = build_receipts(
        repo=repo,
        source_commit=args.source_commit,
        source_tree=args.source_tree,
        llvm_api=llvm_api,
        target_results=target_results,
        platform=platform,
    )

    args.out_dir.mkdir(parents=True, exist_ok=True)
    (args.out_dir / "everyday-apps-receipt.json").write_text(
        json.dumps(apps_receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (args.out_dir / "capabilities-receipt.json").write_text(
        json.dumps(caps_receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    print(
        json.dumps(
            {
                "platform": platform,
                "everyday_apps": f"{apps_receipt['passed_count']}/{apps_receipt['required_count']} ({apps_receipt['status']})",
                "capabilities": f"{caps_receipt['passed_count']}/{caps_receipt['required_count']} ({caps_receipt['status']})",
                "llvm_api_enabled": llvm_api,
            },
            indent=2,
            sort_keys=True,
        ),
        flush=True,
    )
    return 0 if (apps_receipt["status"] == "pass" and caps_receipt["status"] == "pass") else 1


if __name__ == "__main__":
    sys.exit(main())
