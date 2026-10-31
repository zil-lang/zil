#!/usr/bin/env python3
"""Extract common ZIL source with an explicit composed-toolchain cache identity."""
import argparse
import difflib
import hashlib
import json
import re
import subprocess
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--toolchain-receipt', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    revision = subprocess.check_output(['git', 'rev-parse', args.revision], cwd=args.repo, text=True).strip()
    toolchain = json.loads(args.toolchain_receipt.read_text())
    assert toolchain['status'] == 'Pass'
    assert re.fullmatch(r'[a-z0-9][a-z0-9_-]{0,95}', toolchain['arm'])
    library = Path(toolchain['library']['path']).resolve()
    assert sha(library.read_bytes()) == toolchain['library']['sha256']
    args.out.mkdir(parents=True, exist_ok=False)
    paths = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', revision, '--',
        'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'tools/llvm/version.txt'], cwd=args.repo, text=True).splitlines()
    originals = []
    for name in paths:
        data = subprocess.check_output(['git', 'show', revision+':'+name], cwd=args.repo)
        destination = args.out/name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        originals.append({'path': name, 'bytes': len(data), 'sha256': sha(data)})
    file = args.out/'crates/zil-llvm-api/src/lib.rs'
    before = file.read_text()
    start = before.index('pub const fn pipeline_identity(')
    end = before.index('\n#[derive', start)
    block = before[start:end]
    tag = '|research-llvm-stage6='+toolchain['arm']+'|dso-sha256='+toolchain['library']['sha256']
    for literal in ['default<O0>', 'default<O2>', 'default<O2>|merge-functions=after|aliases=false',
                    'mergefunc,default<O2>|merge-functions=after|aliases=false']:
        assert block.count('"'+literal+'"') == 1
        block = block.replace('"'+literal+'"', '"'+literal+tag+'"')
    after = before[:start]+block+before[end:]
    file.write_text(after)
    patch = ''.join(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
        fromfile='a/crates/zil-llvm-api/src/lib.rs', tofile='b/crates/zil-llvm-api/src/lib.rs'))
    (args.out/'RESEARCH.patch').write_text(patch)
    (args.out/'SOURCE-ORIGINALS.json').write_text(json.dumps(originals, indent=2)+'\n')
    receipt = {'status': 'Prepared', 'scope': __doc__, 'revision': revision,
        'production_mutated': False, 'arm': toolchain['arm'], 'pipeline_identity_tag': tag,
        'original_files_manifest_sha256': sha((args.out/'SOURCE-ORIGINALS.json').read_bytes()),
        'modified_file': 'crates/zil-llvm-api/src/lib.rs', 'original_sha256': sha(before.encode()),
        'patched_sha256': sha(after.encode()), 'patch_sha256': sha(patch.encode()),
        'toolchain_receipt': {'path': str(args.toolchain_receipt.resolve()), 'sha256': sha(args.toolchain_receipt.read_bytes())},
        'library': toolchain['library'],
        'build_environment': {'ZIL_LLVM_LIB_DIR': str(library.parent), 'LD_LIBRARY_PATH': str(library.parent),
                              'RUSTFLAGS': '-C link-arg=-Wl,-rpath,'+str(library.parent), 'CARGO_BUILD_JOBS': '1'},
        'runtime_environment': {'LD_LIBRARY_PATH': str(library.parent)},
        'boundary': 'Common Git source differs only in private receipt/cache tag. Actual LLVMRunPasses pipeline is unchanged. Different real DSO is also identified by the existing runtime dladdr object-cache contract. Each process must load exactly its recorded library; loader audit remains required.',
        'prepare_tool_sha256': sha(Path(__file__).read_bytes())}
    (args.out/'RESEARCH.json').write_text(json.dumps(receipt, indent=2)+'\n')
    print(json.dumps({k: v for k, v in receipt.items() if k != 'boundary'}, indent=2))


if __name__ == '__main__':
    main()
