#!/usr/bin/env python3
"""Stage and independently verify public GitHub Release assets for ZIL Alpha releases.

Subcommands:
  stage      Verify internal packaged SHA256SUMS.txt, copy public platform-suffixed
             files into a staging directory, and write SHA256SUMS-<platform>.txt
             referencing the exact final public filenames.
  verify-dir Verify a clean download directory containing ONLY the public GitHub
             Release assets (archive, RELEASE-MANIFEST-<platform>.json,
             SHA256SUMS-<platform>.txt) using both Python SHA-256 checks and
             literal `sha256sum -c` when available on PATH.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path


PLATFORMS = {
    "linux-x86_64": {
        "archive_glob": "zil-*-linux-x86_64.tar.gz",
        "manifest_name": "RELEASE-MANIFEST-linux-x86_64.json",
        "sums_name": "SHA256SUMS-linux-x86_64.txt",
        "receipt_name": "clean-env-verify-receipt-linux-x86_64.json",
    },
    "windows-x86_64": {
        "archive_glob": "zil-*-windows-x86_64.zip",
        "manifest_name": "RELEASE-MANIFEST-windows-x86_64.json",
        "sums_name": "SHA256SUMS-windows-x86_64.txt",
        "receipt_name": "clean-env-verify-receipt-windows-x86_64.json",
    },
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_sha256sums(path: Path) -> list[tuple[str, str]]:
    entries: list[tuple[str, str]] = []
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw.strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) != 2 or len(parts[0]) != 64:
            raise ValueError(f"{path.name}:{lineno}: malformed SHA256SUMS line: {raw!r}")
        digest, filename = parts[0].lower(), parts[1]
        if "/" in filename or "\\" in filename or filename in (".", ".."):
            raise ValueError(f"{path.name}:{lineno}: non-basename filename in SHA256SUMS: {filename!r}")
        entries.append((digest, filename))
    if not entries:
        raise ValueError(f"{path.name}: empty checksum file")
    return entries


def stage_public_assets(
    *,
    platform: str,
    bundle_dir: Path,
    receipt_path: Path,
    stage_dir: Path,
    extra_files: list[tuple[Path, str]] | None = None,
) -> dict[str, object]:
    if platform not in PLATFORMS:
        raise ValueError(f"unsupported platform: {platform}")
    spec = PLATFORMS[platform]
    archives = sorted(bundle_dir.glob(spec["archive_glob"]))
    if len(archives) != 1:
        raise FileNotFoundError(
            f"expected exactly 1 archive matching {spec['archive_glob']} in {bundle_dir}, found {len(archives)}"
        )
    archive = archives[0]
    manifest = bundle_dir / "RELEASE-MANIFEST.json"
    internal_sums = bundle_dir / "SHA256SUMS.txt"
    for required in (manifest, internal_sums, receipt_path):
        if not required.is_file():
            raise FileNotFoundError(f"missing required release input: {required}")

    # Verify internal bundle integrity before staging public assets.
    actual_archive_sha = sha256_file(archive)
    actual_manifest_sha = sha256_file(manifest)
    internal_entries = dict((name, digest) for digest, name in parse_sha256sums(internal_sums))
    if internal_entries.get(archive.name) != actual_archive_sha:
        raise ValueError(
            f"internal SHA256SUMS mismatch for {archive.name}: "
            f"expected {internal_entries.get(archive.name)}, actual {actual_archive_sha}"
        )
    if internal_entries.get("RELEASE-MANIFEST.json") != actual_manifest_sha:
        raise ValueError(
            f"internal SHA256SUMS mismatch for RELEASE-MANIFEST.json: "
            f"expected {internal_entries.get('RELEASE-MANIFEST.json')}, actual {actual_manifest_sha}"
        )

    stage_dir.mkdir(parents=True, exist_ok=True)
    staged_archive = stage_dir / archive.name
    staged_manifest = stage_dir / spec["manifest_name"]
    staged_receipt = stage_dir / spec["receipt_name"]
    staged_sums = stage_dir / spec["sums_name"]

    shutil.copy2(archive, staged_archive)
    shutil.copy2(manifest, staged_manifest)
    shutil.copy2(receipt_path, staged_receipt)

    staged_files = [
        staged_archive.name,
        staged_sums.name,
        staged_manifest.name,
        staged_receipt.name,
    ]
    for src, dest_name in extra_files or []:
        if not src.is_file():
            raise FileNotFoundError(f"missing required extra release file: {src}")
        dest = stage_dir / dest_name
        shutil.copy2(src, dest)
        staged_files.append(dest.name)

    # Write public SHA256SUMS referencing the EXACT final public asset filenames.
    public_sums_text = (
        f"{actual_archive_sha}  {staged_archive.name}\n"
        f"{actual_manifest_sha}  {staged_manifest.name}\n"
    )
    staged_sums.write_text(public_sums_text, encoding="utf-8")

    return {
        "platform": platform,
        "archive_name": staged_archive.name,
        "archive_sha256": actual_archive_sha,
        "manifest_name": staged_manifest.name,
        "manifest_sha256": actual_manifest_sha,
        "sums_name": staged_sums.name,
        "staged_files": staged_files,
    }


def verify_public_dir(
    *,
    download_dir: Path,
    platform: str,
    expected_commit: str | None = None,
    require_sha256sum_cli: bool = False,
) -> tuple[dict[str, object], list[str]]:
    if platform not in PLATFORMS:
        raise ValueError(f"unsupported platform: {platform}")
    spec = PLATFORMS[platform]
    errors: list[str] = []

    sums_path = download_dir / spec["sums_name"]
    manifest_path = download_dir / spec["manifest_name"]
    archives = sorted(download_dir.glob(spec["archive_glob"]))

    if not sums_path.is_file():
        errors.append(f"missing public checksum file: {sums_path.name}")
    if not manifest_path.is_file():
        errors.append(f"missing public manifest file: {manifest_path.name}")
    if len(archives) != 1:
        errors.append(
            f"expected exactly 1 public archive matching {spec['archive_glob']}, found {len(archives)}"
        )

    checked_files: dict[str, str] = {}
    if not errors:
        archive_path = archives[0]
        try:
            entries = parse_sha256sums(sums_path)
        except ValueError as exc:
            errors.append(str(exc))
            entries = []

        seen_names: set[str] = set()
        for expected_digest, filename in entries:
            if filename in seen_names:
                errors.append(f"duplicate filename in {sums_path.name}: {filename}")
                continue
            seen_names.add(filename)
            target = download_dir / filename
            if not target.is_file():
                errors.append(
                    f"{sums_path.name} references missing public file {filename!r} in {download_dir}"
                )
                continue
            actual_digest = sha256_file(target)
            if actual_digest != expected_digest:
                errors.append(
                    f"SHA-256 mismatch for {filename}: expected {expected_digest}, actual {actual_digest}"
                )
            else:
                checked_files[filename] = actual_digest

        for required_name in (archive_path.name, manifest_path.name):
            if required_name not in seen_names:
                errors.append(f"{sums_path.name} does not list required public file {required_name!r}")

        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if manifest.get("archive_name") != archive_path.name:
            errors.append(
                f"manifest archive_name mismatch: expected {archive_path.name!r}, "
                f"got {manifest.get('archive_name')!r}"
            )
        if expected_commit and manifest.get("source_commit") != expected_commit:
            errors.append(
                f"manifest source_commit mismatch: expected {expected_commit!r}, "
                f"got {manifest.get('source_commit')!r}"
            )

        sha256sum_bin = shutil.which("sha256sum")
        if sha256sum_bin:
            proc = subprocess.run(
                [sha256sum_bin, "-c", sums_path.name],
                cwd=download_dir,
                capture_output=True,
                text=True,
                check=False,
            )
            if proc.returncode != 0:
                errors.append(
                    f"sha256sum -c {sums_path.name} failed (exit {proc.returncode}): "
                    f"{proc.stdout.strip()} {proc.stderr.strip()}".strip()
                )
        elif require_sha256sum_cli:
            errors.append("sha256sum CLI not found on PATH")

    report = {
        "status": "pass" if not errors else "fail",
        "platform": platform,
        "sums_file": sums_path.name,
        "manifest_file": manifest_path.name,
        "checked_files": checked_files,
        "errors": errors,
    }
    return report, errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    stage_p = sub.add_parser("stage", help="Stage public release assets with matching SHA256SUMS")
    stage_p.add_argument("--platform", required=True, choices=sorted(PLATFORMS))
    stage_p.add_argument("--bundle-dir", required=True, type=Path)
    stage_p.add_argument("--receipt", required=True, type=Path)
    stage_p.add_argument("--stage-dir", required=True, type=Path)
    stage_p.add_argument(
        "--extra-file",
        action="append",
        default=[],
        help="Optional extra file mapping SRC=DEST_NAME to copy into --stage-dir",
    )

    verify_p = sub.add_parser("verify-dir", help="Verify downloaded public GitHub Release assets")
    verify_p.add_argument("--platform", required=True, choices=sorted(PLATFORMS))
    verify_p.add_argument("--dir", required=True, type=Path)
    verify_p.add_argument("--expected-commit", default=None)
    verify_p.add_argument("--require-sha256sum-cli", action="store_true")

    args = parser.parse_args()
    if args.command == "stage":
        extras: list[tuple[Path, str]] = []
        for spec in args.extra_file:
            if "=" not in spec:
                raise SystemExit(f"invalid --extra-file (expected SRC=DEST): {spec!r}")
            src_s, dest_name = spec.split("=", 1)
            extras.append((Path(src_s), dest_name))
        result = stage_public_assets(
            platform=args.platform,
            bundle_dir=args.bundle_dir,
            receipt_path=args.receipt,
            stage_dir=args.stage_dir,
            extra_files=extras,
        )
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0

    report, errors = verify_public_dir(
        download_dir=args.dir,
        platform=args.platform,
        expected_commit=args.expected_commit,
        require_sha256sum_cli=args.require_sha256sum_cli,
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if not errors else 1


if __name__ == "__main__":
    sys.exit(main())
