#!/usr/bin/env python3
"""Regenerate dispatch/input indexes from spec/authority-manifest.json. Never marks compiler expectations as executed."""
from pathlib import Path
import argparse, hashlib, json, re, sys

R = Path(__file__).resolve().parents[1]


def sha(p: str) -> str:
    return hashlib.sha256((R / p).read_bytes()).hexdigest()


def bundle_hash(file_hashes: dict[str, str]) -> str:
    canonical = ''.join(p + '\0' + h + '\n' for p, h in sorted(file_hashes.items()))
    return hashlib.sha256(canonical.encode()).hexdigest()


def render_json(obj) -> str:
    return json.dumps(obj, ensure_ascii=False, indent=2) + '\n'


def write_or_check(rel: str, text: str, check_only: bool) -> bool:
    target = R / rel
    if check_only:
        return target.is_file() and target.read_text(encoding='utf-8') == text
    target.write_text(text, encoding='utf-8')
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='verify generated indexes are up to date without modifying files')
    args = parser.parse_args()

    rules = [
        r
        for f in ('_zil_design.md', 'docs/architecture/compiler.md')
        for r in re.findall(r'<a id="(ZIL-[A-Z]+(?:-[A-Z]+)*)"></a>', (R / f).read_text(encoding='utf-8'))
    ]
    cases = []
    for group in ('closure', 'receiver', 'record', 'pipe', 'script'):
        cases.extend(json.loads((R / 'conformance' / f'{group}.json').read_text(encoding='utf-8'))['cases'])
    assert len({c['id'] for c in cases}) == len(cases)
    for c in cases:
        c['source_sha256'] = sha(c['path'])
    conformance_index_text = render_json({
        'schema_version': 1,
        'generated_from': ['conformance/' + g + '.json' for g in ('closure', 'receiver', 'record', 'pipe', 'script')],
        'execution_state': 'NotRun',
        'cases': cases,
    })

    # The registry also contains implemented diagnostics outside conformance cases.
    # Cases enrich the explicit registry; regeneration must not delete those codes.
    registered = json.loads((R / 'spec/diagnostics.json').read_text(encoding='utf-8'))
    diagnostics = {d['code']: d for d in registered['entries']}
    assert len(diagnostics) == len(registered['entries']), 'duplicate diagnostic code'
    for c in cases:
        if c['diagnostic']:
            d = diagnostics.setdefault(c['diagnostic'], {'code': c['diagnostic'], 'phases': [], 'requirements': []})
            d['phases'] = sorted(set(d['phases'] + [c['phase']]))
            d['requirements'] = sorted(set(d['requirements'] + c['requirements']))
    diagnostics_text = render_json({
        'schema_version': 1,
        'purpose': 'Explicit stable diagnostic registry enriched by conformance mappings; safety meaning is defined only by normative rule IDs',
        'entries': sorted(diagnostics.values(), key=lambda d: d['code']),
    })

    if not args.check:
        write_or_check('conformance/index.json', conformance_index_text, False)
        write_or_check('spec/diagnostics.json', diagnostics_text, False)

    # Single source of truth for normative inputs: spec/authority-manifest.json.
    manifest = json.loads((R / 'spec/authority-manifest.json').read_text(encoding='utf-8'))
    tiers = manifest['tiers']
    expected_tiers = ('language_spec', 'compiler_contracts', 'validation_contracts')
    assert tuple(tiers.keys()) == expected_tiers, f'unexpected manifest tiers: {list(tiers.keys())}'

    all_inputs: list[str] = []
    for tier_name in expected_tiers:
        for rel in tiers[tier_name]:
            assert (R / rel).is_file(), f'missing authority manifest file: {rel}'
            all_inputs.append(rel)
    assert len(all_inputs) == len(set(all_inputs)), 'duplicate path in spec/authority-manifest.json'

    # Verify spec/interfaces.json normative_inputs stays strictly synchronized with spec/authority-manifest.json.
    interfaces = json.loads((R / 'spec/interfaces.json').read_text(encoding='utf-8'))
    iface_inputs = interfaces.get('normative_inputs', [])
    assert sorted(iface_inputs) == sorted(all_inputs), (
        f'spec/interfaces.json normative_inputs differs from spec/authority-manifest.json: '
        f'only_in_manifest={sorted(set(all_inputs) - set(iface_inputs))}, '
        f'only_in_interfaces={sorted(set(iface_inputs) - set(all_inputs))}'
    )

    # Verify every spec/* file in the manifest (except README.md itself) is indexed in spec/README.md.
    spec_readme = (R / 'spec/README.md').read_text(encoding='utf-8')
    for rel in all_inputs:
        if rel.startswith('spec/') and rel != 'spec/README.md':
            name = rel.removeprefix('spec/')
            assert name in spec_readme, f'{rel} is in spec/authority-manifest.json but missing from spec/README.md'

    files = {p: sha(p) for p in sorted(all_inputs)}
    tier_hashes = {
        tier_name: bundle_hash({p: files[p] for p in tiers[tier_name]})
        for tier_name in expected_tiers
    }
    bundle = bundle_hash(files)
    normative_text = render_json({
        'schema_version': 1,
        'manifest': 'spec/authority-manifest.json',
        'algorithm': 'sha256 of sorted path + NUL + sha256 + LF entries',
        'tier_hashes': tier_hashes,
        'files': files,
        'normative_hash': bundle,
    })

    if args.check:
        stale = []
        if not write_or_check('conformance/index.json', conformance_index_text, True):
            stale.append('conformance/index.json')
        if not write_or_check('spec/diagnostics.json', diagnostics_text, True):
            stale.append('spec/diagnostics.json')
        if not write_or_check('data/normative-inputs.json', normative_text, True):
            stale.append('data/normative-inputs.json')
        if stale:
            print(f'ERROR: stale generated index files: {", ".join(stale)}', file=sys.stderr)
            return 1
    else:
        write_or_check('data/normative-inputs.json', normative_text, False)

    print(json.dumps({'rules': len(rules), 'source_cases': len(cases), 'diagnostics': len(diagnostics), 'normative_files': len(files), 'normative_hash': bundle}, ensure_ascii=False))
    return 0


if __name__ == '__main__':
    sys.exit(main())
