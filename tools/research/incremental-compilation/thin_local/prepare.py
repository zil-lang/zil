#!/usr/bin/env python3
"""Prepare, never build, an exact research compiler snapshot with current preflight."""
import argparse,difflib,hashlib,json,subprocess
from pathlib import Path

def digest(b):return hashlib.sha256(b).hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--repo',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--revision',default='ee786e7fd7162cfd76d5f532fe5c80d9e175ad57');a=p.parse_args()
 rev=subprocess.check_output(['git','rev-parse',a.revision],cwd=a.repo,text=True).strip();a.out.mkdir(parents=True,exist_ok=False)
 paths=subprocess.check_output(['git','ls-tree','-r','--name-only',rev,'--','Cargo.toml','Cargo.lock','rust-toolchain.toml','crates','tools/llvm/version.txt'],cwd=a.repo,text=True).splitlines()
 originals=[]
 for name in paths:
  raw=subprocess.check_output(['git','show',rev+':'+name],cwd=a.repo);dest=a.out/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(raw);originals.append({'path':name,'sha256':digest(raw),'bytes':len(raw)})
 file=a.out/'crates/zil-backend-llvm/src/build.rs';before=file.read_text();after=before
 old='options.optimization == Optimization::O2 && options.incremental_o2,'
 assert after.count(old)==1
 after=after.replace(old,old[:-1]+' && std::env::var_os("ZIL_THIN_IMPORTS_OFF").is_none(),')
 marker='    let mut cache_summary = ObjectCacheSummary {'
 assert after.count(marker)==1
 injection=r'''    // Research-only snapshot: both reference arms execute exact current
    // machine preflight and capture the actual transformed CGUs before clang.
    if options.backend == NativeBackend::Clang {
        let capture = std::path::PathBuf::from(std::env::var_os("ZIL_THIN_CAPTURE")
            .ok_or_else(|| infra("E-RESEARCH-CAPTURE", "research capture directory required"))?);
        fs::create_dir_all(&capture).map_err(|e| io_error("create research capture", &capture, e))?;
        let candidates = lir.functions.iter().filter(|f| f.foreign.is_none())
            .map(|f| format!("zil.fn.{}", f.id.0)).collect::<std::collections::BTreeSet<_>>();
        let mut exports = std::collections::BTreeSet::new();
        let mut receipts = Vec::new();
        for (id, unit) in plan.units.iter().enumerate() {
            zil_llvm_api::validate_module(unit).map_err(|e| infra("E-RESEARCH-PREFLIGHT", &e.to_string()))?;
            let text = unit.to_text();
            let hash = zil_base::Fingerprint::of(text.as_bytes()).to_hex();
            fs::write(capture.join(format!("unit-{id}.ll")), text)
                .map_err(|e| io_error("capture actual research unit", &capture, e))?;
            let dso_candidates = unit.items.iter().filter_map(|item| match item {
                zil_llvm_api::model::Item::Function(f) if f.blocks.is_some() && f.linkage == zil_llvm_api::model::Linkage::External => Some(zil_base::json_string(&f.name)),
                _ => None,
            }).collect::<Vec<_>>();
            receipts.push(format!("{{\"unit\":{id},\"machine_preflight_current\":true,\"fingerprint\":\"{hash}\",\"dso_candidates\":[{}]}}", dso_candidates.join(",")));
            for item in &unit.items {
                use zil_llvm_api::model::{Item, Linkage};
                match item {
                    Item::Function(f) if f.blocks.is_some() && f.linkage == Linkage::External => {
                        if !options.closed_executable || !candidates.contains(&f.name) {
                            exports.insert(f.name.clone());
                        }
                    }
                    Item::Global(g) if g.initializer.is_some() && g.linkage == Linkage::External => { exports.insert(g.name.clone()); }
                    _ => {}
                }
            }
        }
        fs::write(capture.join("preflight.json"), format!("[{}]\n", receipts.join(",")))
            .map_err(|e| io_error("capture research preflight", &capture, e))?;
        fs::write(capture.join("exports.txt"), exports.into_iter().collect::<Vec<_>>().join("\n") + "\n")
            .map_err(|e| io_error("capture research exports", &capture, e))?;
    }
'''
 after=after.replace(marker,injection+marker)
 # Always use the owned unit, including the single-unit case, never module.ll.
 old='let input = if options.closed_executable {'
 assert after.count(old)==1;after=after.replace(old,'let input = if options.backend == NativeBackend::Clang {')
 file.write_text(after)
 patch=''.join(difflib.unified_diff(before.splitlines(True),after.splitlines(True),fromfile='a/crates/zil-backend-llvm/src/build.rs',tofile='b/crates/zil-backend-llvm/src/build.rs'))
 (a.out/'RESEARCH.patch').write_text(patch)
 receipt={'revision':rev,'production_mutated':False,'original_files':originals,'modified_file':'crates/zil-backend-llvm/src/build.rs','original_sha256':digest(before.encode()),'patched_sha256':digest(after.encode()),'patch_sha256':digest(patch.encode()),'build_command':'source env.sh; CARGO_TARGET_DIR=<independent-target> cargo build --offline --locked --release -p zil-cli','boundary':'Only research snapshot. Clang arms execute current machine preflight and use captured actual transformed units. No compilation performed by prepare.py.'}
 (a.out/'RESEARCH.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({k:v for k,v in receipt.items() if k!='original_files'},indent=2))
if __name__=='__main__':main()
