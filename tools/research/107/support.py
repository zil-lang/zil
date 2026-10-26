"""Owned 107 process/archive helpers; historical 100 behavior is bound in provenance."""
from __future__ import annotations
import gzip, hashlib, io, json, os, shutil, subprocess, tarfile, time
from pathlib import Path
BASE = "b9cce1d1b7301850a3e65b81323caea8ea88593c"
SOURCE_BASE = "24ca9583d8789cfdfe3ccdbfe3eeff614bf56429"
ENV_KEYS = ["HOME", "PATH", "LD_LIBRARY_PATH", "LIBRARY_PATH", "C_INCLUDE_PATH", "CPATH", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME", "ZIL_CLANG", "LANG", "LC_ALL"]
ENV = {key: os.environ[key] for key in ENV_KEYS if key in os.environ}
ENV.update(CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="2", CARGO_PROFILE_RELEASE_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CARGO_PROFILE_DEV_DEBUG="0")
ENV["ZIL_CLANG"] = str(Path(shutil.which(ENV.get("ZIL_CLANG", "clang"))).resolve())

def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def write(path, value):
    path = Path(path); path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
def inventory(root):
    return {str(p.relative_to(root)): sha(p) for p in sorted(root.rglob("*")) if p.is_file()}
def invoke(argv, cwd, prefix, env=None, timeout=600, required=True, stdin=None):
    argv = list(map(str, argv)); prefix = Path(prefix); prefix.parent.mkdir(parents=True, exist_ok=True)
    assert not any(prefix.with_suffix(suffix).exists() for suffix in [".stdout", ".stderr", ".command.json"]), "retain prior process attempt"
    began = time.perf_counter_ns()
    try:
        result = subprocess.run(argv, cwd=cwd, env=ENV | (env or {}), input=stdin,
                                capture_output=True, timeout=timeout)
        record = dict(argv=argv, cwd=str(cwd), exit=result.returncode, elapsed_ns=time.perf_counter_ns()-began)
        stdout, stderr = result.stdout, result.stderr
    except subprocess.TimeoutExpired as e:
        record = dict(argv=argv, cwd=str(cwd), exit=None, outcome="Timeout", elapsed_ns=time.perf_counter_ns()-began)
        stdout, stderr = e.stdout or b"", e.stderr or b""
    prefix.with_suffix(".stdout").write_bytes(stdout); prefix.with_suffix(".stderr").write_bytes(stderr)
    write(prefix.with_suffix(".command.json"), record)
    if required and record["exit"] != 0: raise RuntimeError(f"failed; preserved {prefix}: {stderr[-1600:]!r}")
    return record
def compact_report(path):
    raw=path.read_bytes();packed=gzip.compress(raw,compresslevel=6,mtime=0)
    assert gzip.decompress(packed)==raw
    destination=path.with_suffix(path.suffix+".gz")
    assert not destination.exists() and not (path.parent/"report-storage.json").exists()
    destination.write_bytes(packed)
    write(path.parent/"report-storage.json",dict(original=path.name,original_bytes=len(raw),original_sha256=hashlib.sha256(raw).hexdigest(),compressed=destination.name,compressed_sha256=sha(destination)))
    path.unlink()

def archive_directory(folder):
    entries={str(p.relative_to(folder)):(p.stat().st_size,sha(p)) for p in sorted(folder.rglob("*")) if p.is_file()}
    archive=Path(str(folder)+".tar.gz")
    assert not archive.exists() and not Path(str(folder)+".archive.json").exists()
    with tarfile.open(archive,"w:gz",compresslevel=6) as tar:tar.add(folder,arcname=folder.name)
    observed={}
    with tarfile.open(archive,"r:gz") as tar:
        for member in tar:
            if member.isdir():continue
            assert member.isfile()
            rel=Path(member.name).relative_to(folder.name).as_posix()
            data=tar.extractfile(member).read();observed[rel]=(len(data),hashlib.sha256(data).hexdigest())
    assert observed==entries
    write(Path(str(folder)+".archive.json"),dict(archive=archive.name,sha256=sha(archive),files=entries,verified_every_member=True))
    shutil.rmtree(folder)

def extract(repo, commit, paths, destination):
    raw = subprocess.check_output(["git", "archive", "--format=tar", commit, *paths], cwd=repo)
    with tarfile.open(fileobj=io.BytesIO(raw)) as tar:
        assert all(not Path(m.name).is_absolute() and ".." not in Path(m.name).parts
                   and (m.isfile() or m.isdir()) for m in tar.getmembers())
        tar.extractall(destination, filter="data")


def normalize(row):
    report = row["report"]
    value = {k: v for k, v in report.items() if k not in ["seconds", "commands", "artifacts"]}
    value["artifacts"] = {k: sha(p) for k, p in report["artifacts"].items() if k != "executable"}
    value.update({k: v for k, v in row.items() if k not in ["report", "api_wall_ns", "counts", "raw_snapshots", "observation_ns"]})
    return value
