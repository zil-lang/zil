#!/usr/bin/env python3
"""Unit tests for tools/ci/alpha_acceptance_summary.py."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import alpha_acceptance_summary as mod  # noqa: E402


def _write_valid_receipts(d: Path) -> None:
    (d / "build-receipt.json").write_text(
        json.dumps(
            {
                "source_commit": "0" * 40,
                "source_tree": "1" * 40,
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


class AlphaAcceptanceSummaryTests(unittest.TestCase):
    def test_valid_receipts_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            summary, errors = mod.evaluate_receipts(d)
            self.assertEqual(errors, [])
            self.assertEqual(summary["status"], "pass")
            self.assertIn("Alpha Acceptance Evidence Summary (`PASS`)", mod.render_markdown(summary))

    def test_missing_report_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            (d / "roam-task-report.json").unlink()
            summary, errors = mod.evaluate_receipts(d)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("roam-task-report" in e for e in errors))

    def test_zero_executions_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            _write_valid_receipts(d)
            (d / "native-api-summary.json").write_text(
                json.dumps({"status": "Pass", "builds": 0, "executions": 0, "injected_failures": 0}),
                encoding="utf-8",
            )
            summary, errors = mod.evaluate_receipts(d)
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
            summary, errors = mod.evaluate_receipts(d)
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
            summary, errors = mod.evaluate_receipts(d)
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
            summary, errors = mod.evaluate_receipts(d)
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
            summary, errors = mod.evaluate_receipts(d)
            self.assertEqual(summary["status"], "fail")
            self.assertTrue(any("cases count mismatch" in e for e in errors))
            self.assertTrue(any("duplicate case identity" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
