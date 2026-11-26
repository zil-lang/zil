#!/usr/bin/env python3
"""Unit tests for tools/release/package_linux_alpha.py and verify_clean_install.py."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import package_linux_alpha as packager  # noqa: E402
import package_windows_alpha as win_packager  # noqa: E402
import verify_clean_install as verifier  # noqa: E402
import verify_clean_windows_install as win_verifier  # noqa: E402
import verify_published_assets as pub_verifier  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]


class ReleasePackagingTests(unittest.TestCase):
    def test_deterministic_packaging_and_tamper_detection(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake_bin = root / "zil"
            fake_bin.write_bytes(b"\x7fELFfake-zil-binary")
            fake_llvm_dir = root / "llvm-lib"
            fake_llvm_dir.mkdir()
            (fake_llvm_dir / "libLLVM.so.23.1").write_bytes(b"\x7fELFfake-libllvm-23")
            (fake_llvm_dir / "libLLVM-23.so").symlink_to("libLLVM.so.23.1")

            out1 = root / "out1"
            out2 = root / "out2"
            res1 = packager.package_release(
                repo=REPO_ROOT,
                zil_bin=fake_bin,
                llvm_lib_dir=fake_llvm_dir,
                out_dir=out1,
                version="alpha",
                source_commit="a" * 40,
                source_tree="b" * 40,
            )
            res2 = packager.package_release(
                repo=REPO_ROOT,
                zil_bin=fake_bin,
                llvm_lib_dir=fake_llvm_dir,
                out_dir=out2,
                version="alpha",
                source_commit="a" * 40,
                source_tree="b" * 40,
            )
            self.assertEqual(res1["archive_sha256"], res2["archive_sha256"])
            self.assertEqual(res1["manifest_sha256"], res2["manifest_sha256"])

            report, errors = verifier.verify_bundle(
                archive=out1 / "zil-alpha-linux-x86_64.tar.gz",
                sha256sums=out1 / "SHA256SUMS.txt",
                manifest_path=out1 / "RELEASE-MANIFEST.json",
                run_compiler_checks=False,
            )
            self.assertEqual(errors, [])
            self.assertEqual(report["status"], "pass")
            self.assertGreater(report["checks"]["manifest_inventory"]["verified_files"], 20)
            self.assertEqual(report["checks"]["manifest_inventory"]["verified_symlinks"], 1)

            # Tamper with SHA256SUMS and confirm fail-closed rejection.
            (out1 / "SHA256SUMS.txt").write_text(
                "0" * 64 + "  zil-alpha-linux-x86_64.tar.gz\n"
                + res1["manifest_sha256"] + "  RELEASE-MANIFEST.json\n",
                encoding="utf-8",
            )
            bad_report, bad_errors = verifier.verify_bundle(
                archive=out1 / "zil-alpha-linux-x86_64.tar.gz",
                sha256sums=out1 / "SHA256SUMS.txt",
                manifest_path=out1 / "RELEASE-MANIFEST.json",
                run_compiler_checks=False,
            )
            self.assertEqual(bad_report["status"], "fail")
            self.assertTrue(any("archive SHA-256 mismatch" in e for e in bad_errors))

    def test_windows_deterministic_zip_packaging_and_tamper_detection(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake_exe = root / "zil.exe"
            fake_exe.write_bytes(b"MZfake-zil-pe-binary")
            fake_dll = root / "LLVM-C.dll"
            fake_dll.write_bytes(b"MZfake-llvm-c-23-dll")

            out1 = root / "win1"
            out2 = root / "win2"
            res1 = win_packager.package_windows_release(
                repo=REPO_ROOT,
                zil_exe=fake_exe,
                llvm_dll=fake_dll,
                out_dir=out1,
                version="v1.0.0-alpha.2",
                source_commit="c" * 40,
                source_tree="d" * 40,
            )
            res2 = win_packager.package_windows_release(
                repo=REPO_ROOT,
                zil_exe=fake_exe,
                llvm_dll=fake_dll,
                out_dir=out2,
                version="v1.0.0-alpha.2",
                source_commit="c" * 40,
                source_tree="d" * 40,
            )
            self.assertEqual(res1["archive_sha256"], res2["archive_sha256"])
            self.assertEqual(res1["manifest_sha256"], res2["manifest_sha256"])

            report, errors = win_verifier.verify_windows_bundle(
                archive=out1 / "zil-v1.0.0-alpha.2-windows-x86_64.zip",
                sha256sums=out1 / "SHA256SUMS.txt",
                manifest_path=out1 / "RELEASE-MANIFEST.json",
                run_compiler_checks=False,
            )
            self.assertEqual(errors, [])
            self.assertEqual(report["status"], "pass")
            self.assertGreater(report["checks"]["manifest_inventory"]["verified_files"], 20)

            (out1 / "SHA256SUMS.txt").write_text(
                "0" * 64 + "  zil-v1.0.0-alpha.2-windows-x86_64.zip\n"
                + res1["manifest_sha256"] + "  RELEASE-MANIFEST.json\n",
                encoding="utf-8",
            )
            bad_report, bad_errors = win_verifier.verify_windows_bundle(
                archive=out1 / "zil-v1.0.0-alpha.2-windows-x86_64.zip",
                sha256sums=out1 / "SHA256SUMS.txt",
                manifest_path=out1 / "RELEASE-MANIFEST.json",
                run_compiler_checks=False,
            )
            self.assertEqual(bad_report["status"], "fail")
            self.assertTrue(any("archive SHA-256 mismatch" in e for e in bad_errors))

    def test_stage_and_verify_published_public_assets_linux_and_windows(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)

            # 1. Linux public staging + clean-directory verification (with real sha256sum -c).
            fake_bin = root / "zil"
            fake_bin.write_bytes(b"\x7fELFfake-zil-binary")
            fake_llvm_dir = root / "llvm-lib"
            fake_llvm_dir.mkdir()
            (fake_llvm_dir / "libLLVM.so.23.1").write_bytes(b"\x7fELFfake-libllvm-23")
            (fake_llvm_dir / "libLLVM-23.so").symlink_to("libLLVM.so.23.1")
            linux_bundle = root / "linux-bundle"
            packager.package_release(
                repo=REPO_ROOT,
                zil_bin=fake_bin,
                llvm_lib_dir=fake_llvm_dir,
                out_dir=linux_bundle,
                version="v1.0.0-alpha.2",
                source_commit="e" * 40,
                source_tree="f" * 40,
            )
            linux_receipt = root / "linux-receipt.json"
            linux_receipt.write_text('{"status":"pass"}\n', encoding="utf-8")
            linux_stage = root / "linux-stage"
            staged_linux = pub_verifier.stage_public_assets(
                platform="linux-x86_64",
                bundle_dir=linux_bundle,
                receipt_path=linux_receipt,
                stage_dir=linux_stage,
            )
            sums_linux = (linux_stage / "SHA256SUMS-linux-x86_64.txt").read_text(encoding="utf-8")
            self.assertIn("  RELEASE-MANIFEST-linux-x86_64.json\n", sums_linux)
            self.assertNotIn("  RELEASE-MANIFEST.json\n", sums_linux)
            report, errors = pub_verifier.verify_public_dir(
                download_dir=linux_stage,
                platform="linux-x86_64",
                expected_commit="e" * 40,
            )
            self.assertEqual(errors, [])
            self.assertEqual(report["status"], "pass")

            # Fail-closed check: stale un-renamed RELEASE-MANIFEST.json must be rejected.
            (linux_stage / "SHA256SUMS-linux-x86_64.txt").write_text(
                f"{staged_linux['archive_sha256']}  {staged_linux['archive_name']}\n"
                f"{staged_linux['manifest_sha256']}  RELEASE-MANIFEST.json\n",
                encoding="utf-8",
            )
            stale_report, stale_errors = pub_verifier.verify_public_dir(
                download_dir=linux_stage,
                platform="linux-x86_64",
                expected_commit="e" * 40,
            )
            self.assertEqual(stale_report["status"], "fail")
            self.assertTrue(any("missing public file" in e for e in stale_errors))

            # 2. Windows public staging + clean-directory verification.
            fake_exe = root / "zil.exe"
            fake_exe.write_bytes(b"MZfake-zil-pe-binary")
            fake_dll = root / "LLVM-C.dll"
            fake_dll.write_bytes(b"MZfake-llvm-c-23-dll")
            win_bundle = root / "win-bundle"
            win_packager.package_windows_release(
                repo=REPO_ROOT,
                zil_exe=fake_exe,
                llvm_dll=fake_dll,
                out_dir=win_bundle,
                version="v1.0.0-alpha.2",
                source_commit="1" * 40,
                source_tree="2" * 40,
            )
            win_receipt = root / "win-receipt.json"
            win_receipt.write_text('{"status":"pass"}\n', encoding="utf-8")
            extra_apps = root / "everyday-apps-receipt.json"
            extra_apps.write_text('{"status":"pass"}\n', encoding="utf-8")
            win_stage = root / "win-stage"
            pub_verifier.stage_public_assets(
                platform="windows-x86_64",
                bundle_dir=win_bundle,
                receipt_path=win_receipt,
                stage_dir=win_stage,
                extra_files=[(extra_apps, "windows-everyday-apps-receipt.json")],
            )
            sums_win = (win_stage / "SHA256SUMS-windows-x86_64.txt").read_text(encoding="utf-8")
            self.assertIn("  RELEASE-MANIFEST-windows-x86_64.json\n", sums_win)
            self.assertNotIn("  RELEASE-MANIFEST.json\n", sums_win)
            win_report, win_errors = pub_verifier.verify_public_dir(
                download_dir=win_stage,
                platform="windows-x86_64",
                expected_commit="1" * 40,
            )
            self.assertEqual(win_errors, [])
            self.assertEqual(win_report["status"], "pass")


if __name__ == "__main__":
    unittest.main()
