#!/usr/bin/env python3
"""Fail-closed verifier and single-summary builder for Alpha acceptance CI receipts.

Checks that all required receipts from a single exact Git commit exist, have
non-zero executions, contain no Unsupported / ResourceLimit / resource-exhausted
fallbacks, and meet all required fixed-project, Roam, and native-API counts.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


MIN_NATIVE_BUILDS = 16
MIN_NATIVE_EXECUTIONS = 128
MIN_NATIVE_INJECTED_FAILURES = 56
MIN_FIXED_ORIGINAL_PROJECTS = 21
MIN_FIXED_REQUIRED_COMPARISONS = 152
MIN_ROAM_CALLABLE_POS = 28
MIN_ROAM_CALLABLE_NEG = 14
MIN_ROAM_TASK_POS = 16
MIN_ROAM_TASK_NEG = 8

FORBIDDEN_TOKENS = ("Unsupported", "ResourceLimit", "resource_limit")


def _load_json(path: Path, label: str, errors: list[str]) -> dict[str, Any] | None:
    if not path.is_file():
        errors.append(f"missing required receipt: {label} ({path})")
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        errors.append(f"unparseable JSON in {label} ({path}): {exc}")
        return None
    if not isinstance(data, dict):
        errors.append(f"expected JSON object in {label} ({path})")
        return None
    return data


def _scan_forbidden_in_cases(cases: Any, label: str, errors: list[str]) -> list[str]:
    failed_cases: list[str] = []
    if not isinstance(cases, list):
        return failed_cases
    for idx, case in enumerate(cases):
        if not isinstance(case, dict):
            continue
        status = str(case.get("status", "")).lower()
        name = str(case.get("id") or case.get("name") or case.get("exit") or f"case-{idx}")
        if status and status != "pass":
            failed_cases.append(f"{label}:{name}:{status}")
        raw = json.dumps(case, sort_keys=True)
        for tok in FORBIDDEN_TOKENS:
            if tok in raw and status != "pass":
                errors.append(f"{label} case `{name}` failed with forbidden token `{tok}`")
    return failed_cases


def _check_non_empty_sha(val: Any, name: str, errors: list[str]) -> str:
    s = str(val or "").strip()
    if len(s) < 40:
        errors.append(f"missing or invalid {name}: `{s}`")
    return s


def evaluate_receipts(receipts_dir: Path) -> tuple[dict[str, Any], list[str]]:
    errors: list[str] = []
    failed_items: list[str] = []

    build = _load_json(receipts_dir / "build-receipt.json", "build-receipt", errors)
    native = _load_json(receipts_dir / "native-api-summary.json", "native-api-summary", errors)
    fixed = _load_json(receipts_dir / "fixed-projects-summary.json", "fixed-projects-summary", errors)
    roam_callable = _load_json(
        receipts_dir / "roam-callable-report.json", "roam-callable-report", errors
    )
    roam_task = _load_json(receipts_dir / "roam-task-report.json", "roam-task-report", errors)

    commit = ""
    source_tree = ""
    cli_sha256 = ""
    roam_sha256 = ""
    if build is not None:
        commit = _check_non_empty_sha(build.get("source_commit"), "source_commit", errors)
        source_tree = _check_non_empty_sha(build.get("source_tree"), "source_tree", errors)
        cli_sha256 = _check_non_empty_sha(build.get("cli_sha256"), "cli_sha256", errors)
        roam_sha256 = _check_non_empty_sha(build.get("roam_sha256"), "roam_sha256", errors)
        if build.get("exit_code") != 0:
            errors.append(f"build-receipt exit_code != 0: {build.get('exit_code')}")

    native_summary: dict[str, Any] = {}
    if native is not None:
        status = str(native.get("status", ""))
        builds = int(native.get("builds", 0))
        executions = int(native.get("executions", 0))
        injected = int(native.get("injected_failures", 0))
        native_summary = {
            "status": status,
            "builds": builds,
            "executions": executions,
            "injected_failures": injected,
        }
        if status.lower() != "pass":
            errors.append(f"native-api status is `{status}`, expected `Pass`")
        if builds < MIN_NATIVE_BUILDS:
            errors.append(f"native-api builds {builds} < {MIN_NATIVE_BUILDS}")
        if executions < MIN_NATIVE_EXECUTIONS:
            errors.append(f"native-api executions {executions} < {MIN_NATIVE_EXECUTIONS}")
        if injected < MIN_NATIVE_INJECTED_FAILURES:
            errors.append(
                f"native-api injected_failures {injected} < {MIN_NATIVE_INJECTED_FAILURES}"
            )

    fixed_summary: dict[str, Any] = {}
    if fixed is not None:
        orig_projects = int(fixed.get("original_projects", 0))
        originals_verified = int(fixed.get("originals_verified", 0))
        exp_rej_sel = int(fixed.get("expected_rejections_selected", 0))
        exp_rej = int(fixed.get("expected_rejections", 0))
        exhausted = fixed.get("source_resource_exhausted", [])
        orig_native = fixed.get("original_native", {}) or {}
        add_native = fixed.get("additional_native", {}) or {}
        ctrl_native = fixed.get("control_native", {}) or {}
        cleanup_native = fixed.get("cleanup_control_native", {}) or {}

        fixed_summary = {
            "original_projects": orig_projects,
            "originals_verified": originals_verified,
            "expected_rejections": f"{exp_rej}/{exp_rej_sel}",
            "original_native_builds": f"{orig_native.get('builds_passed', 0)}/{orig_native.get('builds_selected', 0)}",
            "original_native_comparisons": f"{orig_native.get('comparisons_passed', 0)}/{orig_native.get('comparisons_required', 0)}",
            "additional_native_builds": f"{add_native.get('builds_passed', 0)}/{add_native.get('builds_selected', 0)}",
            "additional_native_comparisons": f"{add_native.get('comparisons_passed', 0)}/{add_native.get('comparisons_required', 0)}",
            "control_native_builds": f"{ctrl_native.get('builds_passed', 0)}/{ctrl_native.get('builds_selected', 0)}",
            "control_native_comparisons": f"{ctrl_native.get('comparisons_passed', 0)}/{ctrl_native.get('comparisons_required', 0)}",
            "cleanup_control_native": f"{cleanup_native.get('builds_passed', 0)}/{cleanup_native.get('builds_selected', 0)}",
        }

        if orig_projects < MIN_FIXED_ORIGINAL_PROJECTS:
            errors.append(
                f"fixed-projects original_projects {orig_projects} < {MIN_FIXED_ORIGINAL_PROJECTS}"
            )
        if originals_verified + exp_rej != orig_projects:
            errors.append(
                f"fixed-projects originals_verified ({originals_verified}) + expected_rejections ({exp_rej}) != {orig_projects}"
            )
        if exp_rej != exp_rej_sel or exp_rej_sel == 0:
            errors.append(f"fixed-projects expected_rejections incomplete: {exp_rej}/{exp_rej_sel}")
        if exhausted:
            errors.append(f"fixed-projects source_resource_exhausted non-empty: {exhausted}")

        for sec_name, sec, min_req in (
            ("original_native", orig_native, MIN_FIXED_REQUIRED_COMPARISONS),
            ("additional_native", add_native, 1),
            ("control_native", ctrl_native, 1),
            ("cleanup_control_native", cleanup_native, 1),
        ):
            b_sel = int(sec.get("builds_selected", 0))
            b_pass = int(sec.get("builds_passed", 0))
            c_req = int(sec.get("comparisons_required", 0))
            c_pass = int(sec.get("comparisons_passed", 0))
            if b_sel == 0 or b_pass != b_sel:
                errors.append(f"fixed-projects {sec_name} builds incomplete: {b_pass}/{b_sel}")
            if c_req < min_req or c_pass < c_req:
                errors.append(
                    f"fixed-projects {sec_name} comparisons incomplete: {c_pass}/{c_req} (min {min_req})"
                )

    def _check_roam(
        data: dict[str, Any] | None, label: str, min_pos: int, min_neg: int
    ) -> dict[str, Any]:
        if data is None:
            return {}
        status = str(data.get("status", ""))
        pos = data.get("positive", {}) or {}
        neg = data.get("negative", {}) or {}
        pos_sel = int(pos.get("selected", 0))
        pos_pass = int(pos.get("passed", 0))
        neg_sel = int(neg.get("selected", 0))
        neg_pass = int(neg.get("passed", 0))
        if status.lower() != "pass":
            errors.append(f"{label} status is `{status}`, expected `pass`")
        if pos_sel < min_pos or pos_pass != pos_sel:
            errors.append(f"{label} positive incomplete: {pos_pass}/{pos_sel} (min {min_pos})")
        if neg_sel < min_neg or neg_pass != neg_sel:
            errors.append(f"{label} negative incomplete: {neg_pass}/{neg_sel} (min {min_neg})")
        failed_items.extend(_scan_forbidden_in_cases(data.get("cases"), label, errors))
        return {
            "status": status,
            "positive": f"{pos_pass}/{pos_sel}",
            "negative": f"{neg_pass}/{neg_sel}",
            "total": f"{pos_pass + neg_pass}/{pos_sel + neg_sel}",
        }

    roam_callable_summary = _check_roam(
        roam_callable, "roam-callable", MIN_ROAM_CALLABLE_POS, MIN_ROAM_CALLABLE_NEG
    )
    roam_task_summary = _check_roam(
        roam_task, "roam-task", MIN_ROAM_TASK_POS, MIN_ROAM_TASK_NEG
    )

    overall = "pass" if not errors else "fail"
    summary = {
        "status": overall,
        "source_commit": commit,
        "source_tree": source_tree,
        "cli_sha256": cli_sha256,
        "roam_sha256": roam_sha256,
        "native_api": native_summary,
        "fixed_projects": fixed_summary,
        "roam_callable": roam_callable_summary,
        "roam_task": roam_task_summary,
        "failed_items": failed_items,
        "errors": errors,
    }
    return summary, errors


def render_markdown(summary: dict[str, Any]) -> str:
    lines = [
        f"## Alpha Acceptance Evidence Summary (`{summary['status'].upper()}`)",
        "",
        f"- **Commit**: `{summary['source_commit']}`",
        f"- **Source Tree**: `{summary['source_tree']}`",
        f"- **ZIL CLI SHA-256**: `{summary['cli_sha256']}`",
        f"- **Roam SHA-256**: `{summary['roam_sha256']}`",
        "",
        "| Suite | Result | Details |",
        "| --- | --- | --- |",
    ]
    na = summary.get("native_api") or {}
    lines.append(
        f"| Native API (`validate.py`) | `{na.get('status', 'missing')}` | "
        f"builds={na.get('builds', 0)}, executions={na.get('executions', 0)}, "
        f"injected_failures={na.get('injected_failures', 0)} |"
    )
    fp = summary.get("fixed_projects") or {}
    lines.append(
        f"| Fixed Projects (`check-fixed-projects.py`) | "
        f"`{fp.get('original_projects', 0)}` projects | "
        f"verified={fp.get('originals_verified', 0)}, rej={fp.get('expected_rejections', '0/0')}, "
        f"orig={fp.get('original_native_builds', '0/0')} builds / {fp.get('original_native_comparisons', '0/0')} cmp, "
        f"add={fp.get('additional_native_builds', '0/0')}, ctrl={fp.get('control_native_builds', '0/0')} |"
    )
    rc = summary.get("roam_callable") or {}
    lines.append(
        f"| Roam `callable-combinations` | `{rc.get('status', 'missing')}` | "
        f"pos={rc.get('positive', '0/0')}, neg={rc.get('negative', '0/0')}, total={rc.get('total', '0/0')} |"
    )
    rt = summary.get("roam_task") or {}
    lines.append(
        f"| Roam `task-result-combinations` | `{rt.get('status', 'missing')}` | "
        f"pos={rt.get('positive', '0/0')}, neg={rt.get('negative', '0/0')}, total={rt.get('total', '0/0')} |"
    )
    if summary.get("errors"):
        lines.append("")
        lines.append("### Errors")
        for err in summary["errors"]:
            lines.append(f"- {err}")
    if summary.get("failed_items"):
        lines.append("")
        lines.append("### Failed Cases")
        for item in summary["failed_items"]:
            lines.append(f"- `{item}`")
    lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipts-dir", type=Path, required=True)
    parser.add_argument("--out-json", type=Path, required=True)
    parser.add_argument("--out-md", type=Path, required=True)
    args = parser.parse_args(argv)

    summary, errors = evaluate_receipts(args.receipts_dir)
    args.out_json.parent.mkdir(parents=True, exist_ok=True)
    args.out_json.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    md = render_markdown(summary)
    args.out_md.parent.mkdir(parents=True, exist_ok=True)
    args.out_md.write_text(md, encoding="utf-8")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if not errors else 1


if __name__ == "__main__":
    sys.exit(main())
