#!/usr/bin/env python3
"""Transcode immutable39 fixture candidates through their real historical decoder.

The fixture subset has no relation whose binary representation changed in40.
The old writer explicitly refuses any such field; old bytes stay immutable.
Current Analyze/Verify must reconstruct all actual facts after decoding.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
BASE = "e6ad196a5"
WORK = ROOT.parent / ".zil-env" / "fixture-wire40"


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
    text = replace_once(text, "self.u32(WIRE_VERSION)\n", "self.u32(40)\n")
    # Both old BFP and optional role relations go through this writer method.
    start = text.index("    pub(super) fn function_life(&mut self, life: &FunctionLifeContract)")
    end = text.index("\n    pub(super) fn retained_port_binding", start)
    text = text[:start] + '''    pub(super) fn function_life(&mut self, _life: &FunctionLifeContract) -> WireResult<()> {
        Err(malformed("E-FIXTURE-PROFILE", "changed callable relation is outside the frozen fixture subset"))
    }
''' + text[end:]
    text = replace_once(text,
        "    fn storage_contract(&mut self, contract: &Option<StorageLifeContract>) -> WireResult<()> {\n",
        "    fn storage_contract(&mut self, contract: &Option<StorageLifeContract>) -> WireResult<()> {\n        assert!(contract.is_none(), \"changed storage contract outside fixture subset\");\n")
    wire.write_text(text)
    adapter = WORK / "adapter"
    (adapter / "src").mkdir(parents=True, exist_ok=True)
    (adapter / "Cargo.toml").write_text('''[package]
name = "fixture-wire40-adapter"
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
        assert_eq!(&bytes[8..12], &39u32.to_le_bytes());
        let limits = zil_base::Limits::default();
        let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(module.schema_version, 39);
        assert_eq!(plan.schema_version, 39);
        assert!(module.life_declarations.is_empty() && module.life_instances.is_empty());
        assert!(module.dyn_declarations.is_empty() && module.dyn_interfaces.is_empty() && module.dyn_vtables.is_empty());
        module.schema_version = 40;
        plan.schema_version = 40;
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        fs::write(output.join(entry.file_name()), bytes).unwrap();
    }
}
''')
    sources = [ROOT / p for p in [
        "crates/zil-analyze/tests/impl253_fixtures/v39",
        "crates/zil-driver/tests/review223_fixtures/v39",
        "tests/borrowed-callable-arguments/v39",
    ]]
    environment = dict(os.environ, CARGO_TARGET_DIR=str(WORK / "target"),
                       CARGO_PROFILE_DEV_DEBUG="0", CARGO_INCREMENTAL="0")
    for source in sources:
        output = source.parent / "v40"
        subprocess.run(["cargo", "run", "--offline", "-p", "fixture-wire40-adapter", "--",
                        str(source), str(output)], cwd=WORK, env=environment, check=True)
        digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
        receipt = {"decoder_commit": revision, "status": "UntrustedStructuralTranscode",
                   "profile": "No callable/storage relations or nonempty dyn/life inventories; writer refuses changed relation fields",
                   "adapter_source_sha256": digest(adapter / "src/main.rs"),
                   "writer_source_sha256": digest(wire),
                   "inputs": {p.name: digest(p) for p in sorted(source.glob("*.wire"))},
                   "outputs": {p.name: digest(p) for p in sorted(output.glob("*.wire"))},
                   "authority": "Current kernels must recheck; old plan claims grant no authority"}
        (output / "migration.json").write_text(json.dumps(receipt, indent=2) + "\n")
        print(json.dumps({"output": str(output), "files": len(receipt["outputs"])}))


if __name__ == "__main__":
    main()
