#!/usr/bin/env python3
"""Unit tests for tools/ci/alpha_acceptance_summary.py and run_alpha_profile_suites.py."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import alpha_acceptance_summary as mod  # noqa: E402
import run_alpha_profile_suites as runner  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
BACKENDS = ["clang-o0", "clang-o2", "llvmapi-o0", "llvmapi-o2"]


def _declared_ids() -> tuple[list[str], list[str]]:
    profile = json.loads((REPO_ROOT / "validation/language-alpha-profile.json").read_text(encoding="utf-8"))
    gates = json.loads((REPO_ROOT / "validation/language-release-gates.json").read_text(encoding="utf-8"))
    return (
        [a["id"] for a in profile["everyday_applications"]["applications"]],
        [c["id"] for c in gates["capabilities"]],
    )


def _write_valid_receipts(d: Path) -> None:
    app_ids, cap_ids = _declared_ids()
    commit = "0" * 40
    tree = "1" * 40
    (d / "build-receipt.json").write_text(
        json.dumps(
            {
                "source_commit": commit,
                "source_tree": tree,
                "cli_sha256": "a" * 64,
                "roam_sha256": "b" * 64,
                "exit_code": 0,
            }
        ),
        encoding="utf-8",
    )
    (d / "native-api-summary.json").write_text(
        json.dumps(
            {
                "status": "Pass",
                "builds": 16,
                "executions": 128,
                "injected_failures": 56,
            }
        ),
        encoding="utf-8",
    )
    (d / "fixed-projects-summary.json").write_text(
        json.dumps(
            {
                "original_projects": 21,
                "source_resource_exhausted": [],
                "originals_verified": 17,
                "expected_rejections_selected": 4,
                "expected_rejections": 4,
                "original_native": {
                    "builds_selected": 60,
                    "builds_passed": 60,
                    "comparisons_selected": 196,
                    "comparisons_required": 152,
                    "comparisons_passed": 152,
                },
                "additional_native": {
                    "builds_selected": 12,
                    "builds_passed": 12,
                    "comparisons_required": 56,
                    "comparisons_passed": 56,
                },
                "control_native": {
                    "builds_selected": 16,
                    "builds_passed": 16,
                    "comparisons_required": 60,
                    "comparisons_passed": 60,
                },
                "cleanup_control_native": {
                    "builds_selected": 8,
                    "builds_passed": 8,
                    "comparisons_required": 8,
                    "comparisons_passed": 8,
                },
            }
        ),
        encoding="utf-8",
    )
    (d / "roam-callable-report.json").write_text(
        json.dumps(
            {
                "status": "pass",
                "positive": {"selected": 28, "passed": 28},
                "negative": {"selected": 14, "passed": 14},
                "cases": [
                    {"label": f"pos-{i}", "status": "pass", "expected": {"kind": "lifecycle"}}
                    for i in range(28)
                ]
                + [
                    {"label": f"neg-{i}", "status": "pass", "expected": {"kind": "safe-reject"}}
                    for i in range(14)
                ],
            }
        ),
        encoding="utf-8",
    )
    (d / "roam-task-report.json").write_text(
        json.dumps(
            {
                "status": "pass",
                "positive": {"selected": 16, "passed": 16},
                "negative": {"selected": 8, "passed": 8},
                "cases": [
                    {"label": f"tpos-{i}", "status": "pass", "expected": {"kind": "lifecycle"}}
                    for i in range(16)
                ]
                + [
                    {"label": f"tneg-{i}", "status": "pass", "expected": {"kind": "safe-reject"}}
                    for i in range(8)
                ],
            }
        ),
        encoding="utf-8",
    )
    (d / "everyday-apps-receipt.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "source_commit": commit,
                "source_tree": tree,
                "llvm_api_enabled": True,
                "backends": BACKENDS,
                "status": "pass",
                "required_count": len(app_ids),
                "passed_count": len(app_ids),
                "applications": [
                    {
                        "id": app_id,
                        "status": "pass",
                        "backends": BACKENDS,
                        "tests_executed": 2,
                        "tests_passed": 2,
                        "tests_failed": 0,
                        "tests_ignored": 0,
                        "required_tests": ["t1", "t2"],
                    }
                    for app_id in app_ids
                ],
            }
        ),
        encoding="utf-8",
    )
    (d / "capabilities-receipt.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "source_commit": commit,
                "source_tree": tree,
                "llvm_api_enabled": True,
                "backends": BACKENDS,
                "status": "pass",
                "required_count": len(cap_ids),
                "passed_count": len(cap_ids),
                "capabilities": [
                    {
                        "id": cap_id,
                        "status": "pass",
                        "targets": ["zil-driver::sample_target"],
                        "backends": BACKENDS,
                        "tests_executed": 4,
                        "tests_passed": 4,
                        "tests_failed": 0,
                        "tests_ignored": 0,
                    }
                    for cap_id in cap_ids
                ],
            }
        ),
        encoding="utf-8",
    )


class AlphaAcceptanceSummaryTests(unittest.TestCase):
    def test_valid_receipts_pass_and_render_honest_denominators(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(errors, [])
            self.assertEqual(summary["status"], "pass")
            self.assertEqual(summary["everyday_apps"]["passed"], "8/8")
            self.assertEqual(summary["capabilities"]["passed"], "12/12")
            md = mod.render_markdown(summary)
            self.assertIn("Alpha Acceptance Evidence Summary (`PASS`)", md)
            self.assertIn("152/152 required (196 historical, 44 archival due to lost stdin)", md)
            self.assertIn("Everyday Applications (8/8 Contract)", md)
            self.assertIn("Basic-Language Capabilities (12/12 Contract)", md)

    def test_missing_report_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            (d / "roam-task-report.json").unlink()
            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("roam-task-report" in e for e in errors))

    def test_missing_everyday_apps_or_capabilities_receipt_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            (d / "everyday-apps-receipt.json").unlink()
            (d / "capabilities-receipt.json").unlink()
            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("everyday-apps-receipt" in e for e in errors))
            self.assertTrue(any("capabilities-receipt" in e for e in errors))

    def test_missing_app_id_and_missing_capability_id_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            ea = json.loads((d / "everyday-apps-receipt.json").read_text(encoding="utf-8"))
            ea["applications"] = ea["applications"][:-1]
            (d / "everyday-apps-receipt.json").write_text(json.dumps(ea), encoding="utf-8")
            cap = json.loads((d / "capabilities-receipt.json").read_text(encoding="utf-8"))
            cap["capabilities"] = cap["capabilities"][:-1]
            (d / "capabilities-receipt.json").write_text(json.dumps(cap), encoding="utf-8")
            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("everyday-apps missing required id" in e for e in errors))
            self.assertTrue(any("capabilities missing required id" in e for e in errors))

    def test_zero_tests_skipped_tests_missing_llvmapi_and_commit_mismatch_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            ea = json.loads((d / "everyday-apps-receipt.json").read_text(encoding="utf-8"))
            ea["source_commit"] = "9" * 40
            ea["llvm_api_enabled"] = False
            ea["backends"] = ["clang-o0", "clang-o2"]
            ea["applications"][0]["tests_executed"] = 0
            ea["applications"][0]["tests_passed"] = 0
            (d / "everyday-apps-receipt.json").write_text(json.dumps(ea), encoding="utf-8")

            cap = json.loads((d / "capabilities-receipt.json").read_text(encoding="utf-8"))
            cap["capabilities"][0]["tests_ignored"] = 1
            cap["capabilities"][0]["tests_passed"] = 3
            (d / "capabilities-receipt.json").write_text(json.dumps(cap), encoding="utf-8")

            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("source_commit" in e for e in errors))
            self.assertTrue(any("llvm_api_enabled" in e for e in errors))
            self.assertTrue(any("tests_executed==0" in e for e in errors))
            self.assertTrue(any("tests_ignored=1" in e for e in errors))

    def test_zero_executions_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            (d / "native-api-summary.json").write_text(
                json.dumps({"status": "Pass", "builds": 0, "executions": 0, "injected_failures": 0}),
                encoding="utf-8",
            )
            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("executions" in e for e in errors))

    def test_incomplete_fixed_comparisons_and_exhausted_fail(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            fp = json.loads((d / "fixed-projects-summary.json").read_text(encoding="utf-8"))
            fp["source_resource_exhausted"] = ["proj-x"]
            fp["original_native"]["comparisons_passed"] = 150
            (d / "fixed-projects-summary.json").write_text(json.dumps(fp), encoding="utf-8")
            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("source_resource_exhausted" in e for e in errors))
            self.assertTrue(any("comparisons incomplete" in e for e in errors))

    def test_roam_unsupported_case_fails_and_records_item(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            rc = json.loads((d / "roam-callable-report.json").read_text(encoding="utf-8"))
            rc["status"] = "fail"
            rc["positive"]["passed"] = 27
            rc["cases"][0] = {
                "label": "hp-early",
                "status": "fail",
                "expected": {"kind": "lifecycle"},
                "detail": "Unsupported feature",
            }
            (d / "roam-callable-report.json").write_text(json.dumps(rc), encoding="utf-8")
            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(summary["status"], "fail")
            self.assertIn("roam-callable:hp-early:fail", summary["failed_items"])
            self.assertTrue(any("Unsupported" in e for e in errors))

    def test_roam_pass_case_with_unsupported_token_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            rc = json.loads((d / "roam-callable-report.json").read_text(encoding="utf-8"))
            rc["cases"][0]["diagnostic"] = "Unsupported fallback masked as pass"
            (d / "roam-callable-report.json").write_text(json.dumps(rc), encoding="utf-8")
            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("Unsupported" in e for e in errors))

    def test_roam_case_count_and_duplicate_id_mismatch_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            rc = json.loads((d / "roam-callable-report.json").read_text(encoding="utf-8"))
            rc["cases"] = [{"label": "only-one", "status": "pass", "expected": {"kind": "lifecycle"}}]
            (d / "roam-callable-report.json").write_text(json.dumps(rc), encoding="utf-8")
            rt = json.loads((d / "roam-task-report.json").read_text(encoding="utf-8"))
            rt["cases"][1]["label"] = rt["cases"][0]["label"]
            (d / "roam-task-report.json").write_text(json.dumps(rt), encoding="utf-8")
            summary, errors = mod.evaluate_receipts(d, repo=REPO_ROOT)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("cases count mismatch" in e for e in errors))
            self.assertTrue(any("duplicate case identity" in e for e in errors))

    def test_runner_mapping_covers_all_declared_apps_and_capabilities(self) -> None:
        app_ids, cap_ids = _declared_ids()
        self.assertEqual(set(runner.EVERYDAY_APP_TESTS.keys()), set(app_ids))
        self.assertEqual(set(runner.CAPABILITY_TARGETS.keys()), set(cap_ids))

        parsed_ok = runner.parse_libtest_stdout(
            "running 2 tests\ntest a ... ok\ntest b ... ok\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s\n",
            0,
        )
        self.assertEqual(parsed_ok["status"], "pass")
        self.assertEqual(parsed_ok["tests_passed"], 2)

        parsed_ign = runner.parse_libtest_stdout(
            "running 2 tests\ntest a ... ok\ntest b ... ignored\ntest result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.10s\n",
            0,
        )
        self.assertEqual(parsed_ign["status"], "fail")


if __name__ == "__main__":
    unittest.main()
