#!/usr/bin/env python3
"""Fail-closed verifier and single-summary builder for Alpha acceptance CI receipts.

Checks that all required receipts from a single exact Git commit exist, have
non-zero executions, contain no Unsupported / ResourceLimit / resource-exhausted
fallbacks, cover all 8 declared everyday applications and all 12 declared basic-
language capabilities with both Clang and LLVM API backends at O0/O2, and meet
all required Fixed Projects, Roam, and Native API counts.
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
FIXED_HISTORICAL_COMPARISONS = 196
FIXED_ARCHIVAL_COMPARISONS = 44
MIN_ROAM_CALLABLE_POS = 28
MIN_ROAM_CALLABLE_NEG = 14
MIN_ROAM_TASK_POS = 16
MIN_ROAM_TASK_NEG = 8

REQUIRED_BACKENDS = {"clang-o0", "clang-o2", "llvmapi-o0", "llvmapi-o2"}
FORBIDDEN_TOKENS = ("Unsupported", "ResourceLimit", "resource_limit")

DEFAULT_REPO = Path(__file__).resolve().parents[2]


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


def _load_declared_contracts(repo: Path, errors: list[str]) -> tuple[list[str], list[str]]:
    profile = _load_json(
        repo / "validation/language-alpha-profile.json",
        "validation/language-alpha-profile.json",
        errors,
    )
    gates = _load_json(
        repo / "validation/language-release-gates.json",
        "validation/language-release-gates.json",
        errors,
    )
    app_ids: list[str] = []
    if profile is not None:
        apps = (profile.get("everyday_applications") or {}).get("applications") or []
        app_ids = [str(a["id"]) for a in apps if isinstance(a, dict) and isinstance(a.get("id"), str)]
        if len(app_ids) != 8:
            errors.append(f"language-alpha-profile.json declared {len(app_ids)} everyday apps, expected 8")
    cap_ids: list[str] = []
    if gates is not None:
        caps = gates.get("capabilities") or []
        cap_ids = [str(c["id"]) for c in caps if isinstance(c, dict) and isinstance(c.get("id"), str)]
        if len(cap_ids) != 12:
            errors.append(f"language-release-gates.json declared {len(cap_ids)} capabilities, expected 12")
    return app_ids, cap_ids


def _case_identity(case: dict[str, Any]) -> str:
    for key in ("label", "id", "name"):
        val = case.get(key)
        if isinstance(val, str) and val.strip():
            return val.strip()
    return ""


def _is_negative_case(case: dict[str, Any]) -> bool:
    expected = case.get("expected")
    if isinstance(expected, dict) and str(expected.get("kind", "")).lower() == "safe-reject":
        return True
    if str(case.get("kind", "")).lower() in ("negative", "safe-reject"):
        return True
    return False


def _validate_roam_cases(
    cases: Any,
    label: str,
    pos_sel: int,
    pos_pass: int,
    neg_sel: int,
    neg_pass: int,
    errors: list[str],
) -> list[str]:
    failed_cases: list[str] = []
    if not isinstance(cases, list):
        errors.append(f"{label} missing `cases` list")
        return failed_cases

    expected_total = pos_sel + neg_sel
    if len(cases) != expected_total:
        errors.append(
            f"{label} cases count mismatch: len(cases)={len(cases)} != selected={expected_total}"
        )

    seen_ids: set[str] = set()
    actual_pos_total = 0
    actual_pos_pass = 0
    actual_neg_total = 0
    actual_neg_pass = 0

    for idx, case in enumerate(cases):
        if not isinstance(case, dict):
            errors.append(f"{label} case[{idx}] is not a JSON object")
            continue
        name = _case_identity(case)
        if not name:
            errors.append(f"{label} case[{idx}] missing non-empty identity (`label`/`id`/`name`)")
            name = f"case-{idx}"
        elif name in seen_ids:
            errors.append(f"{label} duplicate case identity `{name}`")
        else:
            seen_ids.add(name)

        status = str(case.get("status", "")).lower()
        passed = status == "pass"
        if not passed:
            failed_cases.append(f"{label}:{name}:{status or 'missing'}")
            errors.append(f"{label} case `{name}` status is `{status or 'missing'}`, expected `pass`")

        raw = json.dumps(case, sort_keys=True)
        for tok in FORBIDDEN_TOKENS:
            if tok in raw:
                errors.append(f"{label} case `{name}` contains forbidden token `{tok}`")

        if _is_negative_case(case):
            actual_neg_total += 1
            if passed:
                actual_neg_pass += 1
        else:
            actual_pos_total += 1
            if passed:
                actual_pos_pass += 1

    if actual_pos_total != pos_sel or actual_pos_pass != pos_pass:
        errors.append(
            f"{label} per-case positive tally ({actual_pos_pass}/{actual_pos_total}) "
            f"does not match summary ({pos_pass}/{pos_sel})"
        )
    if actual_neg_total != neg_sel or actual_neg_pass != neg_pass:
        errors.append(
            f"{label} per-case negative tally ({actual_neg_pass}/{actual_neg_total}) "
            f"does not match summary ({neg_pass}/{neg_sel})"
        )

    return failed_cases


def _check_non_empty_sha(val: Any, name: str, errors: list[str]) -> str:
    s = str(val or "").strip()
    if len(s) < 40:
        errors.append(f"missing or invalid {name}: `{s}`")
    return s


def _validate_profile_receipt(
    data: dict[str, Any] | None,
    *,
    label: str,
    items_key: str,
    declared_ids: list[str],
    expected_commit: str,
    expected_tree: str,
    require_targets: bool,
    errors: list[str],
    failed_items: list[str],
) -> dict[str, Any]:
    if data is None:
        return {}

    status = str(data.get("status", "")).lower()
    if status != "pass":
        errors.append(f"{label} status is `{status or 'missing'}`, expected `pass`")

    r_commit = str(data.get("source_commit", "")).strip()
    r_tree = str(data.get("source_tree", "")).strip()
    if expected_commit and r_commit != expected_commit:
        errors.append(
            f"{label} source_commit `{r_commit}` does not match build-receipt `{expected_commit}`"
        )
    if expected_tree and r_tree != expected_tree:
        errors.append(
            f"{label} source_tree `{r_tree}` does not match build-receipt `{expected_tree}`"
        )

    if data.get("llvm_api_enabled") is not True:
        errors.append(f"{label} llvm_api_enabled is not true (dual-backend llvm-api required)")

    top_backends = set(data.get("backends") or [])
    missing_top_backends = sorted(REQUIRED_BACKENDS - top_backends)
    if missing_top_backends:
        errors.append(f"{label} missing required backends: {missing_top_backends}")

    raw_top = json.dumps(data, sort_keys=True)
    for tok in FORBIDDEN_TOKENS:
        if tok in raw_top:
            errors.append(f"{label} contains forbidden token `{tok}`")

    items = data.get(items_key)
    if not isinstance(items, list):
        errors.append(f"{label} missing `{items_key}` list")
        return {"status": "fail", "passed": f"0/{len(declared_ids)}", "items": []}

    by_id: dict[str, dict[str, Any]] = {}
    for idx, entry in enumerate(items):
        if not isinstance(entry, dict):
            errors.append(f"{label} {items_key}[{idx}] is not a JSON object")
            continue
        eid = str(entry.get("id", "")).strip()
        if not eid:
            errors.append(f"{label} {items_key}[{idx}] missing `id`")
            continue
        if eid in by_id:
            errors.append(f"{label} duplicate id `{eid}`")
            continue
        by_id[eid] = entry

    unexpected = sorted(set(by_id) - set(declared_ids))
    if unexpected:
        errors.append(f"{label} contains undeclared ids: {unexpected}")

    summary_rows: list[dict[str, Any]] = []
    passed_count = 0
    total_tests = 0
    for expected_id in declared_ids:
        entry = by_id.get(expected_id)
        if entry is None:
            errors.append(f"{label} missing required id `{expected_id}`")
            failed_items.append(f"{label}:{expected_id}:missing")
            summary_rows.append(
                {"id": expected_id, "status": "missing", "tests_executed": 0, "tests_passed": 0}
            )
            continue

        e_status = str(entry.get("status", "")).lower()
        t_exec = int(entry.get("tests_executed", 0))
        t_pass = int(entry.get("tests_passed", 0))
        t_fail = int(entry.get("tests_failed", 0))
        t_ign = int(entry.get("tests_ignored", 0))
        e_backends = set(entry.get("backends") or [])
        missing_e_backends = sorted(REQUIRED_BACKENDS - e_backends)

        item_errors: list[str] = []
        if e_status != "pass":
            item_errors.append(f"status=`{e_status or 'missing'}`")
        if t_exec <= 0:
            item_errors.append("tests_executed==0")
        if t_pass != t_exec:
            item_errors.append(f"tests_passed({t_pass})!=tests_executed({t_exec})")
        if t_fail != 0:
            item_errors.append(f"tests_failed={t_fail}")
        if t_ign != 0:
            item_errors.append(f"tests_ignored={t_ign}")
        if missing_e_backends:
            item_errors.append(f"missing_backends={missing_e_backends}")
        if require_targets and not (entry.get("targets") or []):
            item_errors.append("targets empty")

        if item_errors:
            errors.append(f"{label} `{expected_id}` failed validation: {', '.join(item_errors)}")
            failed_items.append(f"{label}:{expected_id}:{e_status or 'fail'}")
        else:
            passed_count += 1
        total_tests += t_exec
        summary_rows.append(
            {
                "id": expected_id,
                "status": "pass" if not item_errors else "fail",
                "tests_executed": t_exec,
                "tests_passed": t_pass,
                "tests_failed": t_fail,
                "tests_ignored": t_ign,
            }
        )

    if int(data.get("required_count", 0)) != len(declared_ids):
        errors.append(
            f"{label} required_count={data.get('required_count')} != declared {len(declared_ids)}"
        )
    if int(data.get("passed_count", 0)) != passed_count:
        errors.append(
            f"{label} passed_count={data.get('passed_count')} != verified {passed_count}"
        )

    return {
        "status": "pass" if (passed_count == len(declared_ids) and status == "pass" and not missing_top_backends) else "fail",
        "passed": f"{passed_count}/{len(declared_ids)}",
        "total_tests_executed": total_tests,
        "backends": sorted(top_backends),
        "items": summary_rows,
    }


def evaluate_receipts(
    receipts_dir: Path,
    repo: Path = DEFAULT_REPO,
) -> tuple[dict[str, Any], list[str]]:
    errors: list[str] = []
    failed_items: list[str] = []

    declared_app_ids, declared_cap_ids = _load_declared_contracts(repo, errors)

    build = _load_json(receipts_dir / "build-receipt.json", "build-receipt", errors)
    native = _load_json(receipts_dir / "native-api-summary.json", "native-api-summary", errors)
    fixed = _load_json(receipts_dir / "fixed-projects-summary.json", "fixed-projects-summary", errors)
    roam_callable = _load_json(
        receipts_dir / "roam-callable-report.json", "roam-callable-report", errors
    )
    roam_task = _load_json(receipts_dir / "roam-task-report.json", "roam-task-report", errors)
    everyday_apps = _load_json(
        receipts_dir / "everyday-apps-receipt.json", "everyday-apps-receipt", errors
    )
    capabilities = _load_json(
        receipts_dir / "capabilities-receipt.json", "capabilities-receipt", errors
    )

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

        orig_cmp_pass = int(orig_native.get("comparisons_passed", 0))
        orig_cmp_req = int(orig_native.get("comparisons_required", 0))
        orig_cmp_hist = int(orig_native.get("comparisons_selected", FIXED_HISTORICAL_COMPARISONS))
        orig_cmp_archival = max(0, orig_cmp_hist - orig_cmp_req)

        fixed_summary = {
            "original_projects": orig_projects,
            "originals_verified": originals_verified,
            "expected_rejections": f"{exp_rej}/{exp_rej_sel}",
            "original_native_builds": f"{orig_native.get('builds_passed', 0)}/{orig_native.get('builds_selected', 0)}",
            "original_native_comparisons": f"{orig_cmp_pass}/{orig_cmp_req}",
            "original_native_comparisons_historical": orig_cmp_hist,
            "original_native_comparisons_archival": orig_cmp_archival,
            "original_native_comparisons_note": (
                f"{orig_cmp_pass}/{orig_cmp_req} required "
                f"({orig_cmp_hist} historical, {orig_cmp_archival} archival due to lost stdin)"
            ),
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
        failed_items.extend(
            _validate_roam_cases(
                data.get("cases"), label, pos_sel, pos_pass, neg_sel, neg_pass, errors
            )
        )
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

    everyday_apps_summary = _validate_profile_receipt(
        everyday_apps,
        label="everyday-apps",
        items_key="applications",
        declared_ids=declared_app_ids,
        expected_commit=commit,
        expected_tree=source_tree,
        require_targets=False,
        errors=errors,
        failed_items=failed_items,
    )
    capabilities_summary = _validate_profile_receipt(
        capabilities,
        label="capabilities",
        items_key="capabilities",
        declared_ids=declared_cap_ids,
        expected_commit=commit,
        expected_tree=source_tree,
        require_targets=True,
        errors=errors,
        failed_items=failed_items,
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
        "everyday_apps": everyday_apps_summary,
        "capabilities": capabilities_summary,
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
        f"orig={fp.get('original_native_builds', '0/0')} builds / "
        f"{fp.get('original_native_comparisons_note', fp.get('original_native_comparisons', '0/0'))}, "
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
    ea = summary.get("everyday_apps") or {}
    lines.append(
        f"| Everyday Applications (`language-alpha-profile.json`) | `{ea.get('status', 'missing')}` | "
        f"apps={ea.get('passed', '0/8')}, tests={ea.get('total_tests_executed', 0)}, "
        f"backends={','.join(ea.get('backends', []))} |"
    )
    cap = summary.get("capabilities") or {}
    lines.append(
        f"| Functional Capabilities (`language-release-gates.json`) | `{cap.get('status', 'missing')}` | "
        f"gates={cap.get('passed', '0/12')}, tests={cap.get('total_tests_executed', 0)}, "
        f"backends={','.join(cap.get('backends', []))} |"
    )

    if ea.get("items"):
        lines.extend(["", "### Everyday Applications (8/8 Contract)", "", "| Application | Status | Tests Passed |", "| --- | --- | --- |"])
        for row in ea["items"]:
            lines.append(f"| `{row['id']}` | `{row['status']}` | {row['tests_passed']}/{row['tests_executed']} |")

    if cap.get("items"):
        lines.extend(["", "### Basic-Language Capabilities (12/12 Contract)", "", "| Capability | Status | Tests Passed |", "| --- | --- | --- |"])
        for row in cap["items"]:
            lines.append(f"| `{row['id']}` | `{row['status']}` | {row['tests_passed']}/{row['tests_executed']} |")

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
    parser.add_argument("--repo", type=Path, default=DEFAULT_REPO)
    parser.add_argument("--receipts-dir", type=Path, required=True)
    parser.add_argument("--out-json", type=Path, required=True)
    parser.add_argument("--out-md", type=Path, required=True)
    args = parser.parse_args(argv)

    summary, errors = evaluate_receipts(args.receipts_dir, repo=args.repo.resolve())
    args.out_json.parent.mkdir(parents=True, exist_ok=True)
    args.out_json.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    md = render_markdown(summary)
    args.out_md.parent.mkdir(parents=True, exist_ok=True)
    args.out_md.write_text(md, encoding="utf-8")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if not errors else 1


if __name__ == "__main__":
    sys.exit(main())
