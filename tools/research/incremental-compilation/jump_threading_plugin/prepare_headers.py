#!/usr/bin/env python3
"""Extract exact LLVM SDK headers, including generated headers; never build LLVM."""
import argparse
import hashlib
import json
import shutil
import subprocess
from pathlib import Path


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--archive", type=Path)
    args = parser.parse_args()
    release = json.loads(args.release.read_text())
    assert release["version"] == "23.1.4", "Only the audited version is supported"
    args.cache.mkdir(parents=True, exist_ok=True)
    args.output.mkdir(parents=True, exist_ok=True)
    if (args.output / "include").exists():
        raise SystemExit("Refusing to replace an existing header tree")
    if shutil.disk_usage(args.cache).free < release["archive_bytes"] + 1_000_000_000:
        raise SystemExit("Insufficient room for verified archive and extracted headers")
    archive = args.archive or args.cache / "LLVM-23.1.4-Linux-X64.tar.zst"
    receipt = {
        "status": "preparing", "release": release,
        "release_manifest_sha256": sha(args.release),
        "prepare_script_sha256": sha(Path(__file__)), "commands": [],
    }
    receipt_path = args.output / "headers-receipt.json"

    def save():
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")

    def run(command, log):
        receipt["commands"].append(command)
        save()
        with (args.output / log).open("wb") as stream:
            result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT)
        if result.returncode:
            raise RuntimeError(f"Command failed ({result.returncode}); see {log}")

    save()
    try:
        if not archive.exists():
            if args.archive:
                raise FileNotFoundError(archive)
            partial = archive.with_suffix(archive.suffix + ".partial")
            run(["curl", "--fail", "--location", "--retry", "2", "--continue-at", "-",
                 "--connect-timeout", "15", "--max-time", "600", "--output",
                 str(partial), release["url"]], "download.log")
            partial.rename(archive)
        actual = {"path": str(archive.resolve()), "bytes": archive.stat().st_size,
                  "sha256": sha(archive)}
        receipt["archive"] = actual
        save()
        if (actual["bytes"] != release["archive_bytes"] or
                actual["sha256"] != release["archive_sha256"]):
            raise ValueError("SDK archive identity mismatch; no extraction performed")
        root = release["archive_root"]
        # One zstd decompressor. The official frame needs a 1 GiB window.
        run(["tar", "-I", "zstd --memory=2048MB", "-xf", str(archive),
             "--no-same-owner", "--no-same-permissions", "--strip-components=1",
             "-C", str(args.output), "--", root + "/include/llvm",
             root + "/include/llvm-c"], "extract.log")
        required = ["llvm/Config/abi-breaking.h", "llvm/Config/llvm-config.h",
                    "llvm/IR/IntrinsicEnums.inc", "llvm/Plugins/PassPlugin.h"]
        for name in required:
            if not (args.output / "include" / name).is_file():
                raise ValueError(f"Exact SDK is missing {name}")
        receipt["headers"] = {
            str(path.relative_to(args.output)): sha(path)
            for path in sorted((args.output / "include").rglob("*")) if path.is_file()
        }
        receipt["status"] = "Pass"
        save()
        print(json.dumps({"status": "Pass", "receipt": str(receipt_path),
                          "header_files": len(receipt["headers"])}))
    except Exception as error:
        receipt["status"] = "Fail"
        receipt["error"] = str(error)
        save()
        raise


if __name__ == "__main__":
    main()
