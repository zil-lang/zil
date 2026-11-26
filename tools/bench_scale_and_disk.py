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
    if rep and rep.get("native_codegen"):
        nc = rep["native_codegen"]
        total_units = nc.get("actual_units", 0)
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
    extra_flags: list[str],
    env: dict[str, str],
) -> dict[str, object]:
    print(f"\n================ EXPERIMENT A: MULTI-POINT CONCURRENT EDITS ({opt}, jobs={jobs}) ================")
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

            print(f"\n--- {n_mods} modules (CGU={cgu}, opt={opt}, debuginfo=0) ---")
            print(
                f"{'Scenario':<28} | {'ZIL ms':>8} {'Rust ms':>8} {'Z/R':>6} | "
                f"{'Recomp':>7} {'Miss%':>6} | {'RSS Z/R MB':>12} | {'Cache Z/R':>17} | {'Total Z/R':>17}"
            )
            print("-" * 130)

            def measure(label: str) -> None:
                zr = run_zil_build(
                    zil_bin, zdir / "main.zil", z_out_dir / "app", z_cache,
                    opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_json, env=env, extra_flags=extra_flags,
                )
                rr = run_rust_build(
                    rustc_bin, rdir / "main.rs", r_out_dir / "app", r_inc,
                    opt=opt, cgu=cgu, tmp_json=tmp_json, env=env,
                )
                zc_b, zc_f = dir_stats(z_cache)
                rc_b, rc_f = dir_stats(r_inc)
                zt_b, _ = dir_stats(z_out_dir)
                rt_b, _ = dir_stats(r_out_dir)
                ratio = float(zr["ms"]) / float(rr["ms"])
                print(
                    f"{label:<28} | {zr['ms']:8.1f} {rr['ms']:8.1f} {ratio:5.2f}x | "
                    f"{zr['misses']:2d}/{zr['units']:<4d} {zr['miss_pct']:5.1f}% | "
                    f"{zr['rss_mb']:5.1f}/{rr['rss_mb']:<6.1f} | "
                    f"{fmt_mb(zc_b)}/{fmt_mb(rc_b)} | {fmt_mb(zt_b)}/{fmt_mb(rt_b)}"
                )
                rows.append({
                    "scenario": label,
                    "zil": zr,
                    "rust": rr,
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

            measure("1. cold_build")
            measure("2. warm_noop")
            (zdir / "m0.zil").write_text(zil_module_src(0, salt=1))
            (rdir / "m0.rs").write_text(rust_module_src(0, salt=1))
            measure("3a. priv_edit_1_body")
            (zdir / "m0.zil").write_text(zil_module_src(0, salt=2, extra_helper=True))
            (rdir / "m0.rs").write_text(rust_module_src(0, salt=2, extra_helper=True))
            measure("3b. priv_edit_1_add_fn")
            # 3c. Insert brand-new module imported by main
            (zdir / "m_extra.zil").write_text("pub fn compute(x: i64) -> i64 {\n  return x + 19;\n}\n")
            (rdir / "m_extra.rs").write_text("pub fn compute(x: i64) -> i64 {\n  x + 19\n}\n")
            write_mains(zdir, rdir, n_mods, extra_mod="m_extra")
            measure("3c. add_new_module")
            # 3d. Remove the newly added module
            write_mains(zdir, rdir, n_mods)
            measure("3d. remove_module")
            (zdir / "m0.zil").write_text(zil_module_src(0, salt=3))
            (rdir / "m0.rs").write_text(rust_module_src(0, salt=3))
            k10 = max(1, n_mods // 10)
            for m in range(k10):
                (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=11))
                (rdir / f"m{m}.rs").write_text(rust_module_src(m, salt=11))
            measure("4a. priv_edit_10%_clustered")
            rng = random.Random(42)
            for m in rng.sample(range(n_mods), k10):
                (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=12))
                (rdir / f"m{m}.rs").write_text(rust_module_src(m, salt=12))
            measure("4b. priv_edit_10%_scattered")
            k30 = max(1, (n_mods * 3) // 10)
            for m in range(k30):
                (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=21))
                (rdir / f"m{m}.rs").write_text(rust_module_src(m, salt=21))
            measure("5a. priv_edit_30%_clustered")
            for m in rng.sample(range(n_mods), k30):
                (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=22))
                (rdir / f"m{m}.rs").write_text(rust_module_src(m, salt=22))
            measure("5b. priv_edit_30%_scattered")
            (zdir / "m9.zil").write_text(zil_module_src(9, salt=99, extra_param=True))
            (rdir / "m9.rs").write_text(rust_module_src(9, salt=99, extra_param=True))
            write_mains(zdir, rdir, n_mods, iface_tail=9)
            measure("6. public_iface_edit")

            out_data[str(n_mods)] = {
                "cgu": cgu,
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
    extra_flags: list[str],
    env: dict[str, str],
) -> dict[str, object]:
    print(f"\n================ EXPERIMENT B: {rounds}-ROUND DISK GROWTH ({n_mods} mods, {opt}, CGU=16) ================")
    cgu = 16
    summary: dict[str, object] = {}
    for mode in ["noop", "edit_1pct_clustered", "edit_10pct_clustered", "edit_30pct_clustered"]:
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
            for rd in range(1, rounds + 1):
                if mode == "edit_1pct_clustered":
                    start = (rd % 10) * 10
                    mods = [start]
                elif mode == "edit_10pct_clustered":
                    start = (rd % 10) * 10
                    mods = list(range(start, min(start + 10, n_mods)))
                elif mode == "edit_30pct_clustered":
                    start = (rd % 7) * 10
                    mods = list(range(start, min(start + 30, n_mods)))
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
    max_mb: int,
    env: dict[str, str],
) -> dict[str, object]:
    cgu = 16 if n_mods <= 100 else 32
    print(
        f"\n================ EXPERIMENT C: BOUNDED CACHE EVICTION ({n_mods} mods, {rounds} rds, 30% edits, {opt}, CGU={cgu}, cap={max_mb}MB) ================"
    )
    out: dict[str, object] = {}
    with tempfile.TemporaryDirectory(prefix=f"zil_evict_{n_mods}_") as td_s:
        td = Path(td_s)
        zdir, _ = generate_projects(td, n_mods)
        tmp_json = td / "out.json"
        for label, extra_flags in [
            ("uncapped (0MB)", ["--object-cache-max-mb", "0"]),
            (f"bounded ({max_mb}MB LRU)", ["--object-cache-max-mb", str(max_mb)]),
        ]:
            z_out_dir = td / f"out_{label.split()[0]}"
            z_cache = z_out_dir / "objcache"
            z_out_dir.mkdir(parents=True, exist_ok=True)
            for m in range(n_mods):
                (zdir / f"m{m}.zil").write_text(zil_module_src(m, salt=0))
            run_zil_build(
                zil_bin, zdir / "main.zil", z_out_dir / "app", z_cache,
                opt=opt, cgu=cgu, jobs=jobs, tmp_json=tmp_json, env=env, extra_flags=extra_flags,
            )
            peak_cache_b = 0
            evict_events = 0
            prev_files = dir_stats(z_cache)[1]
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
                cb, cf = dir_stats(z_cache)
                if cb > peak_cache_b:
                    peak_cache_b = cb
                if cf < prev_files:
                    evict_events += 1
                prev_files = cf
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
            print(
                f"  {label:<20} | endCache={fmt_mb(end_cb)}({end_cf:4d}f) peak={fmt_mb(peak_cache_b)} "
                f"tot={fmt_mb(end_tot)} | evicts={evict_events:2d} | "
                f"first={statistics.median(ms_first):5.1f}ms last={statistics.median(ms_last):5.1f}ms "
                f"p95={p95:5.1f}ms | last_hit={statistics.mean(hit_last):5.1f}% | "
                f"post_warm={warm['ms']:5.1f}ms (hit={100.0 - float(warm['miss_pct']):5.1f}%)"
            )
            out[label] = {
                "end_cache_bytes": end_cb,
                "end_cache_files": end_cf,
                "peak_cache_bytes": peak_cache_b,
                "end_total_bytes": end_tot,
                "evict_events": evict_events,
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
    ap = argparse.ArgumentParser(description="ZIL vs rustc large-project edit, disk, LRU & multi-process benchmark")
    ap.add_argument("--zil", required=True, help="Path to release zil binary")
    ap.add_argument("--rustc", default=shutil.which("rustc") or "rustc", help="Path to rustc binary")
    ap.add_argument("--llvm-dir", default="/home/antigravity/.cache/zil-llvm/23.1.4", help="LLVM install prefix")
    ap.add_argument("--scales", default="100,500,1000", help="Comma-separated module counts for Experiment A")
    ap.add_argument("--opt", default="O0", choices=["O0", "O2"])
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--rounds", type=int, default=100)
    ap.add_argument("--skip-scale", action="store_true")
    ap.add_argument("--skip-disk", action="store_true")
    ap.add_argument("--with-eviction", action="store_true")
    ap.add_argument("--evict-mods", type=int, default=100)
    ap.add_argument("--evict-max-mb", type=int, default=1)
    ap.add_argument("--with-multiprocess", action="store_true")
    ap.add_argument("--mp-workers", type=int, default=4)
    ap.add_argument("--mp-rounds", type=int, default=8)
    ap.add_argument("--mp-mods", type=int, default=40)
    ap.add_argument("--mp-max-mb", type=int, default=1)
    ap.add_argument("--extra", action="append", default=[])
    ap.add_argument("--json-out", help="Optional path to save structured JSON report")
    args = ap.parse_args()

    zil_bin = Path(args.zil).resolve()
    rustc_bin = Path(args.rustc)
    llvm_dir = Path(args.llvm_dir) if args.llvm_dir else None
    env = build_env(llvm_dir)
    meta = collect_metadata(zil_bin, rustc_bin, env)
    print(f"Toolchain: {meta['zil_version']} | {meta['rustc_version']} | CPUs={meta['cpu_count']} | {meta['platform']}")

    report: dict[str, object] = {"metadata": meta, "args": vars(args)}
    if not args.skip_scale:
        scales = [int(x) for x in args.scales.split(",") if x.strip()]
        report["experiment_a_scale"] = run_scale_suite(zil_bin, rustc_bin, scales, args.opt, args.jobs, args.extra, env)
    if not args.skip_disk:
        report["experiment_b_disk"] = run_multi_round_disk_suite(
            zil_bin, rustc_bin, 100, args.rounds, args.opt, args.jobs, args.extra, env
        )
    if args.with_eviction:
        report["experiment_c_eviction"] = run_eviction_suite(
            zil_bin, args.evict_mods, args.rounds, args.opt, args.jobs, args.evict_max_mb, env
        )
    if args.with_multiprocess:
        report["experiment_d_multiprocess"] = run_multiprocess_suite(
            zil_bin, args.mp_workers, args.mp_rounds, args.mp_mods, args.opt, args.mp_max_mb, env
        )
    if args.json_out:
        out_path = Path(args.json_out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(report, indent=2))
        print(f"Saved JSON report to {out_path}")


if __name__ == "__main__":
    main()
