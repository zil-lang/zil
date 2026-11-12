#!/usr/bin/env python3
"""Fail-closed machine-quality evidence for explicit post-O2 closed output.

This is instruction/CFG/address-reference comparison, not an x86 semantic
equivalence prover. Unknown encodings/references, live changes, or missing
producer evidence fail. It never changes the ordinary ELF-byte-equality gate.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess

POLICY = "late-closed-v1"
TOOLS = {name: Path("/usr/bin") / name for name in ("objdump", "readelf", "nm")}
LIMIT = 128 * 1024 * 1024


class QualityError(ValueError):
    pass


def require(value, message):
    if not value:
        raise QualityError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def dump(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n")


def read_regular(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        a = os.fstat(fd)
        import stat
        require(stat.S_ISREG(a.st_mode) and a.st_size <= LIMIT, "unbounded/nonregular input")
        with os.fdopen(fd, "rb", closefd=False) as stream:
            data = stream.read(LIMIT + 1)
        b = os.fstat(fd)
        require((a.st_dev, a.st_ino, a.st_size, a.st_mtime_ns, a.st_ctime_ns) ==
                (b.st_dev, b.st_ino, b.st_size, b.st_mtime_ns, b.st_ctime_ns), "input changed while read")
        require(len(data) == a.st_size, "input size changed")
        return data
    finally:
        os.close(fd)


def tool(name, flags, elf, directory):
    executable = TOOLS[name].resolve(strict=True)
    before = read_regular(executable)
    env = {"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C", "DEBUGINFOD_URLS": ""}
    version = subprocess.run([str(executable), "--version"], capture_output=True, env=env, timeout=30)
    argv = [str(executable), *flags, str(elf)]
    result = subprocess.run(argv, capture_output=True, env=env, timeout=120)
    stem = name + ("-" + "-".join(x.strip("-") for x in flags) if flags else "")
    (directory / (stem + ".stdout")).write_bytes(result.stdout)
    (directory / (stem + ".stderr")).write_bytes(result.stderr)
    (directory / (stem + ".version")).write_bytes(version.stdout + version.stderr)
    dump(directory / (stem + ".command.json"), {"argv": argv, "tool": str(executable),
         "tool_sha256": digest(before), "exit": result.returncode,
         "stdout_sha256": digest(result.stdout), "stderr_sha256": digest(result.stderr)})
    require(before == read_regular(executable), "analysis tool changed")
    require(result.returncode == version.returncode == 0 and not result.stderr, "analysis tool failed")
    return result.stdout.decode("utf-8", errors="strict")


SYM = re.compile(r"^\s*\d+:\s+([0-9a-fA-F]+)\s+(\d+)\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)\s*(.*?)\s*$")
INSN = re.compile(r"^\s*([0-9a-fA-F]+):\s*\t([0-9a-fA-F ]+)\t(.+?)\s*$")
LABEL = re.compile(r"^([0-9a-fA-F]+) <(.+)>:$")


def symbols(text):
    result = {}
    for line in text.splitlines():
        match = SYM.match(line)
        if not match:
            continue
        address, size, kind, bind, visibility, section, name = match.groups()
        name = re.sub(r" \(\d+\)$", "", name)
        if not name:
            continue
        row = {"address": int(address, 16), "size": int(size), "kind": kind,
               "bind": bind, "visibility": visibility, "section": section, "name": name}
        key = (name, section, row["address"])
        require(key not in result or result[key] == row, "contradictory symbol table rows")
        result[key] = row
    require(result, "missing symbol table")
    return list(result.values())


def sections(data):
    require(data[:6] == b"\x7fELF\x02\x01", "only ELF64 little-endian is admitted")
    require(len(data) >= 64, "truncated ELF")
    fields = struct.unpack_from("<16sHHIQQQIHHHHHH", data)
    require(fields[1] in (2, 3) and fields[2] == 62 and fields[3] == 1, "unknown ELF target/type")
    offset, stride, count, names = fields[6], fields[11], fields[12], fields[13]
    require(stride == 64 and 0 < names < count and offset + count * stride <= len(data), "unknown section table")
    raw = [struct.unpack_from("<IIQQQQIIQQ", data, offset + i * stride) for i in range(count)]
    ns = raw[names]
    require(ns[4] + ns[5] <= len(data), "bad section-name table")
    names_data = data[ns[4]:ns[4] + ns[5]]
    result = {}
    total = 0
    for i, row in enumerate(raw):
        name_offset, kind, flags, address, start, size = row[:6]
        require(name_offset < len(names_data), "invalid section name")
        end = names_data.find(b"\0", name_offset)
        require(end >= 0, "unterminated section name")
        name = names_data[name_offset:end].decode("utf-8", errors="strict")
        require(size <= LIMIT and (kind == 8 or start + size <= len(data)), "section outside ELF/bound")
        total += size
        require(total <= LIMIT, "aggregate section materialization exceeds bound")
        result[i] = {"name": name, "kind": kind, "flags": flags, "address": address,
                     "size": size, "data": bytes(size) if kind == 8 else data[start:start + size]}
    return result


def disassembly(text):
    instructions, labels = {}, {}
    for line in text.splitlines():
        match = LABEL.match(line)
        if match:
            address, name = match.groups()
            labels[int(address, 16)] = name
        match = INSN.match(line)
        if match:
            address, encoded, assembly = match.groups()
            address = int(address, 16)
            code = bytes.fromhex(encoded)
            require(0 < len(code) <= 15 and "(bad)" not in assembly and ".byte" not in assembly,
                    "unrecognized instruction")
            require(address not in instructions, "duplicate disassembly address")
            instructions[address] = (code, assembly)
    require(instructions, "no actual disassembly")
    return instructions, labels


def relocation_rows(text):
    result = {}
    for line in text.splitlines():
        match = re.match(r"^([0-9a-fA-F]{8,16})\s+[0-9a-fA-F]+\s+(R_X86_64_\S+)\s+(.*)$", line)
        if not match:
            continue
        offset, kind, tail = match.groups()
        offset = int(offset, 16)
        parts = tail.split()
        if kind == "R_X86_64_RELATIVE":
            require(len(parts) == 1, "unknown relative relocation")
            row = {"kind": kind, "relative": int(parts[0], 16)}
        else:
            require(kind in {"R_X86_64_GLOB_DAT", "R_X86_64_JUMP_SLOT", "R_X86_64_64"},
                    "unsupported relocation: " + kind)
            require(len(parts) == 4 and parts[2] == "+", "unknown symbolic relocation")
            row = {"kind": kind, "symbol": parts[1], "addend": int(parts[3], 16)}
        require(offset not in result, "overlapping relocation")
        result[offset] = row
    return result


class Image:
    def __init__(self, data, symtext, asmtext, reltext):
        self.data, self.sections = data, sections(data)
        self.elf_abi = (data[:16] + data[16:24] + data[48:52]).hex()
        self.symbols = symbols(symtext)
        self.instructions, self.labels = disassembly(asmtext)
        self.relocations = relocation_rows(reltext)
        for address, (code, _) in self.instructions.items():
            owners = [s for s in self.sections.values() if s["flags"] & 4 and
                      s["address"] <= address and address + len(code) <= s["address"] + s["size"]]
            require(len(owners) == 1, "decoded instruction outside unique executable section")
            sec = owners[0]
            at = address - sec["address"]
            require(sec["data"][at:at + len(code)] == code, "disassembly is not actual ELF bytes")
        self.functions = {}
        self.positions = {}
        for sym in self.symbols:
            require(sym["kind"] != "IFUNC" or sym["section"] == "UND", "defined IFUNC unsupported")
            if sym["kind"] != "FUNC" or sym["section"] == "UND":
                continue
            require(sym["section"].isdigit(), "non-section function")
            sec = self.sections[int(sym["section"])]
            require(sec["flags"] & 4, "function outside executable section")
            require(sym["address"] in self.instructions, "function has no decoded first instruction")
            start = sym["address"]
            end = start + sym["size"]
            if not sym["size"]:
                # Zero-sized CRT symbols have no declared extent: conservatively
                # include their entire interval up to the next function/section.
                following = [x["address"] for x in self.symbols if x["kind"] == "FUNC"
                             and x["section"] == sym["section"] and x["address"] > start]
                end = min(following, default=sec["address"] + sec["size"])
            require(sec["address"] <= start < end <= sec["address"] + sec["size"], "invalid function extent")
            body = sorted(a for a in self.instructions if start <= a < end)
            cursor = start
            for index, address in enumerate(body):
                require(address == cursor, "gap/undecoded function bytes")
                code = self.instructions[address][0]
                at = address - sec["address"]
                require(sec["data"][at:at + len(code)] == code, "disassembly is not actual ELF bytes")
                cursor += len(code)
                self.positions.setdefault(address, []).append((sym["name"], index))
            require(cursor == end, "function extent not completely decoded")
            value = {"symbol": sym, "end": end, "body": body, "derived_zero_size": not sym["size"]}
            require(sym["name"] not in self.functions or self.functions[sym["name"]] == value,
                    "ambiguous same-named function")
            self.functions[sym["name"]] = value
        require(self.functions, "no defined STT_FUNC")

    def address_identity(self, address):
        """Identity of an address value, without recursively dereferencing it."""
        if address in self.positions:
            return {"code": sorted(self.positions[address])}
        names = sorted((s["name"], s["kind"], s["bind"], s["visibility"])
                       for s in self.symbols if s["section"].isdigit() and s["address"] == address)
        require(names, "unrepresented relocation address value")
        return {"symbols_at_address": names}

    def relocation_value(self, row):
        if "relative" in row:
            return {"kind": row["kind"], "target": self.address_identity(row["relative"])}
        return row

    def reference(self, address, active=()):
        require(address not in active, "cyclic address reference")
        if address in self.positions:
            return {"code": sorted(self.positions[address])}
        label = self.labels.get(address)
        if label and re.fullmatch(r"[^<>+\s]+@plt", label):
            return {"plt": label, "instructions": self.plt_body(address)}
        objects = [s for s in self.symbols if s["kind"] == "OBJECT" and s["section"].isdigit()
                   and s["address"] <= address < s["address"] + max(1, s["size"])]
        if objects:
            facts = []
            for sym in sorted(objects, key=lambda x: x["name"]):
                sec = self.sections[int(sym["section"])]
                start = sym["address"] - sec["address"]
                require(start >= 0 and start + sym["size"] <= len(sec["data"]), "object outside section")
                data = sec["data"][start:start + sym["size"]]
                pieces, cursor = [], 0
                for offset, row in sorted(self.relocations.items()):
                    if not sym["address"] <= offset < sym["address"] + sym["size"]:
                        continue
                    at = offset - sym["address"]
                    require(cursor <= at and at + 8 <= len(data), "partial/overlapping data relocation")
                    pieces.extend([data[cursor:at].hex(), self.relocation_value(row)])
                    cursor = at + 8
                pieces.append(data[cursor:].hex())
                facts.append({"name": sym["name"], "offset": address - sym["address"],
                              "size": sym["size"], "binding": sym["bind"],
                              "visibility": sym["visibility"], "data": pieces})
            return {"objects": facts}
        if address in self.relocations:
            return {"relocation": self.relocation_value(self.relocations[address])}
        secs = [s for s in self.sections.values() if s["flags"] & 2 and
                s["address"] <= address < s["address"] + s["size"]]
        require(len(secs) == 1 and not secs[0]["flags"] & 4, "unresolved code/data reference")
        sec = secs[0]
        require(not any(sec["address"] <= x < sec["address"] + sec["size"] for x in self.relocations),
                "anonymous relocated section reference unsupported")
        return {"section": sec["name"], "offset": address - sec["address"],
                "whole_section_sha256": digest(sec["data"])}

    def plt_body(self, start):
        # PLT target meaning must include its actual GOT relocation, not just
        # an objdump label. Unknown stub forms refuse the comparison.
        code, text = self.instructions[start]
        match = re.search(r"#\s*([0-9a-fA-F]+)\s+<", text)
        require(text.startswith("jmp ") and match, "unknown PLT stub")
        target = int(match.group(1), 16)
        require(target in self.relocations and "symbol" in self.relocations[target], "unbound PLT target")
        return {"entry": self.normalize_instruction(start), "binding": self.relocations[target]}

    def normalize_instruction(self, address):
        code, text = self.instructions[address]
        assembly, _, comment = text.partition("#")
        assembly = " ".join(assembly.strip().split())
        require(assembly and not assembly.startswith((".byte", "(bad)")), "unknown instruction")
        encoded = [f"{x:02x}" for x in code]
        mnemonic, _, operand = assembly.partition(" ")
        target = None
        kind = "ordinary"
        if re.match(r"^(j\w*|loop\w*|call\w*)$", mnemonic):
            kind = "call" if mnemonic.startswith("call") else "branch"
            direct = re.fullmatch(r"([0-9a-fA-F]+)\s+<[^>]+>", operand)
            if direct:
                target = int(direct.group(1), 16)
                prefix = 0
                while prefix < len(code) and code[prefix] in {0x2e, 0x3e, 0x66, 0x67, 0xf2, 0xf3}:
                    prefix += 1
                op = code[prefix:]
                width = (4 if op[:1] in (b"\xe8", b"\xe9") or
                         (len(op) >= 2 and op[0] == 0x0f and 0x80 <= op[1] <= 0x8f) else
                         1 if op and (op[0] == 0xeb or 0x70 <= op[0] <= 0x7f or 0xe0 <= op[0] <= 0xe3) else 0)
                head = 2 if op[:1] == b"\x0f" else 1
                require(width and len(op) == head + width, "unknown direct branch encoding")
                delta = target - address - len(code)
                require(int.from_bytes(code[-width:], "little", signed=True) == delta, "branch text/bytes mismatch")
                encoded[-width:] = ["REL"] * width
                assembly = mnemonic + " " + json.dumps(self.reference(target), sort_keys=True)
        if "rip" in operand:
            match = re.search(r"\[rip(?P<sign>[+-])0x(?P<delta>[0-9a-fA-F]+)\]", operand)
            cm = re.match(r"\s*([0-9a-fA-F]+)\s+<[^>]+>\s*$", comment)
            require(match and cm, "unresolved RIP-relative reference")
            delta = int(match.group("delta"), 16) * (1 if match.group("sign") == "+" else -1)
            target = address + len(code) + delta
            require(target == int(cm.group(1), 16), "RIP text/bytes mismatch")
            disp = (delta & 0xffffffff).to_bytes(4, "little")
            positions = [i for i in range(len(code) - 3) if code[i:i + 4] == disp]
            require(len(positions) == 1, "ambiguous RIP displacement encoding")
            at = positions[0]; encoded[at:at + 4] = ["RIP"] * 4
            ref = self.reference(target)
            require(mnemonic == "lea" or not any(x["size"] == 0 for x in ref.get("objects", [])),
                    "memory load/store references zero-size data marker")
            assembly = assembly.replace(match.group(0), "[" + json.dumps(ref, sort_keys=True) + "]")
        elif comment:
            raise QualityError("uninterpreted disassembler comment")
        # A plain immediate might be an address without a relocation. Do not
        # silently treat an ambiguous address-sized value as an ordinary literal.
        if target is None:
            for value in re.findall(r"0x([0-9a-fA-F]+)", operand):
                number = int(value, 16)
                require(not any(s["flags"] & 2 and s["address"] <= number < s["address"] + s["size"]
                                for s in self.sections.values()), "ambiguous unannotated address immediate")
        require("<" not in assembly and ">" not in assembly, "uninterpreted symbolic operand")
        return {"bytes": encoded, "assembly": assembly, "kind": kind,
                "target": self.reference(target) if target is not None else None}

    def normalized(self):
        result = {}
        for name, fn in sorted(self.functions.items()):
            body = fn["body"]
            records = [self.normalize_instruction(a) for a in body]
            edges = []
            for index, record in enumerate(records):
                mnemonic = self.instructions[body[index]][1].split()[0]
                if record["kind"] == "branch":
                    edges.append([index, record["target"] or {"indirect": record["assembly"]}])
                if mnemonic not in {"ret", "retq", "jmp", "ud2", "hlt", "int3"} and index + 1 < len(body):
                    edges.append([index, {"local": index + 1}])
            sym = fn["symbol"]
            section = self.sections[int(sym["section"])]
            result[name] = {"declared_size": sym["size"], "derived_zero_size": fn["derived_zero_size"],
                            "binding": sym["bind"], "visibility": sym["visibility"],
                            "section": section["name"], "section_flags": section["flags"],
                            "instructions": records, "instruction_CFG": edges}
        return result


def checked_receipt(value):
    require(isinstance(value, dict) and value.get("policy") == POLICY, "missing/current late policy")
    require(type(value.get("ran_globaldce")) is bool, "missing actual late execution status")
    sets = {}
    for key in ("candidates", "eligible", "before_definitions", "after_definitions", "removed"):
        raw = value.get(key)
        require(isinstance(raw, list) and all(isinstance(x, str) and x for x in raw) and len(raw) == len(set(raw)),
                "invalid exact late name set: " + key)
        sets[key] = set(raw)
    require(sets["after_definitions"] <= sets["before_definitions"] and
            sets["removed"] == sets["before_definitions"] - sets["after_definitions"] and
            sets["eligible"] <= sets["candidates"] & sets["before_definitions"], "contradictory late sets")
    require(value["ran_globaldce"] or not sets["removed"], "removals without actual late pass")
    return value


def receipt_identity(value):
    # Actual wall time is evidence, not part of this deterministic name proof.
    return {key: value[key] for key in ("policy", "ran_globaldce", "candidates", "eligible",
            "before_definitions", "after_definitions", "removed")}


def deletion_evidence(row, index, cold):
    summary = row["resident"]["report"]["native_codegen"]
    reports = summary["object_reports"]
    require(isinstance(reports, list) and type(summary.get("actual_units")) is int and
            summary["actual_units"] == len(reports), "incomplete current native unit receipts")
    inventory = row.get("inventory", {}).get("objects", [])
    require(isinstance(inventory, list) and (not inventory or
            [x.get("unit_index") for x in inventory] == list(range(len(reports)))),
            "incomplete/duplicate/out-of-order current object inventory")
    objects = {x["unit_index"]: x for x in inventory}
    removed, pending = set(), {}
    require(reports, "no current native unit receipts")
    for unit, report in enumerate(reports):
        cache = report.get("cache", {})
        key = cache.get("action_key")
        obj = objects.get(unit)
        sha = report.get("object_fingerprint") or report.get("object_sha256") or (obj or {}).get("object", {}).get("sha256")
        require(isinstance(key, str) and re.fullmatch(r"[0-9a-f]{64}", key) and
                isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{64}", sha), "missing actual unit action-key/object SHA binding")
        if obj:
            require(obj.get("action_key") == key, "inventory action-key mismatch")
            require(obj.get("object", {}).get("sha256") == sha, "inventory object SHA mismatch")
        name = key + ":" + sha + ":" + POLICY
        value = report.get("late_closed")
        if value is not None:
            require(cache.get("state") == "miss", "cached unit claims fresh late execution")
            value = checked_receipt(value)
            require(name not in index or receipt_identity(index[name]) == receipt_identity(value),
                    "same object has contradictory producer evidence")
            require(name not in pending or receipt_identity(pending[name]) == receipt_identity(value),
                    "contradictory current producer evidence")
            pending[name] = value
        else:
            require(not cold and cache.get("state") == "hit" and name in index,
                    "unknown cached producer; current candidates cannot authorize deletion")
            policy = report.get("late_closed_policy")
            require(isinstance(policy, dict) and policy.get("policy") == POLICY and
                    policy.get("execution") == "object-cache" and policy.get("producer_action_key") == key,
                    "cached unit lacks current exact policy/key binding")
            value = checked_receipt(index[name])
            require(policy.get("candidates") == value["candidates"], "cached unit candidate contract changed")
        removed.update(value["removed"])
    return removed, pending


def publish_pair(record, identity, directory, state, pending, index_path):
    record["valid"] = True
    record["compiler_runtime_policy_identity"] = identity
    dump(directory / "result.json", record)
    state["units"].update(pending)
    temporary_index = index_path.with_suffix(".tmp")
    dump(temporary_index, state)
    os.replace(temporary_index, index_path)
    return record


def compare_pair(pair, out):
    """Compare {'baseline': row, 'candidate': row}; out is shared RUN root.

    Unknown/mismatched evidence raises QualityError, after preserving available
    raw artifacts and a failed result. Receipt index is block-local, pinned to
    compiler/runtime identity and published only after a complete valid pair.
    """
    out = Path(out).resolve()
    root = out / "closed-machine-quality"
    root.mkdir(exist_ok=True)
    candidate = pair["candidate"]; baseline = pair["baseline"]
    for key in ("block", "request_index", "request", "measured"):
        require(candidate.get(key) == baseline.get(key), "pair identity mismatch")
    block = candidate["block"]
    require(type(block) is int and block >= 0, "invalid block")
    require(type(candidate.get("request_index")) is int and candidate["request_index"] >= 0 and
            re.fullmatch(r"[a-z][a-z0-9_-]{0,60}", candidate.get("request", "")) and
            type(candidate.get("measured")) is bool, "invalid pair label")
    tag = f"block-{block}-{candidate['request_index']}-{candidate['request']}-{'measured' if candidate['measured'] else 'parent'}"
    directory = root / tag
    directory.mkdir(exist_ok=False)
    record = {"schema": "closed-machine-quality-v1", "valid": False, "scope":
              "Strict retained STT_FUNC instruction/CFG/address-binding equality; not semantic equivalence or unwind proof", "pair": tag}
    try:
        raw = {}
        for arm, row in (("baseline", baseline), ("candidate", candidate)):
            path = Path(row["resident"]["report"]["artifacts"]["executable"])
            require(path.is_absolute() and path.resolve().is_relative_to(out), "ELF path escapes run")
            data = read_regular(path)
            require(data[:6] == b"\x7fELF\x02\x01", "artifact is not ELF64 little-endian")
            sha = digest(data)
            arm_dir = root / "elf" / sha; arm_dir.mkdir(parents=True, exist_ok=True)
            elf = arm_dir / "addressed.elf"
            if elf.exists():
                require(read_regular(elf) == data, "corrupt content-addressed ELF evidence")
            else:
                elf.write_bytes(data)
            raw[arm] = (data, sha, arm_dir, elf)
            record[arm] = {"elf_sha256": sha, "bytes": len(data),
                           "artifact_directory": str(arm_dir.relative_to(out))}
            require(row.get("executable_sha256") == sha, "published ELF differs from row")
        for row in (baseline, candidate):
            report = row["resident"]["report"]
            require(report.get("status") == "Success" and row.get("valid") is True and report.get("verification_runs") == 1 and
                    report.get("diagnostics") == [] and row.get("native") and
                    all(n.get("valid") is True for n in row["native"]), "pair has not passed current Verify/native checks")
        comparison_key = "llvm_comparison_sha256_excluding_identity_comment"
        require(re.fullmatch(r"[0-9a-f]{64}", baseline.get(comparison_key, "")) and
                baseline[comparison_key] == candidate.get(comparison_key), "current LLVM inputs are not equal")
        require(re.fullmatch(r"[0-9a-f]{64}", baseline.get("source_identity_sha256", "")) and
                baseline["source_identity_sha256"] == candidate.get("source_identity_sha256"),
                "current source identities are not equal")
        manifest = json.loads((out / "manifest.json").read_text())
        analysis_identity = {"comparator": digest(read_regular(Path(__file__).resolve())),
                             "tools": {name: digest(read_regular(path.resolve(strict=True)))
                                       for name, path in TOOLS.items()}}
        toolchain = root / "analysis-identity.json"
        if toolchain.exists():
            require(json.loads(toolchain.read_text()) == analysis_identity, "analysis implementation/toolchain changed")
        else:
            dump(toolchain, analysis_identity)
        archived_helpers = manifest.get("native_quality_protocol", {}).get("files", [])
        require(any(x.get("path") == "elf_instruction_quality.py" and
                    x.get("sha256") == analysis_identity["comparator"] for x in archived_helpers),
                "comparator is not bound by run manifest")
        runtime = candidate.get("runtime", {}).get("llvm_dsos")
        require(isinstance(runtime, list) and len(runtime) == 1 and
                isinstance(runtime[0], dict) and re.fullmatch(r"[0-9a-f]{64}", runtime[0].get("sha256", "")),
                "missing actual candidate runtime identity")
        identity = {"source": manifest["source_shas"]["candidate"],
                    "binary": manifest["binaries"]["candidate"]["sha256"],
                    "baseline_source": manifest["source_shas"]["baseline"],
                    "baseline_binary": manifest["binaries"]["baseline"]["sha256"],
                    "runtime": runtime, "policy": POLICY, "analysis_identity": analysis_identity}
        require(re.fullmatch(r"[0-9a-f]{40}", identity["source"]) and
                re.fullmatch(r"[0-9a-f]{64}", identity["binary"]) and
                re.fullmatch(r"[0-9a-f]{40}", identity["baseline_source"]) and
                re.fullmatch(r"[0-9a-f]{64}", identity["baseline_binary"]), "unfixed compiler identity")
        pin = digest(json.dumps(identity, sort_keys=True).encode())
        index_path = root / f"producer-index-block-{block}.json"
        cold = candidate["request"] == "cold" and candidate["measured"]
        state = {"identity": pin, "units": {}}
        if index_path.exists():
            state = json.loads(index_path.read_text())
            require(not cold, "cold block cannot inherit producer receipts")
            require(state["identity"] == pin, "compiler/runtime/contract identity changed")
        else:
            require(cold, "block has no validated fresh cold producer")
        # Producer authority is required by BOTH result proofs. Byte equality
        # cannot manufacture a current receipt or borrow another block's cache.
        allowed, pending = deletion_evidence(candidate, state["units"], cold)
        if raw["baseline"][0] == raw["candidate"][0]:
            record.update(proof="raw_elf_identity", machine_normalization_used=False,
                          machine_normalization_executed=False, deleted_functions=[],
                          changed_functions=[], receipt_removed=sorted(allowed),
                          scope="Complete addressed ELF byte equality after current input, Verify/native and producer checks; machine normalization was not executed")
            return publish_pair(record, identity, directory, state, pending, index_path)
        record.update(proof="normalized_retained_machine_identity", machine_normalization_used=True,
                      machine_normalization_executed=False)
        normalized, abi = {}, {}
        for arm, row in (("baseline", baseline), ("candidate", candidate)):
            data, sha, arm_dir, elf = raw[arm]
            analysis_file = arm_dir / "analysis.json"
            if analysis_file.exists():
                cached = json.loads(analysis_file.read_text())
                transcript = read_regular(arm_dir / "normalized-functions.json")
                require(cached["analysis_identity"] == analysis_identity and cached["elf_sha256"] == sha and
                        cached["normalized_sha256"] == digest(transcript), "corrupt prior analysis evidence")
                evidence = cached.get("raw_evidence")
                require(isinstance(evidence, dict) and len(evidence) == 20,
                        "incomplete raw analysis evidence")
                for filename, expected in evidence.items():
                    require(Path(filename).name == filename and
                            digest(read_regular(arm_dir / filename)) == expected,
                            "corrupt raw analysis evidence")
                normalized[arm] = json.loads(transcript)
                abi[arm] = cached["abi"]
                record[arm] = {"elf_sha256": sha, "bytes": len(data), "functions": len(normalized[arm]),
                               "normalized_sha256": digest(transcript),
                               "artifact_directory": str(arm_dir.relative_to(out))}
                continue
            tool("readelf", ["-hW", "-SW"], elf, arm_dir)
            syms = tool("readelf", ["-Ws"], elf, arm_dir)
            rels = tool("readelf", ["-rW"], elf, arm_dir)
            asm = tool("objdump", ["-d", "-w", "-M", "intel"], elf, arm_dir)
            tool("nm", ["-n", "-S", "--defined-only"], elf, arm_dir)
            image = Image(data, syms, asm, rels)
            abi[arm] = {"elf_header": image.elf_abi,
                        "interpreter": [s["data"].hex() for s in image.sections.values() if s["name"] == ".interp"]}
            normalized[arm] = json.loads(json.dumps(image.normalized(), sort_keys=True))
            record["machine_normalization_executed"] = True
            dump(arm_dir / "normalized-functions.json", normalized[arm])
            transcript_sha = digest((arm_dir / "normalized-functions.json").read_bytes())
            evidence = {p.name: digest(read_regular(p)) for p in arm_dir.iterdir()
                        if p.suffix in {".stdout", ".stderr", ".version"} or p.name.endswith(".command.json")}
            require(len(evidence) == 20, "incomplete raw analysis transcripts")
            dump(analysis_file, {"analysis_identity": analysis_identity, "elf_sha256": sha,
                                "abi": abi[arm], "normalized_sha256": transcript_sha,
                                "raw_evidence": evidence})
            record[arm] = {"elf_sha256": sha, "bytes": len(data), "functions": len(normalized[arm]),
                           "normalized_sha256": transcript_sha,
                           "artifact_directory": str(arm_dir.relative_to(out))}
        require(abi["baseline"] == abi["candidate"], "ELF target/ABI/interpreter changed")
        before, after = normalized["baseline"], normalized["candidate"]
        deleted, added = set(before) - set(after), set(after) - set(before)
        require(not added, "new candidate STT_FUNC definitions: " + repr(sorted(added)))
        require(deleted <= allowed, "function deletion absent from exact producer receipt: " + repr(sorted(deleted - allowed)))
        changed = [name for name in sorted(after) if after[name] != before[name]]
        record.update(deleted_functions=sorted(deleted), receipt_removed=sorted(allowed), changed_functions=changed)
        require(not changed, "retained machine instructions/CFG/ABI changed: " + repr(changed[:20]))
        return publish_pair(record, identity, directory, state, pending, index_path)
    except (OSError, ValueError, RuntimeError, KeyError, TypeError, IndexError,
            OverflowError, struct.error, subprocess.TimeoutExpired) as error:
        record["valid"] = False
        record["error"] = str(error)
        dump(directory / "result.json", record)
        raise QualityError(str(error)) from error
