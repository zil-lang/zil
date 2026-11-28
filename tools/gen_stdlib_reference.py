#!/usr/bin/env python3
"""Deterministically extract public ZIL standard library APIs from lib/std/ into docs/guide/stdlib-reference.md."""

from __future__ import annotations

import argparse
import difflib
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STD_DIR = ROOT / "lib" / "std"
OUT_PATH = ROOT / "docs" / "guide" / "stdlib-reference.md"

MODULE_CATEGORIES: list[tuple[str, str, list[str]]] = [
    (
        "文本与数值",
        "Text & Numerics",
        ["utf8", "string", "int", "float", "text_edit"],
    ),
    (
        "集合、存储与图",
        "Collections, Storage & Graphs",
        [
            "store",
            "collections",
            "pod_hash_map",
            "pod_hash_set",
            "i64_hash_map",
            "i64_hash_set",
            "string_map",
            "owning_entries",
            "owning_map",
            "owning_hash_map",
            "owning_hash_set",
            "graph",
        ],
    ),
    (
        "迭代与算法",
        "Iteration",
        ["iter"],
    ),
    (
        "I/O、文件系统、网络与环境",
        "I/O, Filesystem, Network & System",
        ["io", "buffered_io", "fs", "path", "env", "time", "net"],
    ),
    (
        "界面与图形基础",
        "UI Foundations",
        ["ui_input", "ui_layout", "ui_raster"],
    ),
]

MODULE_DESCRIPTIONS: dict[str, str] = {
    "utf8": "UTF-8 验证、标量定位与编码工具",
    "string": "UTF-8 拥有字符串（`String`）、查找、分割与替换",
    "int": "十种整数的十进制解析与格式化",
    "float": "IEEE 754 `f32`/`f64` 文本解析与格式化",
    "text_edit": "纯文本缓冲与编辑操作原语",
    "store": "线性拥有动态存储 `Store<T>` 封装",
    "collections": "通用集合与排序辅助函数",
    "pod_hash_map": "可增长 POD 键值哈希表",
    "pod_hash_set": "可增长 POD 哈希集合",
    "i64_hash_map": "`i64` 键专用哈希表",
    "i64_hash_set": "`i64` 键专用哈希集合",
    "string_map": "字符串键哈希映射",
    "owning_entries": "拥有型容器的条目抽象与转移载体",
    "owning_map": "拥有型有序/基础映射",
    "owning_hash_map": "拥有型键值哈希表（支持 OOM 返还所有权）",
    "owning_hash_set": "拥有型哈希集合",
    "graph": "运行时尺寸拥有资源图（逻辑删除、回收与 ticket 复用）",
    "iter": "Copy 迭代、`LendingIterator` 与拥有型 `Iterator`/`Map`/`Filter`/`Drain`",
    "io": "基础文件句柄与标准 I/O 错误类型",
    "buffered_io": "固定容量缓冲读写器（`BufReader` / `BufWriter`）",
    "fs": "文件读写、目录遍历、元数据与重命名",
    "path": "路径拼接、分割与扩展名操作",
    "env": "命令行参数与进程环境访问",
    "time": "单调时钟与墙上时间查询",
    "net": "IPv4 TCP 监听与连接（`TcpListener` / `TcpStream`）",
    "ui_input": "UI 输入事件基础数据结构",
    "ui_layout": "UI 布局几何计算基础",
    "ui_raster": "UI 软件光栅化基础原语",
    "windows.env": "Windows 平台命令行与环境适配",
    "windows.float": "Windows 平台浮点文本解析与格式化适配",
    "windows.fs": "Windows Win32 Unicode (`*W`) 文件系统适配",
    "windows.io": "Windows 平台基础 I/O 与错误适配",
    "windows.net": "Windows Winsock2 TCP 网络适配",
    "windows.path": "Windows 路径分隔符与前缀处理",
    "windows.time": "Windows 高精度时钟适配",
}


@dataclass
class Decl:
    kind: str
    name: str
    signature: str
    line: int
    body_items: list[str] = field(default_factory=list)


@dataclass
class ModuleApi:
    module_name: str
    rel_path: str
    is_windows: bool
    types: list[Decl] = field(default_factory=list)
    traits: list[Decl] = field(default_factory=list)
    consts: list[Decl] = field(default_factory=list)
    functions: list[Decl] = field(default_factory=list)
    inherent_methods: list[tuple[str, int, list[Decl]]] = field(default_factory=list)
    trait_impls: list[tuple[str, int]] = field(default_factory=list)


def strip_line_comments_and_strings(text: str) -> list[tuple[int, str]]:
    """Return list of (1-indexed line_no, code_without_line_comments) preserving line count."""
    out: list[tuple[int, str]] = []
    in_block_comment = False
    for idx, raw in enumerate(text.splitlines(), 1):
        chars: list[str] = []
        i = 0
        n = len(raw)
        in_str = False
        in_char = False
        while i < n:
            ch = raw[i]
            nxt = raw[i + 1] if i + 1 < n else ""
            if in_block_comment:
                if ch == "*" and nxt == "/":
                    in_block_comment = False
                    i += 2
                else:
                    i += 1
                continue
            if in_str:
                chars.append(ch)
                if ch == "\\" and i + 1 < n:
                    chars.append(raw[i + 1])
                    i += 2
                    continue
                if ch == '"':
                    in_str = False
                i += 1
                continue
            if in_char:
                chars.append(ch)
                if ch == "\\" and i + 1 < n:
                    chars.append(raw[i + 1])
                    i += 2
                    continue
                if ch == "'":
                    in_char = False
                i += 1
                continue
            if ch == "/" and nxt == "/":
                break
            if ch == "/" and nxt == "*":
                in_block_comment = True
                i += 2
                continue
            if ch == '"':
                in_str = True
                chars.append(ch)
                i += 1
                continue
            if ch == "'":
                # Could be lifetime 'a or char literal 'x'
                if i + 2 < n and raw[i + 2] == "'":
                    in_char = True
                elif i + 3 < n and raw[i + 1] == "\\" and raw[i + 3] == "'":
                    in_char = True
                chars.append(ch)
                i += 1
                continue
            chars.append(ch)
            i += 1
        out.append((idx, "".join(chars)))
    return out


def normalize_ws(s: str) -> str:
    return re.sub(r"\s+", " ", s).strip()


def find_matching_brace(text: str, open_idx: int) -> int:
    depth = 0
    in_str = False
    i = open_idx
    n = len(text)
    while i < n:
        ch = text[i]
        if in_str:
            if ch == "\\" and i + 1 < n:
                i += 2
                continue
            if ch == '"':
                in_str = False
            i += 1
            continue
        if ch == '"':
            in_str = True
        elif ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return n - 1


def split_fn_signature(header_with_body: str) -> str:
    """Given text starting at 'pub fn ...', return normalized signature up to the function body '{'."""
    i = 0
    n = len(header_with_body)
    paren = 0
    angle = 0
    bracket = 0
    while i < n:
        ch = header_with_body[i]
        if ch == "(":
            paren += 1
        elif ch == ")":
            paren = max(0, paren - 1)
        elif ch == "[":
            bracket += 1
        elif ch == "]":
            bracket = max(0, bracket - 1)
        elif ch == "<":
            angle += 1
        elif ch == ">":
            if i > 0 and header_with_body[i - 1] == "-":
                pass  # arrow '->'
            else:
                angle = max(0, angle - 1)
        elif ch == "{" and paren == 0 and bracket == 0:
            prefix = header_with_body[:i].rstrip()
            # Handle `contract { ... }` clause which precedes the actual body `{`
            if re.search(r"\bcontract$", prefix):
                end_contract = find_matching_brace(header_with_body, i)
                i = end_contract + 1
                continue
            return normalize_ws(prefix)
        elif ch == ";" and paren == 0 and bracket == 0:
            return normalize_ws(header_with_body[:i])
        i += 1
    return normalize_ws(header_with_body)


def line_of_offset(line_starts: list[int], offset: int) -> int:
    lo, hi = 0, len(line_starts) - 1
    while lo <= hi:
        mid = (lo + hi) // 2
        if line_starts[mid] <= offset:
            lo = mid + 1
        else:
            hi = mid - 1
    return hi + 1


def extract_module(path: Path) -> ModuleApi:
    rel = path.relative_to(ROOT).as_posix()
    rel_under_std = path.relative_to(STD_DIR).with_suffix("")
    parts = rel_under_std.parts
    is_windows = len(parts) >= 2 and parts[0] == "windows"
    module_name = ".".join(parts)

    cleaned_lines = strip_line_comments_and_strings(path.read_text(encoding="utf-8"))
    flat_lines = [line for _, line in cleaned_lines]
    full_text = "\n".join(flat_lines)
    line_starts: list[int] = []
    pos = 0
    for line in flat_lines:
        line_starts.append(pos)
        pos += len(line) + 1

    api = ModuleApi(module_name=module_name, rel_path=rel, is_windows=is_windows)

    # Walk top-level items (brace depth 0)
    i = 0
    n = len(full_text)
    while i < n:
        # Skip whitespace and semicolons
        while i < n and full_text[i] in " \t\r\n;":
            i += 1
        if i >= n:
            break

        start = i
        line_no = line_of_offset(line_starts, start)
        rest = full_text[i:]

        # Skip attributes like @inline or #[...]
        if rest.startswith("@"):
            m_attr = re.match(r"@[A-Za-z_][A-Za-z0-9_]*(?:\([^)]*\))?", rest)
            if m_attr:
                i += m_attr.end()
                continue

        # Match top-level `pub <kind>`
        m_pub = re.match(r"pub\s+(enum|struct|package|resource|trait|const|type|fn)\b", rest)
        if m_pub:
            kind = m_pub.group(1)
            if kind == "fn":
                sig = split_fn_signature(rest)
                m_name = re.match(r"pub\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)", sig)
                name = m_name.group(1) if m_name else "unknown"
                api.functions.append(Decl(kind="fn", name=name, signature=sig, line=line_no))
                # Advance past body
                body_open = full_text.find("{", start + len(sig) - 1)
                if body_open != -1:
                    # Handle contract braces if any
                    probe = start
                    while True:
                        b = full_text.find("{", probe)
                        if b == -1:
                            i = n
                            break
                        if re.search(r"\bcontract\s*$", full_text[start:b]):
                            probe = find_matching_brace(full_text, b) + 1
                            continue
                        i = find_matching_brace(full_text, b) + 1
                        break
                else:
                    i = n
                continue

            if kind in {"const", "type"}:
                semi = full_text.find(";", start)
                nl = full_text.find("\n", start)
                end_idx = semi if semi != -1 else (nl if nl != -1 else n)
                sig = normalize_ws(full_text[start:end_idx])
                m_name = re.match(rf"pub\s+{kind}\s+([A-Za-z_][A-Za-z0-9_]*)", sig)
                name = m_name.group(1) if m_name else "unknown"
                target_list = api.consts if kind == "const" else api.types
                target_list.append(Decl(kind=kind, name=name, signature=sig, line=line_no))
                i = end_idx + 1
                continue

            if kind in {"enum", "struct", "package", "resource", "trait"}:
                brace = full_text.find("{", start)
                if brace == -1:
                    break
                close_brace = find_matching_brace(full_text, brace)
                header = normalize_ws(full_text[start:brace])
                body_raw = full_text[brace + 1 : close_brace]
                m_name = re.match(rf"pub\s+{kind}\s+([A-Za-z_][A-Za-z0-9_]*)", header)
                name = m_name.group(1) if m_name else "unknown"

                if kind == "trait":
                    items: list[str] = []
                    for raw_item in body_raw.splitlines():
                        s_item = normalize_ws(raw_item).rstrip(";")
                        if s_item and (s_item.startswith("type ") or s_item.startswith("fn ") or s_item.startswith("const ")):
                            items.append(s_item)
                    api.traits.append(Decl(kind="trait", name=name, signature=header, line=line_no, body_items=items))
                else:
                    body_norm = normalize_ws(body_raw)
                    full_sig = f"{header} {{ {body_norm} }}" if body_norm else f"{header} {{}}"
                    api.types.append(Decl(kind=kind, name=name, signature=full_sig, line=line_no))
                i = close_brace + 1
                continue

        # Match top-level `impl ... { ... }`
        m_impl = re.match(r"impl\b", rest)
        if m_impl:
            brace = full_text.find("{", start)
            if brace == -1:
                break
            close_brace = find_matching_brace(full_text, brace)
            header = normalize_ws(full_text[start:brace])
            if re.search(r"\bfor\b", header):
                api.trait_impls.append((header, line_no))
            else:
                # Inherent impl: scan pub fn items at depth 1 inside impl block
                methods: list[Decl] = []
                inner = brace + 1
                while inner < close_brace:
                    while inner < close_brace and full_text[inner] in " \t\r\n;":
                        inner += 1
                    if inner >= close_brace:
                        break
                    sub = full_text[inner:close_brace]
                    m_mfn = re.match(r"pub\s+fn\b", sub)
                    if m_mfn:
                        m_line = line_of_offset(line_starts, inner)
                        sig = split_fn_signature(sub)
                        m_name = re.match(r"pub\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)", sig)
                        mname = m_name.group(1) if m_name else "unknown"
                        methods.append(Decl(kind="fn", name=mname, signature=sig, line=m_line))
                        probe = inner
                        while True:
                            b = full_text.find("{", probe, close_brace)
                            if b == -1:
                                inner = close_brace
                                break
                            if re.search(r"\bcontract\s*$", full_text[inner:b]):
                                probe = find_matching_brace(full_text, b) + 1
                                continue
                            inner = find_matching_brace(full_text, b) + 1
                            break
                        continue
                    # Skip non-pub item inside impl block
                    b = full_text.find("{", inner, close_brace)
                    nl = full_text.find("\n", inner, close_brace)
                    if b != -1 and (nl == -1 or b < nl):
                        inner = find_matching_brace(full_text, b) + 1
                    elif nl != -1:
                        inner = nl + 1
                    else:
                        break
                if methods:
                    api.inherent_methods.append((header, line_no, methods))
            i = close_brace + 1
            continue

        # Skip any other top-level private declaration (fn, struct, enum, import, extern, resource, etc.)
        next_brace = full_text.find("{", start)
        next_nl = full_text.find("\n", start)
        if next_brace != -1 and (next_nl == -1 or next_brace < next_nl):
            # Check if this is a multi-line signature with a brace later on the same declaration
            i = find_matching_brace(full_text, next_brace) + 1
        elif next_nl != -1:
            # Handle private `fn ... -> ... {\n` where `{` might be on a subsequent line? Usually `{` is on same line.
            i = next_nl + 1
        else:
            break

    return api


def render_module_section(api: ModuleApi) -> list[str]:
    lines: list[str] = []
    anchor = api.module_name.replace(".", "-")
    lines.append(f'<a id="std-{anchor}"></a>')
    lines.append(f"### `std.{api.module_name}`")
    lines.append("")
    desc = MODULE_DESCRIPTIONS.get(api.module_name, "")
    source_link = f"[`{api.rel_path}`](../../{api.rel_path})"
    if desc:
        lines.append(f"{desc}。源码：{source_link}。")
    else:
        lines.append(f"源码：{source_link}。")
    lines.append("")

    if api.types:
        lines.append("**公开类型（Types / Resources / Packages / Enums）**")
        lines.append("")
        for d in api.types:
            lines.append(f"- [`L{d.line}`](../../{api.rel_path}#L{d.line}) `{d.signature}`")
        lines.append("")

    if api.traits:
        lines.append("**公开 Trait**")
        lines.append("")
        for d in api.traits:
            lines.append(f"- [`L{d.line}`](../../{api.rel_path}#L{d.line}) `{d.signature}`")
            for item in d.body_items:
                lines.append(f"  - `{item}`")
        lines.append("")

    if api.consts:
        lines.append("**公开常量**")
        lines.append("")
        for d in api.consts:
            lines.append(f"- [`L{d.line}`](../../{api.rel_path}#L{d.line}) `{d.signature}`")
        lines.append("")

    if api.inherent_methods:
        for header, imp_line, methods in api.inherent_methods:
            lines.append(f"**方法（`{header}`，[`L{imp_line}`](../../{api.rel_path}#L{imp_line})）**")
            lines.append("")
            for m in methods:
                lines.append(f"- [`L{m.line}`](../../{api.rel_path}#L{m.line}) `{m.signature}`")
            lines.append("")

    if api.functions:
        lines.append("**公开函数**")
        lines.append("")
        for f in api.functions:
            lines.append(f"- [`L{f.line}`](../../{api.rel_path}#L{f.line}) `{f.signature}`")
        lines.append("")

    if api.trait_impls:
        lines.append("**Trait 实现**")
        lines.append("")
        for header, imp_line in api.trait_impls:
            lines.append(f"- [`L{imp_line}`](../../{api.rel_path}#L{imp_line}) `{header}`")
        lines.append("")

    return lines


def generate_markdown() -> str:
    core_files = sorted(STD_DIR.glob("*.zil"))
    win_files = sorted((STD_DIR / "windows").glob("*.zil")) if (STD_DIR / "windows").is_dir() else []

    core_apis = {api.module_name: api for api in (extract_module(p) for p in core_files)}
    win_apis = [extract_module(p) for p in win_files]

    # Verify all core modules are covered by categories (append any new uncategorized module deterministically)
    categorized = {m for _, _, mods in MODULE_CATEGORIES for m in mods}
    extra_core = sorted(set(core_apis) - categorized)

    total_types = sum(len(a.types) for a in core_apis.values()) + sum(len(a.types) for a in win_apis)
    total_traits = sum(len(a.traits) for a in core_apis.values()) + sum(len(a.traits) for a in win_apis)
    total_fns = (
        sum(len(a.functions) + sum(len(ms) for _, _, ms in a.inherent_methods) for a in core_apis.values())
        + sum(len(a.functions) + sum(len(ms) for _, _, ms in a.inherent_methods) for a in win_apis)
    )

    lines: list[str] = [
        "# ZIL 标准库参考（Standard Library Reference）",
        "",
        "> **文档性质**：本页由 [`tools/gen_stdlib_reference.py`](../../tools/gen_stdlib_reference.py) 直接从仓库 [`lib/std/`](../../lib/std) 真实源码确定性抽取生成，**不手写不存在的 API**。修改标准库公开声明后，请运行 `python3 tools/gen_stdlib_reference.py` 更新本页，并用 `python3 tools/gen_stdlib_reference.py --check` 校验零漂移。",
        ">",
        "> 语言教程见 [`docs/guide/language-tour.md`](language-tour.md)；当前开发预览边界见 [`docs/guide/supported-language.md`](supported-language.md)；规范权威见 [`_zil_design.md`](../../_zil_design.md)。",
        "",
        "## 概览统计",
        "",
        f"- 跨平台 / Linux POSIX 标准库模块：**{len(core_apis)}** 个（`lib/std/*.zil`）",
        f"- Windows 平台适配模块：**{len(win_apis)}** 个（`lib/std/windows/*.zil`）",
        f"- 公开类型（`struct` / `package` / `resource` / `enum`）：**{total_types}** 个",
        f"- 公开 Trait：**{total_traits}** 个",
        f"- 公开函数与方法：**{total_fns}** 个",
        "",
        "## 模块索引",
        "",
        "### 核心标准库（`lib/std/*.zil`）",
        "",
        "| 分类 | 模块 | 类型 | Trait | 函数/方法 | 职责说明 |",
        "| --- | --- | ---: | ---: | ---: | --- |",
    ]

    for cat_zh, _cat_en, mods in MODULE_CATEGORIES:
        for mod in mods:
            if mod not in core_apis:
                continue
            a = core_apis[mod]
            fn_cnt = len(a.functions) + sum(len(ms) for _, _, ms in a.inherent_methods)
            anchor = a.module_name.replace(".", "-")
            desc = MODULE_DESCRIPTIONS.get(mod, "")
            lines.append(
                f"| {cat_zh} | [`std.{mod}`](#std-{anchor}) | {len(a.types)} | {len(a.traits)} | {fn_cnt} | {desc} |"
            )

    for mod in extra_core:
        a = core_apis[mod]
        fn_cnt = len(a.functions) + sum(len(ms) for _, _, ms in a.inherent_methods)
        anchor = a.module_name.replace(".", "-")
        lines.append(f"| 其他 | [`std.{mod}`](#std-{anchor}) | {len(a.types)} | {len(a.traits)} | {fn_cnt} | — |")

    if win_apis:
        lines.extend(
            [
                "",
                "### Windows 平台模块（`lib/std/windows/*.zil`）",
                "",
                "| 模块 | 类型 | Trait | 函数/方法 | 职责说明 |",
                "| --- | ---: | ---: | ---: | --- |",
            ]
        )
        for a in win_apis:
            fn_cnt = len(a.functions) + sum(len(ms) for _, _, ms in a.inherent_methods)
            anchor = a.module_name.replace(".", "-")
            desc = MODULE_DESCRIPTIONS.get(a.module_name, "")
            lines.append(
                f"| [`std.{a.module_name}`](#std-{anchor}) | {len(a.types)} | {len(a.traits)} | {fn_cnt} | {desc} |"
            )

    lines.append("")

    for cat_zh, cat_en, mods in MODULE_CATEGORIES:
        present = [core_apis[m] for m in mods if m in core_apis]
        if not present:
            continue
        lines.append(f"## {cat_zh}（{cat_en}）")
        lines.append("")
        for a in present:
            lines.extend(render_module_section(a))

    if extra_core:
        lines.append("## 其他核心模块")
        lines.append("")
        for mod in extra_core:
            lines.extend(render_module_section(core_apis[mod]))

    if win_apis:
        lines.append("## Windows 平台模块（`lib/std/windows/`）")
        lines.append("")
        lines.append(
            "Windows 目标编译时，`std.env` / `std.float` / `std.fs` / `std.io` / `std.net` / `std.path` / `std.time` 的底层实现由下列 Win32/UCRT/Winsock2 模块提供，保持与跨平台接口契约一致。"
        )
        lines.append("")
        for a in win_apis:
            lines.extend(render_module_section(a))

    return "\n".join(lines).rstrip() + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify docs/guide/stdlib-reference.md is up to date")
    args = parser.parse_args()

    rendered = generate_markdown()
    if args.check:
        if not OUT_PATH.is_file():
            print(f"ERROR: missing {OUT_PATH.relative_to(ROOT)}", file=sys.stderr)
            return 1
        existing = OUT_PATH.read_text(encoding="utf-8")
        if existing != rendered:
            diff = "".join(
                difflib.unified_diff(
                    existing.splitlines(keepends=True),
                    rendered.splitlines(keepends=True),
                    fromfile=str(OUT_PATH.relative_to(ROOT)),
                    tofile="generated",
                    n=2,
                )
            )
            print(diff[:4000], file=sys.stderr)
            print("ERROR: docs/guide/stdlib-reference.md is out of date; run python3 tools/gen_stdlib_reference.py", file=sys.stderr)
            return 1
        print(f"OK: {OUT_PATH.relative_to(ROOT)} matches lib/std/ ({len(rendered.splitlines())} lines)")
        return 0

    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUT_PATH.write_text(rendered, encoding="utf-8")
    print(f"Wrote {OUT_PATH.relative_to(ROOT)} ({len(rendered.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
