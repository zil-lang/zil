#!/usr/bin/env python3
"""Real Win64 PE / Wine / isolated X display validation, with complete receipts.

Wine is a compatibility executor, never labelled native Windows. The source
window has no C host; the separate C file only checks the C ABI in a test.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--imports", type=Path, required=True)
    parser.add_argument("--wine", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--clang", default=os.environ.get("ZIL_CLANG", "clang"))
    parser.add_argument("--linker", default="lld-link")
    parser.add_argument("--lib", default="llvm-lib")
    parser.add_argument("--readobj", default="llvm-readobj")
    parser.add_argument("--xdotool", default="xdotool")
    parser.add_argument("--import-image", default="import")
    args = parser.parse_args()
    from PIL import Image
    root = Path(__file__).resolve().parents[3]
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    records = []
    commands = []
    processes = []

    def command(argv, stem, timeout=60, expected=0):
        argv = [str(item) for item in argv]
        result = subprocess.run(argv, capture_output=True, timeout=timeout)
        (out / (stem + ".stdout")).write_bytes(result.stdout)
        (out / (stem + ".stderr")).write_bytes(result.stderr)
        commands.append({"argv": argv, "exit_code": result.returncode, "stem": stem})
        assert result.returncode == expected, (stem, result.returncode, result.stdout[-3000:], result.stderr[-3000:])
        return result.stdout

    def check(name, okay, detail=None):
        records.append({"name": name, "passed": bool(okay), "detail": detail})
        assert okay, (name, detail)

    def compile_source(source, name, level, extra=()):
        directory = out / name
        directory.mkdir()
        binary = directory / "program.exe"
        argv = [args.compiler.resolve(), "build", source, "--target", "win64", "--opt", level,
                "--clang", args.clang, "--linker", args.linker, "-o", binary,
                "--emit-dir", directory / "phases", "--json"]
        libraries = sorted(args.imports.resolve().glob("*.lib")) + list(extra)
        assert len(libraries) >= 5
        for library in libraries:
            argv += ["--windows-lib", library]
        result = command(argv, name + "-compile", timeout=120)
        report = json.loads(result)
        check(name + ": verified complete build", report["status"] == "Success" and report["verification_runs"] == 1, report["status"])
        check(name + ": distinct clang/link commands", len(report["commands"]) == 2, report["commands"])
        check(name + ": PE signature", binary.read_bytes().startswith(b"MZ"))
        check(name + ": Windows LLVM target", 'target triple = "x86_64-pc-windows-msvc"' in binary.with_name("program.exe.ll").read_text())
        return binary

    def run_quiet(binary, name, expected):
        data = command([args.wine.resolve(), binary], name + "-run", timeout=30, expected=expected)
        trace = data.decode("utf-8", errors="strict").replace("\r\n", "\n")
        return trace

    def traces(path):
        return path.read_text(errors="replace").replace("\r\n", "\n").splitlines()

    def wait_for(predicate, label, seconds=10):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            value = predicate()
            if value:
                return value
            time.sleep(0.025)
        raise AssertionError("timeout: " + label)

    def gui(binary, name, close_key):
        stdout = out / (name + "-run.stdout")
        stderr = out / (name + "-run.stderr")
        with stdout.open("wb") as output, stderr.open("wb") as errors:
            argv = [str(args.wine.resolve()), str(binary)]
            process = subprocess.Popen(argv, stdout=output, stderr=errors)
            processes.append(process)
            wait_for(lambda: "P:0" in traces(stdout), "initial native paint")
            ids = command([args.xdotool, "search", "--name", "^ZIL ZUI - state 0$"], name + "-find").decode().split()
            check(name + ": one actual X window", len(ids) == 1, ids)
            window = ids[0]
            images = []
            for state in range(3):
                if state == 1:
                    command([args.xdotool, "windowfocus", "--sync", window, "key", "space"], name + "-keyboard")
                if state == 2:
                    command([args.xdotool, "mousemove", "--window", window, "120", "160", "click", "1"], name + "-mouse")
                wait_for(lambda: f"P:{state}" in traces(stdout), f"native paint {state}")
                image = out / (name + f"-state{state}.png")
                command([args.import_image, "-display", os.environ["DISPLAY"], "-window", window, image], name + f"-capture{state}")
                images.append(image)
            pixels = []
            for path in images:
                with Image.open(path) as image:
                    rgb = image.convert("RGB")
                    colors = Counter(rgb.get_flattened_data() if hasattr(rgb, "get_flattened_data") else rgb.getdata())
                    pixels.append({"blue": colors[(199, 223, 244)], "green": colors[(202, 237, 201)], "size": image.size})
            check(name + ": real state-dependent paint pixels", pixels[0]["blue"] > 5000 and pixels[1]["green"] > 5000 and pixels[2]["blue"] > 5000, pixels)
            count = traces(stdout).count("P:2")
            command([args.xdotool, "windowsize", "--sync", window, "640", "420"], name + "-resize")
            wait_for(lambda: traces(stdout).count("P:2") > count, "native repaint after resize")
            resized = out / (name + "-resized.png")
            command([args.import_image, "-display", os.environ["DISPLAY"], "-window", window, resized], name + "-capture-resized")
            with Image.open(resized) as image:
                check(name + ": actual resize", image.width > pixels[0]["size"][0] and image.height > pixels[0]["size"][1], image.size)
            command([args.xdotool, "windowfocus", "--sync", window, "key", close_key], name + "-close")
            code = process.wait(timeout=15)
            commands.append({"argv": argv, "exit_code": code, "stem": name + "-run"})
            check(name + ": normal process exit", code == 0, code)
        lines = traces(stdout)
        counts = Counter(lines)
        paint_count = sum(line.startswith("P:") for line in lines)
        check(name + ": two input events", counts["E:1"] == 1 and counts["E:2"] == 1, lines)
        check(name + ": exactly one ordered window/class cleanup", counts["D:1"] == counts["W:1"] == counts["U:1"] == 1 and lines[-3:] == ["D:1", "W:1", "U:1"], lines)
        check(name + ": all paint resources released", counts["B:1"] == 2 * paint_count and counts["T:1"] == paint_count, counts)
        check(name + ": no reported API failure", not any(line.startswith("F:") or line in ["B:0", "T:0", "W:0", "U:0"] for line in lines))

    result = {"status": "Fail", "execution": "Win64 PE under Wine on isolated X display; not native Windows",
              "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
              "source_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=root)),
              "compiler_path": str(args.compiler.resolve()), "compiler_sha256": digest(args.compiler),
              "harness_sha256": digest(Path(__file__)),
              "records": records, "commands": commands}
    try:
        result["wine_version"] = command([args.wine, "--version"], "wine-version").decode().strip()
        result["clang_version"] = command([args.clang, "--version"], "clang-version").decode().splitlines()[0]
        for level, closing in [("O0", "Escape"), ("O2", "alt+F4")]:
            for stem, expected in [("fail_create", 42), ("fail_register", 43)]:
                name = stem + "-" + level
                binary = compile_source(root / "examples/mainline/win32" / (stem + ".zil"), name, level)
                trace = run_quiet(binary, name, expected)
                check(name + ": failure cleanup", trace.startswith("R:1\nX:") and trace.endswith("U:1\n") and "W:" not in trace and "C:" not in trace, trace)
            name = "window-" + level
            binary = compile_source(root / "examples/mainline/win32/main.zil", name, level)
            gui(binary, name, closing)
        # The C oracle is deliberately outside the application and checks ABI
        # values, natural record offsets, and callback/static-data retention.
        fixture = (root / "crates/zil-driver/tests/impl087_native.rs").read_text()
        zil = re.search(r'const ABI_SOURCE: &str = r#"(.*?)"#;', fixture, re.S).group(1)
        c = re.search(r'const ABI_C: &str = r#"(.*?)"#;', fixture, re.S).group(1)
        (out / "abi.zil").write_text(zil)
        (out / "oracle.c").write_text(c)
        for level in ["O0", "O2"]:
            obj = out / ("oracle-" + level + ".obj")
            archive = out / ("oracle-" + level + ".lib")
            command([args.clang, "--target=x86_64-pc-windows-msvc", "-" + level, "-Werror", "-c", out / "oracle.c", "-o", obj], "oracle-" + level)
            command([args.lib, "/out:" + str(archive), obj], "oracle-lib-" + level)
            binary = compile_source(out / "abi.zil", "abi-" + level, level, [archive])
            check("abi-" + level + ": native C ABI agreement", run_quiet(binary, "abi-" + level, 0) == "")
        # O0 must resolve and execute a real stack-probe helper. O2 may erase the
        # nonescaping buffer, so it must not be required to retain the probe.
        source = 'fn sink(b:&mut [i64;1024])->i32{b[1023]=65535;return i32(b[1023])}\nfn main()->i32{var b:[i64;1024]=[' + ','.join(['0'] * 1024) + '];if sink(&mut b)!=65535{return 1}return 0}\n'
        (out / "stack.zil").write_text(source)
        for level in ["O0", "O2"]:
            binary = compile_source(out / "stack.zil", "stack-" + level, level)
            pe = command([args.readobj, "--coff-imports", binary], "stack-imports-" + level).decode()
            if level == "O0":
                check("stack-O0: real __chkstk import", "__chkstk" in pe, pe)
            check("stack-" + level + ": stack and buffer machine execution", run_quiet(binary, "stack-" + level, 0) == "")
        check("source and compiler identity stable", result["source_commit"] == subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
              and result["compiler_sha256"] == digest(args.compiler)
              and result["source_dirty"] == bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=root)))
        result["status"] = "Pass"
    except Exception as error:
        result["error"] = repr(error)
        raise
    finally:
        for process in processes:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        result["assertions"] = len(records)
        result["passed"] = sum(record["passed"] for record in records)
        result["pe_executions"] = sum(item["argv"][0] == str(args.wine.resolve()) and item["stem"].endswith("-run") for item in commands)
        result["files"] = [{"path": str(path.relative_to(out)), "bytes": path.stat().st_size, "sha256": digest(path)} for path in sorted(out.rglob("*")) if path.is_file()]
        (out / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ["status", "assertions", "passed", "execution"]}))


if __name__ == "__main__":
    main()
