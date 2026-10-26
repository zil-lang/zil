#!/usr/bin/env python3
"""Build explicit Win64 import archives; never compile a C window host."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

EXPORTS = {
    "kernel32": ["ExitProcess", "GetModuleHandleW", "GetLastError"],
    "kernelbase": ["__chkstk"],
    "user32": ["RegisterClassExW", "UnregisterClassW", "CreateWindowExW", "DefWindowProcW",
               "GetMessageW", "TranslateMessage", "DispatchMessageW", "ShowWindow", "UpdateWindow",
               "DestroyWindow", "PostQuitMessage", "PostMessageW", "GetWindowLongPtrW",
               "SetWindowLongPtrW", "SetWindowTextW", "InvalidateRect", "BeginPaint", "EndPaint",
               "FillRect", "GetCursor"],
    "gdi32": ["CreateSolidBrush", "DeleteObject", "CreateFontW", "SelectObject", "SetBkMode", "SetTextColor", "TextOutW"],
    "msvcrt": ["putchar", "fflush", "printf", "memcpy", "memmove", "memset", "memcmp"],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--dlltool", default=shutil.which("llvm-dlltool"))
    args = parser.parse_args()
    if not args.dlltool:
        parser.error("llvm-dlltool is required")
    args.out.mkdir(parents=True, exist_ok=False)
    records = []
    for dll, symbols in EXPORTS.items():
        definition = args.out / (dll + ".def")
        archive = args.out / (dll + ".lib")
        definition.write_text("LIBRARY " + dll + ".dll\nEXPORTS\n" + "".join("  " + s + "\n" for s in symbols))
        command = [args.dlltool, "-m", "i386:x86-64", "-d", str(definition.resolve()), "-l", str(archive.resolve())]
        result = subprocess.run(command, text=True, capture_output=True, timeout=30)
        record = {"dll": dll + ".dll", "symbols": symbols, "command": command,
                  "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
        records.append(record)
        if result.returncode != 0:
            (args.out / "result.json").write_text(json.dumps({"status": "Fail", "records": records}, indent=2) + "\n")
            raise SystemExit(result.returncode)
        record["archive"] = str(archive.resolve())
        record["archive_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
        record["definition_sha256"] = hashlib.sha256(definition.read_bytes()).hexdigest()
    (args.out / "result.json").write_text(json.dumps({"status": "Pass", "target": "x86_64-pc-windows-msvc", "records": records}, indent=2) + "\n")
    print(json.dumps({"status": "Pass", "archives": len(records), "out": str(args.out.resolve())}))


if __name__ == "__main__":
    main()
