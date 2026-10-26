#!/usr/bin/env python3
"""New freeze after a counter-only defect; keep all failed runs unchanged.

Only reuse an existing binary when all compiler-readable source inputs match.
The Rust harness has no build scripts/include/include_bytes/include_str source
dependencies outside .rs/Cargo.toml/Cargo.lock; the checked inventories bind
every .rs and Cargo input, and source trees are retained in their entirety.
"""
import argparse, json, shutil
from pathlib import Path
from instrument import instrument
from run import ENV, functional, inputs, inventory, invoke, sha, write

def compiled_inputs(tree):
    return {name: digest for name, digest in inventory(tree).items()
            if name.endswith('.rs') or Path(name).name in ['Cargo.toml', 'Cargo.lock']}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--prior', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--repo', type=Path, required=True)
    args = parser.parse_args()
    prior, out, repo = args.prior.resolve(), args.out.resolve(), args.repo.resolve()
    identity = json.loads((prior/'identity.json').read_text())
    out.mkdir(parents=True, exist_ok=False)
    receipts = []
    for variant in ['B', 'H']:
        source = out/f'source-{variant}'
        shutil.copytree(prior/f'source-{variant}', source)
        counted = out/f'source-{variant}-counted'
        shutil.copytree(source, counted)
        instrument(counted)
        for kind, tree in [('timed', source), ('counted', counted)]:
            suffix = '' if kind == 'timed' else '-counted'
            oldtree = prior/f'source-{variant}{suffix}'
            write(out/f'source-{variant}{suffix}-sha256.json', inventory(tree))
            before, after = compiled_inputs(oldtree), compiled_inputs(tree)
            differences = sorted(p for p in before.keys() | after.keys() if before.get(p) != after.get(p))
            binary = out/'bin'/f'{variant}-{kind}'
            binary.parent.mkdir(exist_ok=True)
            if not differences:
                oldbin = prior/'bin'/binary.name
                assert sha(oldbin) == identity['binaries'][binary.name]
                shutil.copy2(oldbin, binary)
                receipts.append(dict(binary=binary.name, action='Reused',
                    prior_binary=str(oldbin), binary_sha256=sha(binary),
                    compiler_input_files=len(before), changed_compiler_inputs=[]))
            else:
                assert variant == 'H' and kind == 'counted'
                assert differences == ['crates/zil-driver/src/session.rs']
                target = out/f'target-{variant}-{kind}'
                env = {'ZIL093_PRODUCTION_COMMIT': identity['variants'][variant],
                    'ZIL089_PRODUCTION_COMMIT': identity['variants'][variant],
                    'RUSTFLAGS': '--check-cfg=cfg(zil093_count) --cfg zil093_count'}
                invoke(['cargo', 'build', '--offline', '--locked', '--release',
                    '--manifest-path', tree/'tools/research/093/Cargo.toml',
                    '--target-dir', target], tree, out/f'logs/build-{variant}-{kind}', env)
                shutil.copy2(target/'release/pipeline093', binary)
                assert sha(binary) == sha(target/'release/pipeline093')
                write(out/f'logs/prune-{variant}-{kind}.json', dict(path=str(target),
                    retained_binary=str(binary), sha256=sha(binary)))
                shutil.rmtree(target)
                receipts.append(dict(binary=binary.name, action='Rebuilt',
                    changed_compiler_inputs=differences, binary_sha256=sha(binary)))
            print(f'{variant} {kind} {receipts[-1]["action"]}', flush=True)
    cases = inputs(repo, out)
    identity.update(cases=cases, input_sha256=inventory(out/'inputs'),
        edit_sha256=inventory(out/'edit-manifests'),
        binaries={p.name:sha(p) for p in (out/'bin').iterdir()},
        controller_sha256=sha(Path(__file__).with_name('run.py')),
        instrument_sha256=sha(Path(__file__).with_name('instrument.py')),
        repair_controller_sha256=sha(__file__),
        repair=dict(prior=str(prior), prior_identity_sha256=sha(prior/'identity.json'),
            reason='H raw_debug_calls instrument matched an inactive unit-test oracle instead of RawSnapshot::digest',
            receipts=receipts))
    write(out/'identity.json', identity)
    functional(out)

if __name__ == '__main__': main()
