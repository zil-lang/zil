#!/usr/bin/env python3
"""Reproduce Wire39 regressions using the immutable accepted Wire37 decoder.

Only the test writer adds five empty inventories and absent stored-life fields.
This is structural transcoding, not current validation; frozen v37 stays intact.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
BASE = "4bc1d77b5ec3bccb5b789fd813e85386f03ae13f"
WORK = ROOT.parent / ".zil-env" / "fixture-wire39"


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args])


def replace_once(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new)


def main():
    revision = git("rev-parse", BASE).decode().strip()
    WORK.mkdir(parents=True, exist_ok=True)
    files = git("ls-tree", "-r", "--name-only", revision,
                "crates/zil-base", "crates/zil-ir").decode().splitlines()
    for name in files:
        if "/src/" not in name and not name.endswith("/Cargo.toml"):
            continue
        target = WORK / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(git("show", f"{revision}:{name}"))
    cargo = git("show", f"{revision}:Cargo.toml").decode()
    begin = cargo.index("members = [")
    end = cargo.index("[workspace.package]")
    cargo = cargo[:begin] + 'members = ["crates/zil-base", "crates/zil-ir", "adapter"]\n\n' + cargo[end:]
    (WORK / "Cargo.toml").write_text(cargo)
    (WORK / "Cargo.lock").write_bytes(git("show", f"{revision}:Cargo.lock"))
    wire = WORK / "crates/zil-ir/src/wire.rs"
    text = wire.read_text()
    text = replace_once(text, "self.u32(WIRE_VERSION)\n", "self.u32(39)\n")
    text = replace_once(text,
        "self.target(&module.target)?;\n",
        "self.target(&module.target)?;\n        for _ in 0..5 { self.count(0, self.limits.max_nodes)?; }\n")
    text = replace_once(text,
        "self.u8(function.call_kind as u8)?;\n",
        "self.u8(function.call_kind as u8)?;\n        self.boolean(false)?; // No storage relation in the historical function.\n")
    text = replace_once(text,
        "for index in &closure.lending_captures {\n                self.u32(*index)?;\n            }\n",
        "for index in &closure.lending_captures {\n                self.u32(*index)?;\n            }\n            self.boolean(false)?; // No storage relation in the historical closure.\n")
    wire.write_text(text)
    adapter = WORK / "adapter"
    (adapter / "src").mkdir(parents=True, exist_ok=True)
    (adapter / "Cargo.toml").write_text('''[package]
name = "fixture-wire39-adapter"
version = "0.0.0"
edition = "2024"
[dependencies]
zil-ir = { path = "../crates/zil-ir" }
zil-base = { path = "../crates/zil-base" }
''')
    (adapter / "src/main.rs").write_text('''use std::{env, fs, path::Path};
fn main() {
    let args: Vec<_> = env::args().collect();
    let source = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    fs::create_dir_all(output).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_none_or(|s| s != "wire") { continue; }
        let bytes = fs::read(entry.path()).unwrap();
        assert_eq!(&bytes[..8], b"ZILMODP\\0");
        assert_eq!(&bytes[8..12], &37u32.to_le_bytes());
        let limits = zil_base::Limits::default();
        let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(module.schema_version, 37);
        assert_eq!(plan.schema_version, 37);
        module.schema_version = 39;
        plan.schema_version = 39;
        // Historical proof claims are deliberately not promoted to current authority.
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        fs::write(output.join(entry.file_name()), bytes).unwrap();
    }
}
''')
    sources = [ROOT / p for p in [
        "crates/zil-analyze/tests/forward_call_wire_fixtures/v37",
        "crates/zil-driver/tests/review223_fixtures/v37",
        "tests/borrowed-callable-arguments/v37",
    ]]
    environment = dict(os.environ, CARGO_TARGET_DIR=str(WORK / "target"),
                       CARGO_PROFILE_DEV_DEBUG="0", CARGO_INCREMENTAL="0")
    for source in sources:
        output = source.parent / "v39"
        subprocess.run(["cargo", "run", "--offline", "-p", "fixture-wire39-adapter", "--",
                        str(source), str(output)], cwd=WORK, env=environment, check=True)
        digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
        receipt = {"decoder_commit": revision, "status": "UntrustedStructuralTranscode",
                   "fields_added": ["empty life_declarations/life_instances/dyn_declarations/dyn_interfaces/dyn_vtables", "TypedFunction.storage_life=None", "ClosureDefinition.storage_life=None"],
                   "adapter_source_sha256": digest(adapter / "src/main.rs"),
                   "writer_source_sha256": digest(wire),
                   "inputs": {p.name: digest(p) for p in sorted(source.glob("*.wire"))},
                   "outputs": {p.name: digest(p) for p in sorted(output.glob("*.wire"))},
                   "authority": "Current Analyze and Verify must recheck the decoded module; old plan claims are not evidence"}
        (output / "migration.json").write_text(json.dumps(receipt, indent=2) + "\n")
        print(json.dumps({"output": str(output), "files": len(receipt["outputs"])}))



if __name__ == "__main__":
    main()
