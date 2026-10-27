"""Independent review: nested receiver argument failure and caller cleanup."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


parser = argparse.ArgumentParser()
parser.add_argument("--cli", type=pathlib.Path, required=True)
parser.add_argument("--repo", type=pathlib.Path, required=True)
parser.add_argument("--out", type=pathlib.Path, required=True)
args = parser.parse_args()
args.cli = args.cli.resolve()
args.repo = args.repo.resolve()
args.out = args.out.resolve()
args.out.mkdir(parents=True, exist_ok=False)
here = pathlib.Path(__file__).resolve().parent


def git(*parts):
    return subprocess.check_output(["git", "-C", str(args.repo), *parts], text=True).strip()


summary = {
    "status": "Running",
    "scope": "Independent four-module nested receiver argument-preparation error path",
    "source_commit": git("rev-parse", "HEAD"),
    "crates_tree": git("rev-parse", "HEAD:crates"),
    "source_status": git("status", "--porcelain", "--", "crates"),
    "cli_sha256": digest(args.cli),
    "sources_sha256": {f.name: digest(f) for f in sorted(here.glob("*.zil"))},
    "native": [],
}
write(args.out / "summary.json", summary)


def command(parts, name):
    argv = [str(x) for x in parts]
    receipt = {"command": argv, "status": "Running", "start": time.time()}
    path = args.out / (name + ".command.json")
    write(path, receipt)
    try:
        result = subprocess.run(argv, capture_output=True, timeout=45)
        (args.out / (name + ".stdout")).write_bytes(result.stdout)
        (args.out / (name + ".stderr")).write_bytes(result.stderr)
        receipt.update(status="Completed", exit_code=result.returncode)
        return result
    except Exception as error:
        receipt.update(status="InfraError", error_type=type(error).__name__, error=str(error))
        raise
    finally:
        receipt["end"] = time.time()
        write(path, receipt)


try:
    assert not summary["source_status"], "review requires clean compiler sources"
    for opt in ("O0", "O2"):
        phases = args.out / (opt + "-phases")
        executable = args.out / opt
        result = command([
            args.cli, "build", here / "main.zil", "--opt", opt,
            "-o", executable, "--emit-dir", phases, "--json",
        ], opt + "-build")
        report = json.loads(result.stdout)
        assert result.returncode == 0 and report["status"] == "Success" and report["verification_runs"] == 1, report
        result = command([executable], opt + "-run")
        assert result.returncode == 1 and result.stdout == b".", (result.returncode, result.stdout, result.stderr)
        probe = args.out / (opt + "-probe")
        result = command([
            "clang", "-" + opt, phases / "llvm.ll", args.repo / "tools/research/142/allocator_probe.c",
            "-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o", probe,
        ], opt + "-probe-link")
        assert result.returncode == 0, result.stderr
        result = command([probe], opt + "-probe-run")
        allocation = json.loads(result.stderr)
        assert result.returncode == 1 and result.stdout == b"." and allocation["allocated"] == allocation["frees"] == 1 and allocation["live"] == 0, (result.returncode, result.stdout, allocation)
        summary["native"].append({
            "optimization": opt,
            "native_executions": 2,
            "expected_error_exit": 1,
            "ticket_drop_observations_per_execution": 1,
            "verification_runs": report["verification_runs"],
            "llvm_sha256": digest(phases / "llvm.ll"),
            "allocation": allocation,
        })
        write(args.out / "summary.json", summary)
    assert git("rev-parse", "HEAD:crates") == summary["crates_tree"], "compiler sources changed during review"
    assert not git("status", "--porcelain", "--", "crates"), "compiler worktree changed during review"
    assert digest(args.cli) == summary["cli_sha256"], "compiler binary changed during review"
    assert {f.name: digest(f) for f in sorted(here.glob("*.zil"))} == summary["sources_sha256"], "consumer sources changed during review"
    summary["status"] = "Pass"
except Exception as error:
    summary.update(status="Fail", error_type=type(error).__name__, error=str(error))
    raise
finally:
    write(args.out / "summary.json", summary)
print(json.dumps(summary))
