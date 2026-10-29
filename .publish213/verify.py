#!/usr/bin/env python3
"""Run each unchanged publication gate, then seal an exact-attempt receipt."""
import argparse
import datetime
import os
from pathlib import Path
import shutil
import subprocess
import sys
from publish import GATE_IDS, Publication, Refusal, digest, read_json, require, safe_output, write_json


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def commands(target, runner, clang):
    consumer = runner / "consumer208"
    rows = [
        ("format", ["cargo", "fmt", "--all", "--", "--check"], {}),
        ("workspace", ["cargo", "test", "--locked", "--workspace"], {}),
        ("reference", ["cargo", "test", "--locked", "--workspace", "--no-default-features"],
         {"CARGO_TARGET_DIR": str(runner / "target-reference")}),
        ("native_build", ["cargo", "build", "--locked", "--release", "-p", "zil-cli"], {}),
        ("consumer208", ["python3", "tools/research/208/validate.py", "--repo", ".", "--cli",
                         str(target / "release" / "zil"), "--clang", clang, "--out", str(consumer)], {}),
        ("clippy", ["cargo", "clippy", "--locked", "--workspace", "--all-targets", "--", "-D", "warnings"], {}),
    ]
    require([row[0] for row in rows] == GATE_IDS, "gate contract changed")
    return rows


def resources(root):
    disk = shutil.disk_usage(root)
    result = {"at": now(), "disk_free_bytes": disk.free, "disk_total_bytes": disk.total}
    mem = Path("/proc/meminfo")
    if mem.is_file():
        result["memory"] = [line for line in mem.read_text().splitlines()
                            if line.startswith(("MemTotal:", "MemAvailable:", "SwapFree:"))]
    paths = [root / name for name in ("target", "target-reference", "native-evidence", "consumer208")]
    paths = [str(path) for path in paths if path.exists()]
    if paths:
        try:
            p = subprocess.run(["du", "-sk", *paths], capture_output=True, text=True, timeout=15)
            result["directory_kib"] = p.stdout.splitlines()
        except (OSError, subprocess.TimeoutExpired) as error:
            result["directory_measurement_error"] = type(error).__name__
    return result


class Verification:
    def __init__(self, args):
        self.args = args
        self.publisher = Publication(args)
        prepared = read_json(args.receipt)
        require(prepared.get("stage") == "prepared", "not a preparation receipt")
        self.publisher.receipt_identity(prepared)
        self.publisher.clean_source()
        self.output = Path(args.out).resolve()
        self.output.mkdir(parents=True, exist_ok=True)
        require(not self.output.is_relative_to(self.publisher.repo), "receipts must be outside the source tree")
        self.runner = self.output.parent
        self.target = Path(os.environ["CARGO_TARGET_DIR"]).resolve()
        require(self.target == self.runner / "target", "unexpected cache root; refusing unsafe cleanup layout")
        require(args.run_id and args.run_attempt and args.run_id.isdigit() and args.run_attempt.isdigit(),
                "numeric run ID and attempt are required")
        self.identity = {**self.publisher.local_identity(),
                         "source_tree": self.publisher.git("rev-parse", "HEAD^{tree}").stdout.strip(),
                         "run_id": args.run_id, "run_attempt": args.run_attempt,
                         "verification_script_sha256": digest(__file__)}
        self.command_rows = commands(self.target, self.runner, os.environ["ZIL_CLANG"])
        self.path = self.output / "gates.json"

    def init(self):
        require(not self.path.exists(), "attempt already initialized; never overwrite prior evidence")
        require(not any((self.output / (gate + ".log")).exists() for gate in GATE_IDS), "old gate logs exist")
        write_json(self.path, {"schema": 1, "stage": "verified", **self.identity,
                              "status": "Running", "started_at": now(), "finalized": False,
                              "gates": [], "resources_initial": resources(self.runner)})
        return 0

    def load(self):
        receipt = read_json(self.path)
        for key, value in self.identity.items():
            require(receipt.get(key) == value, "verification identity mismatch: " + key)
        require(receipt.get("stage") == "verified", "invalid verification stage")
        return receipt

    def check_row(self, row, index):
        gate, command, overrides = self.command_rows[index]
        require(row.get("id") == gate and row.get("command") == command and
                row.get("environment_overrides") == overrides, "gate command/order differs: " + gate)
        require(row.get("log") == gate + ".log", "unexpected log path")
        logfile = self.output / row["log"]
        require(logfile.is_file() and not logfile.is_symlink() and digest(logfile) == row.get("log_sha256"),
                "gate log missing or changed: " + gate)
        require(row.get("status") in ("Pass", "Fail"), "gate is incomplete: " + gate)
        if row["status"] == "Pass":
            require(row.get("exit_code") == 0 and not row.get("error"), "invalid successful gate: " + gate)
        if gate == "consumer208" and row.get("summary_sha256"):
            summary = self.output / "consumer208.json"
            require(summary.is_file() and digest(summary) == row["summary_sha256"], "consumer summary changed")
            data = read_json(summary)
            if row["status"] == "Pass":
                require(data.get("status") == "Pass" and data.get("source_commit") == self.publisher.source,
                        "consumer source/status differs")
        elif gate == "consumer208" and row["status"] == "Pass":
            raise Refusal("successful consumer has no bound summary")

    def gate(self, name):
        receipt = self.load()
        require(not receipt.get("finalized"), "attempt has already been finalized")
        rows = receipt["gates"]
        require(len(rows) < len(GATE_IDS) and name == GATE_IDS[len(rows)], "gate repeated or out of order")
        for index, row in enumerate(rows):
            self.check_row(row, index)
        gate, command, overrides = self.command_rows[len(rows)]
        logfile = self.output / (gate + ".log")
        require(not logfile.exists(), "refusing to overwrite gate log")
        row = {"id": gate, "command": command, "environment_overrides": overrides,
               "status": "Running", "exit_code": None, "log": logfile.name,
               "started_at": now(), "resources_before": resources(self.runner)}
        rows.append(row)
        receipt["current_gate"] = gate
        receipt["status"] = "Fail" if any(r["status"] == "Fail" for r in rows) else "Running"
        write_json(self.path, receipt)
        print("::group::" + gate, flush=True)
        print("Command: " + " ".join(command), flush=True)
        environment = os.environ.copy()
        environment.update(overrides)
        error = None
        try:
            with logfile.open("x", encoding="utf-8", buffering=1) as stream:
                proc = subprocess.Popen(command, cwd=self.publisher.repo, env=environment,
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                        text=True, encoding="utf-8", errors="replace")
                for line in proc.stdout:
                    stream.write(line)
                    stream.flush()
                    print(line, end="", flush=True)
                row["exit_code"] = proc.wait()
            self.publisher.clean_source()
        except (OSError, Refusal) as caught:
            error = safe_output(str(caught))
        finally:
            if gate == "consumer208":
                summary = self.runner / "consumer208" / "summary.json"
                if summary.is_file():
                    data = summary.read_bytes()
                    temporary = self.output / "consumer208.json.tmp"
                    temporary.write_bytes(data)
                    temporary.replace(self.output / "consumer208.json")
                    row["summary_sha256"] = digest(self.output / "consumer208.json")
                    try:
                        report = read_json(self.output / "consumer208.json")
                        require(report.get("source_commit") == self.publisher.source, "consumer source mismatch")
                        require(report.get("status") == "Pass", "consumer did not report Pass")
                        require(report.get("cli_sha256") == digest(self.target / "release" / "zil"),
                                "consumer CLI hash mismatch")
                    except (Refusal, ValueError, OSError) as caught:
                        error = safe_output(str(caught))
                else:
                    error = error or "consumer summary is missing"
            row["status"] = "Pass" if row["exit_code"] == 0 and error is None else "Fail"
            if error:
                row["error"] = error
            row["ended_at"] = now()
            row["resources_after"] = resources(self.runner)
            if logfile.is_file():
                row["log_sha256"] = digest(logfile)
            receipt["status"] = "Fail" if any(r["status"] == "Fail" for r in rows) else "Running"
            receipt.pop("current_gate", None)
            write_json(self.path, receipt)
            print("::endgroup::", flush=True)
        return 0 if row["status"] == "Pass" else 1

    def finalize(self):
        receipt = self.load()
        require(not receipt.get("finalized"), "attempt already finalized")
        receipt["status"] = "Fail"
        write_json(self.path, receipt)
        try:
            require(len(receipt["gates"]) == len(GATE_IDS), "not all six gates completed")
            for index, row in enumerate(receipt["gates"]):
                self.check_row(row, index)
                require(row["status"] == "Pass" and row["exit_code"] == 0, "failed gate: " + row["id"])
            self.publisher.clean_source()
            receipt["status"] = "Pass"
        except Refusal as error:
            receipt["finalization_error"] = safe_output(str(error))
        receipt["finalized"] = True
        receipt["ended_at"] = now()
        write_json(self.path, receipt)
        return 0 if receipt["status"] == "Pass" else 1

    def cleanup(self, gate):
        require(gate in ("workspace", "reference"), "only completed test caches may be removed")
        require(self.args.artifact_id and self.args.artifact_digest,
                "successful receipt upload ID and digest are required before cache cleanup")
        receipt = self.load()
        index = GATE_IDS.index(gate)
        require(len(receipt["gates"]) > index, "gate not recorded")
        self.check_row(receipt["gates"][index], index)
        path = self.target / "debug" if gate == "workspace" else self.runner / "target-reference"
        require(path == path.resolve() and not path.is_symlink(), "cache path is a symlink")
        require(path.is_relative_to(self.runner) and not self.publisher.repo.is_relative_to(path)
                and not self.output.is_relative_to(path), "unsafe cache location")
        if path.exists():
            shutil.rmtree(path)
        receipt.setdefault("cache_cleanup", []).append({"gate": gate, "path": str(path), "at": now(),
            "receipt_artifact_id": self.args.artifact_id, "receipt_artifact_digest": self.args.artifact_digest,
            "upload_attestation": "workflow upload step output; not independently queried",
            "native_evidence_deleted": False, "resources_after": resources(self.runner)})
        write_json(self.path, receipt)
        return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("init", "gate", "finalize", "cleanup"))
    for name in ("repo", "manifest", "bundle", "repository", "transport-sha", "receipt", "out"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--gate", choices=GATE_IDS)
    parser.add_argument("--run-id", default=os.environ.get("GITHUB_RUN_ID", ""))
    parser.add_argument("--run-attempt", default=os.environ.get("GITHUB_RUN_ATTEMPT", ""))
    parser.add_argument("--artifact-id")
    parser.add_argument("--artifact-digest")
    args = parser.parse_args()
    if args.action in ("gate", "cleanup"):
        require(args.gate is not None, "--gate is required")
    verification = Verification(args)
    if args.action == "init":
        return verification.init()
    if args.action == "gate":
        return verification.gate(args.gate)
    if args.action == "finalize":
        return verification.finalize()
    return verification.cleanup(args.gate)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (Refusal, KeyError, ValueError, OSError) as error:
        print("VERIFICATION REFUSED: " + safe_output(str(error)), file=sys.stderr)
        sys.exit(1)
