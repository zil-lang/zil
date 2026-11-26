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
import verify_clean_install as verifier  # noqa: E402

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


if __name__ == "__main__":
    unittest.main()
