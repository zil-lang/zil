#!/usr/bin/env python3
"""Reproducible large-project multi-point edit, disk-footprint, LRU-eviction,
and multi-process shared-cache benchmark harness: ZIL vs rustc.

Note: This harness measures synthetic multi-module call chains (aligned opt-level,
debuginfo=0, overflow-checks=yes, panic=abort, codegen-units, and incremental/object
cache). It is intended for reproducible regression & scaling comparisons rather than
claiming universal dominance across arbitrary real-world generic/trait dependency graphs.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import platform
import random
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def build_env(llvm_dir: Path | None) -> dict[str, str]:
    env = os.environ.copy()
    if llvm_dir is not None and llvm_dir.exists():
        lib_dir = str(llvm_dir / "lib")
        env["ZIL_LLVM_LIB_DIR"] = lib_dir
        prev_ld = env.get("LD_LIBRARY_PATH", "")
        env["LD_LIBRARY_PATH"] = f"{lib_dir}:{prev_ld}" if prev_ld else lib_dir
        clang = llvm_dir / "bin" / "clang"
        if clang.exists():
            env["ZIL_CLANG"] = str(clang)
    return env


def dir_stats(path: Path) -> tuple[int, int]:
    """Return (total_bytes, file_count) recursively under path."""
    if not path.exists():
        return 0, 0
    total = 0
    count = 0
    for root, _, files in os.walk(path):
        for f in files:
            p = os.path.join(root, f)
            try:
                st = os.lstat(p)
                total += st.st_size
                count += 1
            except OSError:
                pass
    return total, count


def dir_manifest(path: Path) -> list[dict[str, object]]:
    """Return sorted relative file sizes under path for reproducible inspection."""
    items: list[dict[str, object]] = []
    if not path.exists():
        return items
    for root, _, files in os.walk(path):
        for f in sorted(files):
            p = Path(root) / f
            try:
                st = p.lstat()
                items.append({"rel": str(p.relative_to(path)), "bytes": st.st_size})
            except OSError:
                pass
    items.sort(key=lambda x: str(x["rel"]))
    return items


def timed_wait_json(cmd: list[str], tmp_out: Path, env: dict[str, str]) -> tuple[float, int, dict | None]:
    err_path = Path(f"{tmp_out}.err")
    with open(tmp_out, "wb") as out_f, open(err_path, "wb") as err_f:
        t0 = time.perf_counter()
        proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=out_f, stderr=err_f, env=env)
        if hasattr(os, "wait4"):
            _, status, usage = os.wait4(proc.pid, 0)
            rc = os.waitstatus_to_exitcode(status)
            rss_kib = int(usage.ru_maxrss)
        else:
            rc = proc.wait()
            rss_kib = 0
        dt_ms = (time.perf_counter() - t0) * 1000.0
    if rc != 0:
        err_txt = err_path.read_text(errors="replace")
        out_txt = tmp_out.read_text(errors="replace")
        raise RuntimeError(
            f"command failed ({rc}): {' '.join(cmd)}\nstdout={out_txt[:600]}\nstderr={err_txt[:600]}"
        )
    raw = tmp_out.read_text(errors="replace").strip()
    data = None
    if raw.startswith("{"):
        try:
            data = json.loads(raw)
        except Exception:
            data = None
    return dt_ms, rss_kib, data


def zil_module_src(m: int, *, salt: int = 0, extra_helper: bool = False, extra_param: bool = False) -> str:
    group_start = (m // 10) * 10
    has_prev = m > group_start
    imp = f"import m{m - 1};\n" if has_prev else ""
    extra_fn = (
        f"fn priv_extra_{m}(v: i64) -> i64 {{\n  return (v * 3) + {salt + 7};\n}}\n"
        if extra_helper
        else ""
    )
    helper_call = f"acc = acc + priv_extra_{m}(acc % 101);" if extra_helper else ""
    prev_call = f"  acc = acc + m{m - 1}.compute(acc % 997);\n" if has_prev else ""
    if extra_param:
        sig = "pub fn compute(x: i64, bias: i64) -> i64"
        bias_add = " + bias"
    else:
        sig = "pub fn compute(x: i64) -> i64"
        bias_add = ""
    return (
        f"{imp}"
        f"{extra_fn}"
        f"fn priv_step(x: i64) -> i64 {{\n"
        f"  var acc: i64 = x + {salt + m + 1};\n"
        f"  var i: i64 = 0;\n"
        f"  while i < 6 {{\n"
        f"    acc = acc + (i * {(m % 17) + 3}) + {(salt % 13) + 1};\n"
        f"    {helper_call}\n"
        f"    i = i + 1;\n"
        f"  }}\n"
        f"  return acc;\n"
        f"}}\n"
        f"{sig} {{\n"
        f"  var acc: i64 = priv_step(x){bias_add};\n"
        f"{prev_call}"
        f"  return acc;\n"
        f"}}\n"
    )


def rust_module_src(m: int, *, salt: int = 0, extra_helper: bool = False, extra_param: bool = False) -> str:
    group_start = (m // 10) * 10
    has_prev = m > group_start
    extra_fn = (
        f"fn priv_extra_{m}(v: i64) -> i64 {{\n  (v * 3) + {salt + 7}\n}}\n"
        if extra_helper
        else ""
    )
    helper_call = f"acc = acc + priv_extra_{m}(acc % 101);" if extra_helper else ""
    prev_call = f"  acc = acc + crate::m{m - 1}::compute(acc % 997);\n" if has_prev else ""
    if extra_param:
        sig = "pub fn compute(x: i64, bias: i64) -> i64"
        bias_add = " + bias"
    else:
        sig = "pub fn compute(x: i64) -> i64"
        bias_add = ""
    return (
        f"{extra_fn}"
        f"fn priv_step(x: i64) -> i64 {{\n"
        f"  let mut acc: i64 = x + {salt + m + 1};\n"
        f"  let mut i: i64 = 0;\n"
        f"  while i < 6 {{\n"
        f"    acc = acc + (i * {(m % 17) + 3}) + {(salt % 13) + 1};\n"
        f"    {helper_call}\n"
        f"    i = i + 1;\n"
        f"  }}\n"
        f"  acc\n"
        f"}}\n"
        f"{sig} {{\n"
        f"  let mut acc: i64 = priv_step(x){bias_add};\n"
        f"{prev_call}"
        f"  acc\n"
        f"}}\n"
    )


def write_mains(
    zdir: Path,
    rdir: Path,
    n_mods: int,
    *,
    extra_mod: str | None = None,
    skip_tail: int | None = None,
    iface_tail: int | None = None,
) -> None:
    group_tails = [min(g + 9, n_mods - 1) for g in range(0, n_mods, 10) if min(g + 9, n_mods - 1) != skip_tail]
    z_imports = [f"import m{t};" for t in group_tails]
    z_calls = [
        f"  sum = sum + m{t}.compute(sum % 997, 5);" if (iface_tail == t)
        else f"  sum = sum + m{t}.compute(sum % 997);"
        for t in group_tails
    ]
    if extra_mod:
        z_imports.append(f"import {extra_mod};")
        z_calls.append(f"  sum = sum + {extra_mod}.compute(sum % 997);")
    (zdir / "main.zil").write_text(
        "\n".join(z_imports)
        + "\npub fn main() -> i32 {\n  var sum: i64 = 1;\n"
        + "\n".join(z_calls)
        + "\n  return i32(sum % 127);\n}\n"
    )

    r_mods = [f"mod m{m};" for m in range(n_mods) if m != skip_tail]
    r_calls = [
        f"  sum = sum + m{t}::compute(sum % 997, 5);" if (iface_tail == t)
        else f"  sum = sum + m{t}::compute(sum % 997);"
        for t in group_tails
    ]
    if extra_mod:
        r_mods.append(f"mod {extra_mod};")
        r_calls.append(f"  sum = sum + {extra_mod}::compute(sum % 997);")
    (rdir / "main.rs").write_text(
        "\n".join(r_mods)
        + "\nfn main() {\n  let mut sum: i64 = 1;\n"
        + "\n".join(r_calls)
        + "\n  std::process::exit((sum % 127) as i32);\n}\n"
    )


def generate_projects(root: Path, n_mods: int) -> tuple[Path, Path]:
    zdir = root / f"zil_{n_mods}"
    rdir = root / f"rs_{n_mods}"
    zdir.mkdir(parents=True, exist_ok=True)
    rdir.mkdir(parents=True, exist_ok=True)
    for m in range(n_mods):
        (zdir / f"m{m}.zil").write_text(zil_module_src(m))
        (rdir / f"m{m}.rs").write_text(rust_module_src(m))
    write_mains(zdir, rdir, n_mods)
    return zdir, rdir


def run_zil_build(
    zil_bin: Path,
    src: Path,
    out_exe: Path,
    cache_dir: Path,
    *,
    opt: str,
    cgu: int,
    jobs: int,
    tmp_json: Path,
    env: dict[str, str],
    extra_flags: list[str] | None = None,
) -> dict[str, object]:
    cmd = [
        str(zil_bin),
        "build",
        str(src),
        "--opt",
        opt,
        "--codegen-units",
        str(cgu),
        "--codegen-jobs",
        str(jobs),
        "--object-cache",
        str(cache_dir),
        "--json",
        "-o",
        str(out_exe),
    ]
    if extra_flags:
        cmd.extend(extra_flags)
    dt_ms, rss_kib, rep = timed_wait_json(cmd, tmp_json, env)
    hits, misses, total_units = 0, 0, 0
    evicted_entries, evicted_bytes, evict_ns = 0, 0, 0
    if rep and rep.get("native_codegen"):
        nc = rep["native_codegen"]
        total_units = nc.get("actual_units", 0)
        evicted_entries = int(nc.get("cache_evicted_entries", 0) or 0)
        evicted_bytes = int(nc.get("cache_evicted_bytes", 0) or 0)
        evict_ns = int(nc.get("cache_evict_ns", 0) or 0)
        for obj_s in nc.get("object_reports", []):
            is_hit = (
                obj_s.get("input_kind") == "object-cache-hit"
                if isinstance(obj_s, dict)
                else ("object-cache-hit" in str(obj_s))
            )
            if is_hit:
                hits += 1
            else:
                misses += 1
    miss_rate = (misses / total_units * 100.0) if total_units else 0.0
    return {
        "ms": round(dt_ms, 2),
        "rss_mb": round(rss_kib / 1024.0, 2),
        "units": total_units,
        "hits": hits,
        "misses": misses,
        "miss_pct": round(miss_rate, 2),
        "cache_evicted_entries": evicted_entries,
        "cache_evicted_bytes": evicted_bytes,
        "cache_evict_ns": evict_ns,
        "disk_cache": rep.get("disk_cache") if rep else None,
        "verification_runs": rep.get("verification_runs") if rep else None,
    }


def run_rust_build(
    rustc_bin: Path,
    src: Path,
    out_exe: Path,
    inc_dir: Path,
    *,
    opt: str,
    cgu: int,
    tmp_json: Path,
    env: dict[str, str],
) -> dict[str, object]:
    opt_lvl = "0" if opt == "O0" else "2"
    cmd = [
        str(rustc_bin),
        "--edition=2024",
        "-C",
        f"opt-level={opt_lvl}",
        "-C",
        "debuginfo=0",
        "-C",
        "overflow-checks=yes",
        "-C",
        "panic=abort",
        "-C",
        f"codegen-units={cgu}",
        "-C",
        f"incremental={inc_dir}",
        str(src),
        "-o",
        str(out_exe),
    ]
    dt_ms, rss_kib, _ = timed_wait_json(cmd, tmp_json, env)
    return {"ms": round(dt_ms, 2), "rss_mb": round(rss_kib / 1024.0, 2)}


def fmt_mb(n_bytes: int) -> str:
    return f"{n_bytes / (1024 * 1024):6.2f}MB"


def run_scale_suite(
    zil_bin: Path,
    rustc_bin: Path,
    scales: list[int],
    opt: str,
    jobs: int,
    samples: int,
    extra_flags: list[str],
    env: dict[str, str],
) -> dict[str, object]:
    n_samples = max(1, samples)
    print(
        f"\n================ EXPERIMENT A: MULTI-POINT CONCURRENT EDITS ({opt}, jobs={jobs}, samples={n_samples}) ================"
    )
    out_data: dict[str, object] = {}
    for n_mods in scales:
        cgu = 16 if n_mods <= 100 else (32 if n_mods <= 500 else 64)
        rows: list[dict[str, object]] = []
        with tempfile.TemporaryDirectory(prefix=f"zil_scale_{n_mods}_") as td_s:
            td = Path(td_s)
            zdir, rdir = generate_projects(td, n_mods)
            z_out_dir = td / "zil_target"
            r_out_dir = td / "rust_target"
            z_cache = z_out_dir / "objcache"
            r_inc = r_out_dir / "incremental"
            z_out_dir.mkdir()
            r_out_dir.mkdir()
            tmp_json = td / "out.json"

            print(f"\n--- {n_mods} modules (CGU={cgu}, opt={opt}, debuginfo=0, zil_jobs={jobs}, samples={n_samples}) ---")
            print(
                f"{'Scenario':<28} | {'ZIL ms':>8} {'Rust ms':>8} {'Z/R':>6} | "
                f"{'Recomp':>7} {'Miss%':>6} | {'RSS Z/R MB':>12} | {'Cache Z/R':>17} | {'Total Z/R':>17}"
            )
            print("-" * 130)

            def measure_samples(label: str, mutate_fn) -> None:
                z_runs, r_runs = [], []
                for s_idx in range(n_samples):
                    mutate_fn(s_idx)
                    zr = run_zil_build(
                        zil_bin, zdir / "main.zil", z_out_dir / "app", z_cache,
                        opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_json, env=env, extra_flags=extra_flags,
                    )
                    rr = run_rust_build(
                        rustc_bin, rdir / "main.rs", r_out_dir / "app", r_inc,
                        opt=opt, cgu=cgu, tmp_json=tmp_json, env=env,
                    )
                    z_runs.append(zr)
                    r_runs.append(rr)
                z_med_ms = statistics.median(float(x["ms"]) for x in z_runs)
                r_med_ms = statistics.median(float(x["ms"]) for x in r_runs)
                z_rss_med = statistics.median(float(x["rss_mb"]) for x in z_runs)
                r_rss_med = statistics.median(float(x["rss_mb"]) for x in r_runs)
                last_z = z_runs[-1]
                zc_b, zc_f = dir_stats(z_cache)
                rc_b, rc_f = dir_stats(r_inc)
                zt_b, _ = dir_stats(z_out_dir)
                rt_b, _ = dir_stats(r_out_dir)
                ratio = z_med_ms / r_med_ms
                print(
                    f"{label:<28} | {z_med_ms:8.1f} {r_med_ms:8.1f} {ratio:5.2f}x | "
                    f"{last_z['misses']:2d}/{last_z['units']:<4d} {last_z['miss_pct']:5.1f}% | "
                    f"{z_rss_med:5.1f}/{r_rss_med:<6.1f} | "
                    f"{fmt_mb(zc_b)}/{fmt_mb(rc_b)} | {fmt_mb(zt_b)}/{fmt_mb(rt_b)}"
                )
                rows.append({
                    "scenario": label,
                    "samples": n_samples,
                    "zil_median_ms": round(z_med_ms, 2),
                    "rust_median_ms": round(r_med_ms, 2),
                    "zil_raw_ms": [x["ms"] for x in z_runs],
                    "rust_raw_ms": [x["ms"] for x in r_runs],
                    "zil_last": last_z,
                    "rust_last": r_runs[-1],
                    "ratio_z_over_r": round(ratio, 3),
                    "disk_bytes": {
                        "zil_cache": zc_b,
                        "zil_cache_files": zc_f,
                        "zil_total": zt_b,
                        "rust_incremental": rc_b,
                        "rust_incremental_files": rc_f,
                        "rust_total": rt_b,
                    },
                })

            def cold_mutate(s_idx: int) -> None:
                if s_idx > 0:
                    shutil.rmtree(z_cache, ignore_errors=True)
                    shutil.rmtree(r_inc, ignore_errors=True)

            measure_samples("1. cold_build", cold_mutate)
            measure_samples("2. warm_noop", lambda _: None)

            def priv_body(s_idx: int) -> None:
                (zdir / "m0.zil").write_text(zil_module_src(0, salt=10 + s_idx))
                (rdir / "m0.rs").write_text(rust_module_src(0, salt=10 + s_idx))

            measure_samples("3a. priv_edit_1_body", priv_body)

            def priv_add_fn(s_idx: int) -> None:
                (zdir / "m0.zil").write_text(zil_module_src(0, salt=50 + s_idx, extra_helper=True))
                (rdir / "m0.rs").write_text(rust_module_src(0, salt=50 + s_idx, extra_helper=True))

            measure_samples("3b. priv_edit_1_add_fn", priv_add_fn)

            def add_mod(s_idx: int) -> None:
                mod_name = f"m_extra_{s_idx}"
                (zdir / f"{mod_name}.zil").write_text(
                    f"pub fn compute(x: i64) -> i64 {{\n  return x + {19 + s_idx};\n}}\n"
                )
                (rdir / f"{mod_name}.rs").write_text(
                    f"pub fn compute(x: i64) -> i64 {{\n  x + {19 + s_idx}\n}}\n"
                )
                write_mains(zdir, rdir, n_mods, extra_mod=mod_name)

            measure_samples("3c. add_new_module", add_mod)

            def rem_mod(s_idx: int) -> None:
                if s_idx > 0:
                    add_mod(s_idx)
                    run_zil_build(
                        zil_bin, zdir / "main.zil", z_out_dir / "app", z_cache,
                        opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_json, env=env, extra_flags=extra_flags,
                    )
                    run_rust_build(
                        rustc_bin, rdir / "main.rs", r_out_dir / "app", r_inc,
                        opt=opt, cgu=cgu, tmp_json=tmp_json, env=env,
                    )
                write_mains(zdir, rdir, n_mods)

            measure_samples("3d. remove_module", rem_mod)

            k10 = max(1, n_mods // 10)
            k30 = max(1, (n_mods * 3) // 10)

            def edit_10_clust(s_idx: int) -> None:
                for m in range(k10):
                    (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=100 + s_idx))
                    (rdir / f"m{m}.rs").write_text(rust_module_src(m, salt=100 + s_idx))

            measure_samples("4a. priv_edit_10%_clustered", edit_10_clust)

            def edit_10_scat(s_idx: int) -> None:
                rng = random.Random(42 + s_idx)
                for m in rng.sample(range(n_mods), k10):
                    (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=200 + s_idx))
                    (rdir / f"m{m}.rs").write_text(rust_module_src(m, salt=200 + s_idx))

            measure_samples("4b. priv_edit_10%_scattered", edit_10_scat)

            def edit_30_clust(s_idx: int) -> None:
                for m in range(k30):
                    (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=300 + s_idx))
                    (rdir / f"m{m}.rs").write_text(rust_module_src(m, salt=300 + s_idx))

            measure_samples("5a. priv_edit_30%_clustered", edit_30_clust)

            def edit_30_scat(s_idx: int) -> None:
                rng = random.Random(142 + s_idx)
                for m in rng.sample(range(n_mods), k30):
                    (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=400 + s_idx))
                    (rdir / f"m{m}.rs").write_text(rust_module_src(m, salt=400 + s_idx))

            measure_samples("5b. priv_edit_30%_scattered", edit_30_scat)

            def iface_edit(s_idx: int) -> None:
                extra_p = (s_idx % 2) == 0
                (zdir / "m9.zil").write_text(zil_module_src(9, salt=500 + s_idx, extra_param=extra_p))
                (rdir / "m9.rs").write_text(rust_module_src(9, salt=500 + s_idx, extra_param=extra_p))
                write_mains(zdir, rdir, n_mods, iface_tail=(9 if extra_p else None))

            measure_samples("6. public_iface_edit", iface_edit)

            out_data[str(n_mods)] = {
                "cgu": cgu,
                "samples": n_samples,
                "rows": rows,
                "end_zil_manifest_count": len(dir_manifest(z_out_dir)),
                "end_rust_manifest_count": len(dir_manifest(r_out_dir)),
            }
    return out_data


def run_multi_round_disk_suite(
    zil_bin: Path,
    rustc_bin: Path,
    n_mods: int,
    rounds: int,
    opt: str,
    jobs: int,
    modes: list[str],
    extra_flags: list[str],
    env: dict[str, str],
) -> dict[str, object]:
    cgu = 16 if n_mods <= 100 else (32 if n_mods <= 500 else 64)
    print(f"\n================ EXPERIMENT B: {rounds}-ROUND DISK GROWTH ({n_mods} mods, {opt}, CGU={cgu}, jobs={jobs}) ================")
    summary: dict[str, object] = {}
    for mode in modes:
        with tempfile.TemporaryDirectory(prefix=f"zil_disk_{mode}_") as td_s:
            td = Path(td_s)
            zdir, rdir = generate_projects(td, n_mods)
            z_out_dir = td / "zil_target"
            r_out_dir = td / "rust_target"
            z_cache = z_out_dir / "objcache"
            r_inc = r_out_dir / "incremental"
            z_out_dir.mkdir()
            r_out_dir.mkdir()
            tmp_json = td / "out.json"

            run_zil_build(
                zil_bin, zdir / "main.zil", z_out_dir / "app", z_cache,
                opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_json, env=env, extra_flags=extra_flags,
            )
            run_rust_build(
                rustc_bin, rdir / "main.rs", r_out_dir / "app", r_inc,
                opt=opt, cgu=cgu, tmp_json=tmp_json, env=env,
            )
            zc0, zcf0 = dir_stats(z_cache)
            rc0, rcf0 = dir_stats(r_inc)

            z_ms_list, r_ms_list, z_hit_pcts = [], [], []
            k1 = max(1, n_mods // 100)
            k10 = max(1, n_mods // 10)
            k30 = max(1, (n_mods * 3) // 10)
            stride = max(1, n_mods // 10)
            for rd in range(1, rounds + 1):
                if mode == "edit_1pct_clustered":
                    start = (rd % 10) * stride
                    mods = list(range(start, min(start + k1, n_mods)))
                elif mode == "edit_10pct_clustered":
                    start = (rd % 10) * stride
                    mods = list(range(start, min(start + k10, n_mods)))
                elif mode == "edit_30pct_clustered":
                    start = (rd % 7) * stride
                    mods = list(range(start, min(start + k30, n_mods)))
                else:
                    mods = []
                for m in mods:
                    (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=rd))
                    (rdir / f"m{m}.rs").write_text(rust_module_src(m, salt=rd))
                zr = run_zil_build(
                    zil_bin, zdir / "main.zil", z_out_dir / "app", z_cache,
                    opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_json, env=env, extra_flags=extra_flags,
                )
                rr = run_rust_build(
                    rustc_bin, rdir / "main.rs", r_out_dir / "app", r_inc,
                    opt=opt, cgu=cgu, tmp_json=tmp_json, env=env,
                )
                z_ms_list.append(float(zr["ms"]))
                r_ms_list.append(float(rr["ms"]))
                z_hit_pcts.append(100.0 - float(zr["miss_pct"]))

            zc1, zcf1 = dir_stats(z_cache)
            rc1, rcf1 = dir_stats(r_inc)
            zt1, _ = dir_stats(z_out_dir)
            rt1, _ = dir_stats(r_out_dir)
            z_growth_kb = (zc1 - zc0) / 1024.0 / max(1, rounds)
            r_growth_kb = (rc1 - rc0) / 1024.0 / max(1, rounds)
            print(
                f"  {mode:<22} | "
                f"Cache Z: {fmt_mb(zc0)}({zcf0:3d}f)->{fmt_mb(zc1)}({zcf1:4d}f) [+{z_growth_kb:5.1f}KB/rd] | "
                f"Inc R: {fmt_mb(rc0)}({rcf0:3d}f)->{fmt_mb(rc1)}({rcf1:4d}f) [+{r_growth_kb:5.1f}KB/rd] | "
                f"Tot Z/R: {fmt_mb(zt1)}/{fmt_mb(rt1)} | "
                f"Z hit={statistics.mean(z_hit_pcts):5.1f}% | "
                f"med ms Z/R={statistics.median(z_ms_list):5.1f}/{statistics.median(r_ms_list):5.1f}"
            )
            summary[mode] = {
                "zil_cache_start_bytes": zc0,
                "zil_cache_end_bytes": zc1,
                "zil_total_end_bytes": zt1,
                "rust_inc_start_bytes": rc0,
                "rust_inc_end_bytes": rc1,
                "rust_total_end_bytes": rt1,
                "zil_mean_hit_pct": round(statistics.mean(z_hit_pcts), 2),
                "zil_median_ms": round(statistics.median(z_ms_list), 2),
                "rust_median_ms": round(statistics.median(r_ms_list), 2),
                "zil_raw_ms": z_ms_list,
                "rust_raw_ms": r_ms_list,
            }
    return summary


def run_eviction_suite(
    zil_bin: Path,
    n_mods: int,
    rounds: int,
    opt: str,
    jobs: int,
    budgets_mb: list[int],
    env: dict[str, str],
) -> dict[str, object]:
    cgu = 16 if n_mods <= 100 else (32 if n_mods <= 500 else 64)
    budget_desc = ",".join(f"{b}MB" if b > 0 else "uncapped" for b in budgets_mb)
    print(
        f"\n================ EXPERIMENT C: BOUNDED CACHE EVICTION ({n_mods} mods, {rounds} rds, 30% edits, {opt}, CGU={cgu}, budgets=[{budget_desc}]) ================"
    )
    out: dict[str, object] = {}
    with tempfile.TemporaryDirectory(prefix=f"zil_evict_{n_mods}_") as td_s:
        td = Path(td_s)
        zdir, _ = generate_projects(td, n_mods)
        tmp_json = td / "out.json"
        for idx_b, cap_mb in enumerate(budgets_mb):
            label = "uncapped (0MB)" if cap_mb == 0 else f"soft-LRU ({cap_mb}MB)"
            extra_flags = ["--object-cache-max-mb", str(cap_mb)]
            z_out_dir = td / f"out_b{idx_b}_{cap_mb}"
            z_cache = z_out_dir / "objcache"
            z_out_dir.mkdir(parents=True, exist_ok=True)
            for m in range(n_mods):
                (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=0))
            run_zil_build(
                zil_bin, zdir / "main.zil", z_out_dir / "app", z_cache,
                opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_json, env=env, extra_flags=extra_flags,
            )
            peak_cache_b = 0
            evict_rounds = 0
            evicted_entries_total = 0
            evicted_bytes_total = 0
            evict_ns_total = 0
            ms_all, ms_first, ms_last, hit_last = [], [], [], []
            k30 = max(1, (n_mods * 3) // 10)
            window = min(20, max(1, rounds // 3))
            for rd in range(1, rounds + 1):
                start = (rd % 7) * max(1, n_mods // 10)
                mods = list(range(start, min(start + k30, n_mods)))
                for m in mods:
                    (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=rd))
                zr = run_zil_build(
                    zil_bin, zdir / "main.zil", z_out_dir / "app", z_cache,
                    opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_json, env=env, extra_flags=extra_flags,
                )
                cb, _ = dir_stats(z_cache)
                if cb > peak_cache_b:
                    peak_cache_b = cb
                ev_n = int(zr["cache_evicted_entries"])
                if ev_n > 0:
                    evict_rounds += 1
                    evicted_entries_total += ev_n
                    evicted_bytes_total += int(zr["cache_evicted_bytes"])
                evict_ns_total += int(zr["cache_evict_ns"])
                ms_val = float(zr["ms"])
                ms_all.append(ms_val)
                if rd <= window:
                    ms_first.append(ms_val)
                if rd > rounds - window:
                    ms_last.append(ms_val)
                    hit_last.append(100.0 - float(zr["miss_pct"]))
            warm = run_zil_build(
                zil_bin, zdir / "main.zil", z_out_dir / "app", z_cache,
                opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_json, env=env, extra_flags=extra_flags,
            )
            end_cb, end_cf = dir_stats(z_cache)
            end_tot, _ = dir_stats(z_out_dir)
            p95 = sorted(ms_all)[int(len(ms_all) * 0.95)]
            evict_ms_total = evict_ns_total / 1e6
            print(
                f"  {label:<18} | end={fmt_mb(end_cb)}({end_cf:5d}f) peak={fmt_mb(peak_cache_b)} "
                f"tot={fmt_mb(end_tot)} | evict={evicted_entries_total:4d} objs/{evict_rounds:2d} rds "
                f"({evicted_bytes_total/(1024*1024):5.1f}MB, {evict_ms_total:5.1f}ms) | "
                f"last={statistics.median(ms_last):5.1f}ms p95={p95:5.1f}ms | "
                f"post_warm={warm['ms']:5.1f}ms (hit={100.0 - float(warm['miss_pct']):5.1f}%)"
            )
            out[label] = {
                "budget_mb": cap_mb,
                "end_cache_bytes": end_cb,
                "end_cache_files": end_cf,
                "peak_cache_bytes": peak_cache_b,
                "end_total_bytes": end_tot,
                "evict_rounds": evict_rounds,
                "evicted_entries_total": evicted_entries_total,
                "evicted_bytes_total": evicted_bytes_total,
                "evict_ms_total": round(evict_ms_total, 2),
                "first_median_ms": round(statistics.median(ms_first), 2),
                "last_median_ms": round(statistics.median(ms_last), 2),
                "p95_ms": round(p95, 2),
                "last_hit_pct": round(statistics.mean(hit_last), 2),
                "post_warm_ms": warm["ms"],
                "post_warm_hit_pct": round(100.0 - float(warm["miss_pct"]), 2),
            }
    return out


def run_multiprocess_suite(
    zil_bin: Path,
    workers: int,
    rounds: int,
    n_mods: int,
    opt: str,
    max_mb: int,
    env: dict[str, str],
) -> dict[str, object]:
    cgu = 16
    print(
        f"\n================ EXPERIMENT D: MULTI-PROCESS SHARED CACHE ({workers} procs x {rounds} rds, {n_mods} mods, {opt}, CGU={cgu}, cap={max_mb}MB) ================"
    )
    with tempfile.TemporaryDirectory(prefix="zil_mp_bench_") as td_s:
        td = Path(td_s)
        shared_cache = td / "shared_objcache"
        shared_cache.mkdir()

        def worker_task(wid: int) -> dict[str, object]:
            wdir = td / f"worker_{wid}"
            wdir.mkdir()
            out_exe = wdir / "app"
            tmp_json = wdir / "out.json"
            # Shared deterministic base modules across all workers + worker-private module
            for m in range(n_mods):
                (wdir / f"m{m}.zil").write_text(zil_module_src(m, salt=0))
            times: list[float] = []
            hits_total = 0
            units_total = 0
            for rd in range(rounds):
                salt = wid * 1000 + rd
                (wdir / f"m{wid % n_mods}.zil").write_text(zil_module_src(wid % n_mods, salt=salt))
                group_tails = [min(g + 9, n_mods - 1) for g in range(0, n_mods, 10)]
                z_imports = "\n".join(f"import m{t};" for t in group_tails)
                z_calls = "\n".join(f"  sum = sum + m{t}.compute(sum % 997);" for t in group_tails)
                (wdir / "main.zil").write_text(
                    f"{z_imports}\npub fn main() -> i32 {{\n  var sum: i64 = 1;\n{z_calls}\n  return i32(sum % 127);\n}}\n"
                )
                zr = run_zil_build(
                    zil_bin, wdir / "main.zil", out_exe, shared_cache,
                    opt=opt, cgu=cgu, jobs=2, tmp_json=tmp_json, env=env,
                    extra_flags=["--object-cache-max-mb", str(max_mb)],
                )
                # Verify produced binary actually executes cleanly.
                run_rc = subprocess.run([str(out_exe)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode
                if run_rc < 0:
                    raise RuntimeError(f"worker {wid} rd {rd} binary crashed with signal {-run_rc}")
                times.append(float(zr["ms"]))
                hits_total += int(zr["hits"])
                units_total += int(zr["units"])
            return {
                "worker": wid,
                "builds": rounds,
                "median_ms": round(statistics.median(times), 2),
                "hit_pct": round((hits_total / units_total * 100.0) if units_total else 0.0, 2),
            }

        t0 = time.perf_counter()
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
            futs = [pool.submit(worker_task, wid) for wid in range(workers)]
            worker_results = [f.result() for f in futs]
        wall_ms = (time.perf_counter() - t0) * 1000.0
        end_cb, end_cf = dir_stats(shared_cache)
        leaked_stages = [p.name for p in shared_cache.iterdir() if p.name.startswith(".stage-")]
        total_builds = workers * rounds
        print(
            f"  {total_builds}/{total_builds} concurrent OS builds passed in {wall_ms:.1f}ms wall "
            f"| endCache={fmt_mb(end_cb)}({end_cf}f) | leaked_stages={len(leaked_stages)}"
        )
        for wr in worker_results:
            print(f"    worker {wr['worker']}: {wr['builds']} builds, med={wr['median_ms']}ms, hit={wr['hit_pct']}%")
        return {
            "workers": workers,
            "rounds_per_worker": rounds,
            "total_builds": total_builds,
            "wall_ms": round(wall_ms, 2),
            "end_cache_bytes": end_cb,
            "end_cache_files": end_cf,
            "leaked_stages": leaked_stages,
            "worker_results": worker_results,
        }


def run_generics_suite(
    zil_bin: Path,
    rustc_bin: Path,
    n_mods: int,
    opt: str,
    jobs: int,
    samples: int,
    env: dict[str, str],
) -> dict[str, object]:
    """Experiment E: Generics + ADT + Trait monomorphization across N modules (cold, warm, 1-edit)."""
    cgu = 16 if n_mods <= 100 else 32
    r_opt = "0" if opt == "O0" else "2"
    print(f"\n================ EXPERIMENT E: GENERICS + ADT + TRAIT MONOMORPHIZATION ({n_mods} mods, {opt}, CGU={cgu}, jobs={jobs}, samples={samples}) ================")
    with tempfile.TemporaryDirectory(prefix="zil_gen_bench_") as td:
        root = Path(td)
        gz = root / "gen_zil"
        gr = root / "gen_rs"
        gz.mkdir()
        gr.mkdir()
        (gz / "core_gen.zil").write_text(
            "pub trait Reducer {\n"
            "    fn reduce(self: &Self, acc: i64) -> i64;\n"
            "}\n"
            "pub fn apply_pair<A: Value, B: Value>(a: A, b: B) -> (B, A) {\n"
            "    return (b, a)\n"
            "}\n"
            "pub fn identity<T: Value>(v: T) -> T {\n"
            "    return v\n"
            "}\n"
        )
        (gr / "core_gen.rs").write_text(
            "pub trait Reducer {\n"
            "    fn reduce(&self, acc: i64) -> i64;\n"
            "}\n"
            "#[inline(never)]\n"
            "pub fn apply_pair<A, B>(a: A, b: B) -> (B, A) { (b, a) }\n"
            "#[inline(never)]\n"
            "pub fn identity<T>(v: T) -> T { v }\n"
        )

        def write_mod(i: int, salt: int = 0) -> None:
            (gz / f"g{i}.zil").write_text(
                f"import core_gen;\n"
                f"pub struct Rec{i} {{ a: i64, b: i64, c: i64 }}\n"
                f"impl core_gen.Reducer for Rec{i} {{\n"
                f"    fn reduce(self: &Self, acc: i64) -> i64 {{\n"
                f"        return acc + self.a * 3 + self.b + self.c + {i + 1 + salt}\n"
                f"    }}\n"
                f"}}\n"
                f"pub fn run(x: i64) -> i64 {{\n"
                f"    let r = core_gen.identity::<Rec{i}>(Rec{i} {{ a: x + {i + salt}, b: x * 2, c: {(i % 19) + 1} }})\n"
                f"    let p = core_gen.apply_pair::<Rec{i}, i64>(r, x + {i + 7})\n"
                f"    let (k, r2) = p\n"
                f"    return r2.reduce(k)\n"
                f"}}\n"
            )
            (gr / f"g{i}.rs").write_text(
                f"use crate::core_gen::{{Reducer, apply_pair, identity}};\n"
                f"pub struct Rec{i} {{ pub a: i64, pub b: i64, pub c: i64 }}\n"
                f"impl Reducer for Rec{i} {{\n"
                f"    #[inline(never)]\n"
                f"    fn reduce(&self, acc: i64) -> i64 {{\n"
                f"        acc.wrapping_add(self.a.wrapping_mul(3)).wrapping_add(self.b).wrapping_add(self.c).wrapping_add({i + 1 + salt})\n"
                f"    }}\n"
                f"}}\n"
                f"#[inline(never)]\n"
                f"pub fn run(x: i64) -> i64 {{\n"
                f"    let r = identity::<Rec{i}>(Rec{i} {{ a: x.wrapping_add({i + salt}), b: x.wrapping_mul(2), c: {(i % 19) + 1} }});\n"
                f"    let (k, r2) = apply_pair::<Rec{i}, i64>(r, x.wrapping_add({i + 7}));\n"
                f"    r2.reduce(k)\n"
                f"}}\n"
            )

        z_imps, z_calls = ["import core_gen;"], []
        r_mods, r_calls = ["mod core_gen;"], []
        for i in range(n_mods):
            write_mod(i, salt=0)
            z_imps.append(f"import g{i};")
            z_calls.append(f"    acc = g{i}.run(acc % 997);")
            r_mods.append(f"mod g{i};")
            r_calls.append(f"    acc = g{i}::run(acc % 997);")
        (gz / "main.zil").write_text(
            "\n".join(z_imps) + "\n\nfn main() -> i64 {\n    var acc: i64 = 1;\n" + "\n".join(z_calls) + "\n    return acc % 100;\n}\n"
        )
        (gr / "main.rs").write_text(
            "\n".join(r_mods) + "\n\nfn main() {\n    let mut acc: i64 = 1;\n" + "\n".join(r_calls) + "\n    std::process::exit((acc % 100) as i32);\n}\n"
        )

        z_cache = root / "z_cache"
        z_out = root / "z_app"
        r_inc = root / "r_inc"
        r_out = root / "r_app"
        tmp_out = root / "cmd.out"

        def run_zil() -> dict[str, object]:
            return run_zil_build(
                zil_bin, gz / "main.zil", z_out, z_cache,
                opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_out, env=env,
            )

        def run_rust() -> dict[str, object]:
            return run_rust_build(
                rustc_bin, gr / "main.rs", r_out, r_inc,
                opt=opt, cgu=cgu, tmp_json=tmp_out, env=env,
            )

        # Cold build
        cold_z_ms, cold_r_ms = [], []
        last_z, last_r = {}, {}
        for _ in range(samples):
            shutil.rmtree(z_cache, ignore_errors=True)
            shutil.rmtree(r_inc, ignore_errors=True)
            last_z = run_zil()
            last_r = run_rust()
            cold_z_ms.append(float(last_z["ms"]))
            cold_r_ms.append(float(last_r["ms"]))
        z_rc = subprocess.run([str(z_out)]).returncode
        r_rc = subprocess.run([str(r_out)]).returncode
        if z_rc != r_rc:
            raise RuntimeError(f"generics exit code mismatch: zil={z_rc} vs rust={r_rc}")

        # Warm no-op
        warm_z_ms, warm_r_ms = [], []
        for _ in range(samples):
            last_z = run_zil()
            last_r = run_rust()
            warm_z_ms.append(float(last_z["ms"]))
            warm_r_ms.append(float(last_r["ms"]))

        # Single-module private edit
        edit_z_ms, edit_r_ms = [], []
        for rep in range(samples):
            write_mod(0, salt=rep + 1)
            last_z = run_zil()
            last_r = run_rust()
            edit_z_ms.append(float(last_z["ms"]))
            edit_r_ms.append(float(last_r["ms"]))

        rows = []
        for name, zs, rs in [
            ("1. cold_build", cold_z_ms, cold_r_ms),
            ("2. warm_noop", warm_z_ms, warm_r_ms),
            ("3. priv_edit_1_mod", edit_z_ms, edit_r_ms),
        ]:
            zm = statistics.median(zs)
            rm = statistics.median(rs)
            ratio = zm / rm if rm > 0 else 0.0
            print(f"  {name:<22} | ZIL={zm:6.1f}ms  Rust={rm:6.1f}ms  ({ratio:.2f}x)")
            rows.append({
                "scenario": name,
                "zil_ms_median": round(zm, 2),
                "rust_ms_median": round(rm, 2),
                "ratio_zil_over_rust": round(ratio, 3),
                "zil_samples_ms": [round(x, 2) for x in zs],
                "rust_samples_ms": [round(x, 2) for x in rs],
            })
        return {
            "modules": n_mods,
            "opt": opt,
            "cgu": cgu,
            "jobs": jobs,
            "samples": samples,
            "verified_exit_code": z_rc,
            "rows": rows,
        }


def run_frontend_cache_suite(
    zil_bin: Path,
    rustc_bin: Path,
    n_mods: int,
    opt: str,
    jobs: int,
    samples: int,
    env: dict[str, str],
) -> dict[str, object]:
    """Experiment F: large-project frontend incremental cache comparison
    (plain CLI --object-cache vs CLI --semantic-cache vs --resident vs rustc).
    """
    cgu = 16
    rs_opt = "0" if opt == "O0" else "2"
    print(f"\n=== Experiment F: {n_mods} modules ({opt}, jobs={jobs}, samples={samples}) — Semantic Cache & Resident ===")
    samples = max(1, samples)
    res: dict[str, dict[str, list[float]]] = {
        "rustc": {"cold": [], "warm": [], "priv_edit_1": []},
        "zil_plain_cli": {"cold": [], "warm": [], "priv_edit_1": []},
        "zil_semantic_cache": {"cold": [], "warm": [], "priv_edit_1": []},
        "zil_resident": {"cold": [], "warm": [], "priv_edit_1": []},
    }
    sem_cache_bytes_last = 0
    sem_cache_files_last = 0
    checks: dict[str, object] = {}

    for s in range(samples):
        work = Path(tempfile.mkdtemp(prefix=f"zil_bench_f_{n_mods}_{s}_"))
        try:
            z_src, r_src = generate_projects(work, n_mods)
            entry = z_src / "main.zil"
            z_out = work / "z_app"

            def run_zil(obj_cache: Path, sem_cache: Path | None = None) -> tuple[float, dict]:
                cmd = [
                    str(zil_bin), "build", str(entry),
                    "--opt", opt, "--codegen-units", str(cgu), "--codegen-jobs", str(jobs),
                    "--object-cache", str(obj_cache), "--json", "-o", str(z_out),
                ]
                if sem_cache is not None:
                    cmd += ["--semantic-cache", str(sem_cache)]
                t0 = time.perf_counter()
                p = subprocess.run(cmd, env=env, capture_output=True, text=True)
                dt = (time.perf_counter() - t0) * 1000.0
                if p.returncode != 0:
                    raise RuntimeError(f"zil failed: {p.stderr}\n{p.stdout}")
                return dt, json.loads(p.stdout)

            # 1. Plain CLI (--object-cache)
            oc1 = work / "oc1"
            c1, _ = run_zil(oc1)
            w1, jw1 = run_zil(oc1)
            (z_src / "m500.zil").write_text(zil_module_src(500, salt=10 + s))
            e1, _ = run_zil(oc1)
            (z_src / "m500.zil").write_text(zil_module_src(500, salt=0))
            res["zil_plain_cli"]["cold"].append(c1)
            res["zil_plain_cli"]["warm"].append(w1)
            res["zil_plain_cli"]["priv_edit_1"].append(e1)

            # 2. CLI + --semantic-cache + --object-cache
            oc2 = work / "oc2"
            sc2 = work / "sc2"
            c2, _ = run_zil(oc2, sc2)
            w2, jw2 = run_zil(oc2, sc2)
            (z_src / "m500.zil").write_text(zil_module_src(500, salt=20 + s))
            e2, _ = run_zil(oc2, sc2)
            (z_src / "m500.zil").write_text(zil_module_src(500, salt=0))
            sem_cache_bytes_last, sem_cache_files_last = dir_stats(sc2)
            res["zil_semantic_cache"]["cold"].append(c2)
            res["zil_semantic_cache"]["warm"].append(w2)
            res["zil_semantic_cache"]["priv_edit_1"].append(e2)

            # 3. Resident session (--resident --resident-capacity 4096 --object-cache)
            oc3 = work / "oc3"
            proc = subprocess.Popen(
                [
                    str(zil_bin), "build", str(entry),
                    "--opt", opt, "--codegen-units", str(cgu), "--codegen-jobs", str(jobs),
                    "--object-cache", str(oc3), "-o", str(z_out),
                    "--resident", "--resident-capacity", "4096", "--json",
                ],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                text=True, env=env, bufsize=1,
            )
            assert proc.stdin is not None and proc.stdout is not None
            ready = json.loads(proc.stdout.readline())
            if not ready.get("ready"):
                raise RuntimeError(f"resident not ready: {ready}")

            def res_compile() -> tuple[float, dict]:
                t0 = time.perf_counter()
                proc.stdin.write("compile\n")
                proc.stdin.flush()
                line = proc.stdout.readline()
                dt = (time.perf_counter() - t0) * 1000.0
                j = json.loads(line)
                rep = j.get("report", j)
                if rep.get("status") != "Success":
                    raise RuntimeError(f"resident compile failed: {rep.get('status')} {rep.get('diagnostics')}")
                return dt, rep

            rc, _ = res_compile()
            rw, jrw = res_compile()
            (z_src / "m500.zil").write_text(zil_module_src(500, salt=30 + s))
            re1, _ = res_compile()
            proc.stdin.write("quit\n")
            proc.stdin.flush()
            proc.wait(timeout=10)
            res["zil_resident"]["cold"].append(rc)
            res["zil_resident"]["warm"].append(rw)
            res["zil_resident"]["priv_edit_1"].append(re1)

            checks = {
                "plain_warm_link_skipped": bool(jw1.get("native_codegen", {}).get("link_skipped")),
                "sem_warm_cache_hit": bool(jw2.get("cache_hit")),
                "sem_warm_verification_runs": int(jw2.get("verification_runs", 0)),
                "sem_warm_link_skipped": bool(jw2.get("native_codegen", {}).get("link_skipped")),
                "res_warm_cache_hit": bool(jrw.get("cache_hit")),
                "res_warm_verification_runs": int(jrw.get("verification_runs", 0)),
                "res_warm_link_skipped": bool(jrw.get("native_codegen", {}).get("link_skipped")),
            }

            # 4. Rust baseline
            rs_out = work / "rs_app"
            rs_inc = work / "rs_inc"

            def run_rs() -> float:
                cmd = [
                    str(rustc_bin), str(r_src / "main.rs"), "-o", str(rs_out),
                    "-C", f"opt-level={rs_opt}", "-C", "debuginfo=0",
                    "-C", "overflow-checks=yes", "-C", "panic=abort",
                    "-C", f"codegen-units={cgu}", "-C", f"incremental={rs_inc}",
                ]
                t0 = time.perf_counter()
                p = subprocess.run(cmd, env=env, capture_output=True, text=True)
                dt = (time.perf_counter() - t0) * 1000.0
                if p.returncode != 0:
                    raise RuntimeError(f"rustc failed: {p.stderr}")
                return dt

            res["rustc"]["cold"].append(run_rs())
            res["rustc"]["warm"].append(run_rs())
            (r_src / "m500.rs").write_text(rust_module_src(500, salt=10 + s))
            res["rustc"]["priv_edit_1"].append(run_rs())
        finally:
            shutil.rmtree(work, ignore_errors=True)

    summary: dict[str, dict[str, float]] = {}
    for mode, buckets in res.items():
        summary[mode] = {k: round(statistics.median(v), 2) for k, v in buckets.items()}

    rs_m = summary["rustc"]
    for mode in ("rustc", "zil_plain_cli", "zil_semantic_cache", "zil_resident"):
        m = summary[mode]
        if mode == "rustc":
            print(f"  {mode:20s}: cold={m['cold']:6.1f}ms  warm={m['warm']:6.1f}ms  priv_edit_1={m['priv_edit_1']:6.1f}ms")
        else:
            print(
                f"  {mode:20s}: cold={m['cold']:6.1f}ms ({m['cold']/rs_m['cold']:.2f}x)  "
                f"warm={m['warm']:6.1f}ms ({m['warm']/rs_m['warm']:.2f}x)  "
                f"priv_edit_1={m['priv_edit_1']:6.1f}ms ({m['priv_edit_1']/rs_m['priv_edit_1']:.2f}x)"
            )
    print(f"  semantic_cache disk: {fmt_mb(sem_cache_bytes_last)} ({sem_cache_files_last} files) | checks={checks}")
    return {
        "n_mods": n_mods,
        "opt": opt,
        "samples": samples,
        "median_ms": summary,
        "raw_ms": res,
        "semantic_cache_bytes": sem_cache_bytes_last,
        "semantic_cache_files": sem_cache_files_last,
        "checks": checks,
    }


def collect_metadata(zil_bin: Path, rustc_bin: Path, env: dict[str, str]) -> dict[str, object]:
    def ver(cmd: list[str]) -> str:
        try:
            return subprocess.check_output(cmd, env=env, stderr=subprocess.STDOUT, text=True).strip()
        except Exception as exc:
            return f"unknown ({exc})"

    return {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "cpu_count": os.cpu_count(),
        "python": platform.python_version(),
        "zil_version": ver([str(zil_bin), "version"]),
        "rustc_version": ver([str(rustc_bin), "--version"]),
    }


def main() -> None:
    default_jobs = max(1, os.cpu_count() or 2)
    ap = argparse.ArgumentParser(description="ZIL vs rustc large-project edit, disk, LRU & multi-process benchmark")
    ap.add_argument("--zil", required=True, help="Path to release zil binary")
    ap.add_argument("--rustc", default=shutil.which("rustc") or "rustc", help="Path to rustc binary")
    ap.add_argument("--llvm-dir", default="/home/antigravity/.cache/zil-llvm/23.1.4", help="LLVM install prefix")
    ap.add_argument("--scales", default="100,500,1000", help="Comma-separated module counts for Experiment A")
    ap.add_argument("--opt", default="O0", choices=["O0", "O2"])
    ap.add_argument("--jobs", type=int, default=default_jobs, help=f"ZIL codegen jobs (default: CPU count = {default_jobs})")
    ap.add_argument("--samples", type=int, default=1, help="Paired repetitions per Experiment A scenario (median reported)")
    ap.add_argument("--rounds", type=int, default=100)
    ap.add_argument("--disk-mods", type=int, default=100, help="Module count for Experiment B multi-round disk suite")
    ap.add_argument(
        "--disk-modes",
        default="noop,edit_1pct_clustered,edit_10pct_clustered,edit_30pct_clustered",
        help="Comma-separated modes for Experiment B",
    )
    ap.add_argument("--skip-scale", action="store_true")
    ap.add_argument("--skip-disk", action="store_true")
    ap.add_argument("--with-eviction", action="store_true")
    ap.add_argument("--evict-mods", type=int, default=100)
    ap.add_argument("--evict-max-mb", type=int, default=1)
    ap.add_argument(
        "--evict-budgets",
        help="Optional comma-separated MB budgets for Experiment C (e.g. '0,16,32,64,128'); overrides --evict-max-mb",
    )
    ap.add_argument("--with-multiprocess", action="store_true")
    ap.add_argument("--mp-workers", type=int, default=4)
    ap.add_argument("--mp-rounds", type=int, default=8)
    ap.add_argument("--mp-mods", type=int, default=40)
    ap.add_argument("--mp-max-mb", type=int, default=1)
    ap.add_argument("--with-generics", action="store_true", help="Run Experiment E generics/ADT/trait monomorphization suite")
    ap.add_argument("--gen-mods", type=int, default=80, help="Module count for Experiment E")
    ap.add_argument("--with-frontend-cache", action="store_true", help="Run Experiment F semantic-cache & resident session suite")
    ap.add_argument("--fc-mods", type=int, default=1000, help="Module count for Experiment F")
    ap.add_argument("--extra", action="append", default=[])
    ap.add_argument("--json-out", help="Optional path to save structured JSON report")
    args = ap.parse_args()

    zil_bin = Path(args.zil).resolve()
    rustc_bin = Path(args.rustc)
    llvm_dir = Path(args.llvm_dir) if args.llvm_dir else None
    env = build_env(llvm_dir)
    meta = collect_metadata(zil_bin, rustc_bin, env)
    meta["zil_codegen_jobs"] = args.jobs
    meta["rustc_default_threads"] = os.cpu_count()
    print(
        f"Toolchain: {meta['zil_version']} | {meta['rustc_version']} | "
        f"CPUs={meta['cpu_count']} (zil_jobs={args.jobs}, rustc_default={meta['rustc_default_threads']}) | {meta['platform']}"
    )

    report: dict[str, object] = {"metadata": meta, "args": vars(args)}
    if not args.skip_scale:
        scales = [int(x) for x in args.scales.split(",") if x.strip()]
        report["experiment_a_scale"] = run_scale_suite(
            zil_bin, rustc_bin, scales, args.opt, args.jobs, args.samples, args.extra, env
        )
    if not args.skip_disk:
        modes = [m.strip() for m in args.disk_modes.split(",") if m.strip()]
        report["experiment_b_disk"] = run_multi_round_disk_suite(
            zil_bin, rustc_bin, args.disk_mods, args.rounds, args.opt, args.jobs, modes, args.extra, env
        )
    if args.with_eviction:
        if args.evict_budgets:
            budgets = [int(x) for x in args.evict_budgets.split(",") if x.strip()]
        else:
            budgets = [0, args.evict_max_mb]
        report["experiment_c_eviction"] = run_eviction_suite(
            zil_bin, args.evict_mods, args.rounds, args.opt, args.jobs, budgets, env
        )
    if args.with_multiprocess:
        report["experiment_d_multiprocess"] = run_multiprocess_suite(
            zil_bin, args.mp_workers, args.mp_rounds, args.mp_mods, args.opt, args.mp_max_mb, env
        )
    if args.with_generics:
        report["experiment_e_generics"] = run_generics_suite(
            zil_bin, rustc_bin, args.gen_mods, args.opt, args.jobs, args.samples, env
        )
    if args.with_frontend_cache:
        report["experiment_f_frontend_cache"] = run_frontend_cache_suite(
            zil_bin, rustc_bin, args.fc_mods, args.opt, args.jobs, args.samples, env
        )
    if args.json_out:
        out_path = Path(args.json_out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(report, indent=2))
        print(f"Saved JSON report to {out_path}")


if __name__ == "__main__":
    main()
