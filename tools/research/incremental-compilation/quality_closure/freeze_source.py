#!/usr/bin/env python3
"""Record a private structured-clone snapshot and its reproducible source patch."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import subprocess

POLICY = 'complete-direct-internal-v1'
MACHINE = 'crates/zil-backend-llvm/src/machine_cgu.rs'
CONTROLS = 'crates/zil-backend-llvm/src/machine_cgu_clone_controls.rs'
API = 'crates/zil-llvm-api/src/lib.rs'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def original(repo, revision, name):
    result = subprocess.run(['git', 'show', revision + ':' + name], cwd=repo,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        assert name == CONTROLS, result.stderr.decode()
        return b''
    return result.stdout


def difference(name, before, after):
    return ''.join(difflib.unified_diff(before.decode().splitlines(True), after.decode().splitlines(True),
        fromfile='a/' + name if before else '/dev/null', tofile='b/' + name))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--snapshot', type=Path, required=True)
    parser.add_argument('--patch-out', type=Path, required=True)
    args = parser.parse_args()
    base = args.snapshot / 'RESEARCH.BASE.json'
    if not base.exists():
        base.write_bytes((args.snapshot / 'RESEARCH.json').read_bytes())
    receipt = json.loads(base.read_text())
    revision = receipt['revision']
    old_tag = receipt['pipeline_identity_tag']
    new_tag = '|research-o2-closure=' + POLICY + old_tag
    api = args.snapshot / API
    text = api.read_text()
    if new_tag not in text:
        assert text.count(old_tag) == 4
        text = text.replace(old_tag, new_tag)
        api.write_text(text)
    assert text.count(new_tag) == 4
    patch, complete, files = '', '', []
    for name in [MACHINE, CONTROLS, API]:
        before = original(args.repo, revision, name)
        after = (args.snapshot / name).read_bytes()
        change = difference(name, before, after)
        complete += change
        if name != API:
            patch += change
        files.append(dict(path=name, original_sha256=sha(before) if before else None,
                          patched_sha256=sha(after), bytes=len(after)))
    args.patch_out.write_text(patch)
    (args.snapshot / 'RESEARCH.patch').write_text(complete)
    for field in ['modified_file', 'original_sha256', 'patched_sha256', 'patch_sha256']:
        receipt.pop(field, None)
    receipt.update(scope=__doc__, policy=POLICY, pipeline_identity_tag=new_tag,
        production_mutated=False, isolated_production_sources_modified=True,
        modified_files=files, patch_sha256=sha(complete.encode()),
        complete_clone_patch_sha256=sha(patch.encode()),
        base_source_preparation=dict(path=str(base), sha256=sha(base.read_bytes())),
        finalize_tool_sha256=sha(Path(__file__).read_bytes()),
        clone_resource_guard='sum of additional per-consumer Function::cost <= original source-function cost; otherwise explicit whole-module fallback before body copies',
        boundary='Private structured-machine partition changes apply only when incremental_o2 requests imports. Direct callee operands may reference real internal implementation clones; canonical definitions and all address/value operands remain original. Actual LLVM pipeline and DSO are unchanged. The strategy tag and complete generated modules participate in cache identity. This is Prepared, not a build or runtime claim.')
    (args.snapshot / 'RESEARCH.json').write_text(json.dumps(receipt, indent=2) + '\n')
    (args.patch_out.with_suffix('.json')).write_text(json.dumps(dict(revision=revision,
        policy=POLICY, patch_sha256=sha(patch.encode()), files=files[:2]), indent=2) + '\n')


if __name__ == '__main__':
    main()
