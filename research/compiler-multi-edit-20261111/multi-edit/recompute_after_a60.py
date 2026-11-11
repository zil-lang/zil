import collections, datetime, hashlib, json, math, pathlib, statistics

root = pathlib.Path('/workspace/scratch/abf690ce880c/out/multi-edit/runs/inc-after-streaming-a60')
out = root.parent.parent
sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
load = lambda p: json.loads(pathlib.Path(p).read_text())
proto = load(root / 'protocol.json')
rows = [json.loads(x) for x in (root / 'rows.jsonl').read_text().splitlines()]
manifest, summary = load(root / 'execution-manifest.json'), load(root / 'summary.json')
assert sha(root / 'protocol.json') == 'dbab00ff9c04bb8a58eaaf0eeba2109f3aefa0d8c2a79bab3638baa49379dd35'
assert manifest['status'] == 'screen-complete' and len(rows) == 368
for item in manifest['binaries'].values():
    assert sha(item['path']) == item['sha256']
for name, digest in proto['tools'].items():
    assert sha(out / name) == digest
expected = []
for n in proto['files']:
    for k in proto['edits']:
        for state in proto['states']:
            for block, order in enumerate(proto['order']):
                arms = list(order)
                if state != 'resident-cli':
                    arms = ['R', *arms] if block % 2 == 0 else [*arms, 'R']
                for position, arm in enumerate(arms):
                    for request in (['edit'] if state == 'cold-cli' else ['seed', 'edit']):
                        expected.append((n, k, state, block, position, arm, request))
actual = [(r['files'], r['edited_files'], r['state'], r['block'], r['position'], r['arm'], r['request']) for r in rows]
assert actual == expected and len(set(actual)) == 368
counts, kinds, oracles = collections.Counter(), collections.Counter(), {}

def model(n, k, seed):
    selected = {(1 + j * n // k) % n for j in range(k)} if k else set()
    total = 3 * (0 in selected)
    for i in range(1, n):
        for j in range(8):
            delta = (1 if j == 0 else 2 if j == 7 else 0) if i in selected else 0
            value = seed
            for step in range(4):
                value = (value * (2 + (i * 3 + j + step * 2) % 11) + 1 + (i * 7 + j * 3 + step) % 37 + (delta if step == 0 else 0)) % 251
            total += value
    return total.to_bytes(4, 'little').hex()

def identity(program, llvm):
    lines = pathlib.Path(llvm).read_bytes().splitlines(keepends=True)
    assert sum(line.startswith(b'; verified-fingerprint: ') for line in lines) == 1
    normalized = b''.join(line for line in lines if not line.startswith(b'; verified-fingerprint: '))
    return dict(elf_sha256=sha(program), sidecar_raw_sha256=sha(llvm),
                sidecar_without_verified_fingerprint_sha256=hashlib.sha256(normalized).hexdigest(), excluded_lines=1)

def native(vectors, n, k):
    assert len(vectors) == 3
    for vector, seed in zip(vectors, [0, 65, 255]):
        assert vector['stdin_hex'] == bytes([seed]).hex()
        assert vector['exit'] == 0 and vector['stderr_hex'] == '' and vector['stdout_hex'] == model(n, k, seed)
        counts['native_checks'] += 1

def verify(report, n):
    assert report['status'] == 'Success' and report['verification_runs'] == 1 and len(report['source_files']) == n
    counts['zil_verify_requests'] += 1
    native = report['native_codegen']
    units = native['object_reports']
    assert native['backend'] == 'llvm-api' and native['actual_units'] == 8 and len(units) == 8
    for unit in units:
        assert unit['pipeline'] == 'default<O2>|merge-functions=after|aliases=false' and unit['llvm_version'] == '23.1.4'
        assert isinstance(unit['preflight_ns'], int) and unit['preflight_ns'] > 0
        if unit['input_kind'] == 'object-cache':
            assert unit['producer_verified_before_after'] is True
            counts['current_object_hit_preflight_units'] += 1
        else:
            assert unit['verify_before_after'] is True
            counts['current_llvm_verified_units'] += 1
        if unit['input_kind'] == 'object-patch':
            assert unit['current_full_machine_preflight'] is True
        kinds[unit['input_kind']] += 1
        counts['current_full_preflight_units'] += 1
    assert native['object_cache']['hits'] == sum(unit['input_kind'] == 'object-cache' for unit in units)

for rel, fixture in proto['fixtures'].items():
    n, k = fixture['total_files_per_language'], fixture['changed_files_per_language']
    assert [v['stdout_hex'] for v in fixture['native_vectors']] == [model(n, k, s) for s in [0, 65, 255]]
    directory = root / 'clean-oracles' / rel
    record, report = load(directory / 'oracle.json'), load(directory / 'oracle.stdout')
    assert record['valid'] and not record['errors'] and record['exit'] == 0 and not record['timeout']
    assert record['performance_sample'] is False and record['object_cache_requested'] is False
    assert '--object-cache' not in record['command']
    verify(report, n)
    native(record['native'], n, k)
    ident = identity(directory / 'program', directory / 'program.ll')
    assert ident == record['identity']
    oracles[rel] = ident
    counts['clean_oracles'] += 1
    counts['artifact_files_rehashed'] += 2

for row in rows:
    n, k, request = row['files'], row['edited_files'], row['request']
    assert row['valid'] and not row['errors'] and row['exit'] == 0 and not row['timeout']
    assert row['measured'] == (request == 'edit')
    counts['rows'] += 1
    directory = pathlib.Path(row['command'][row['command'].index('-o') + 1]).parent
    assert load(directory / f'{request}.row.json') == row
    rel = f'n{n}/base' if request == 'seed' else f'n{n}/edit-{k}'
    fixture = proto['fixtures'][rel]
    ext = 'rs' if row['arm'] == 'R' else 'zil'
    wanted = {name: digest for name, digest in fixture['source_hashes'].items() if name.endswith('.' + ext)}
    assert row['source_hashes'] == wanted
    native(row['native'], n, 0 if request == 'seed' else k)
    if request == 'edit':
        assert {p.name: sha(p) for p in (directory / 'source').glob('*.' + ext)} == wanted
        counts['final_source_snapshots_rehashed'] += 1
    if row['arm'] == 'R':
        argv = row['command']
        assert all(flag in argv for flag in ['opt-level=2', 'overflow-checks=yes', 'panic=abort', 'codegen-units=8'])
        assert any(flag.startswith('incremental=') for flag in argv)
        counts['rust_requests'] += 1
        continue
    report = load(directory / f'{request}.stdout.json' if row['state'] == 'resident-cli' else directory / f'{request}.stdout')
    verify(report, n)
    if row['state'] == 'resident-cli':
        envelope = load(directory / f'{request}.resident.json')
        assert envelope['report'] == report and not envelope.get('session_cleared') and envelope['active_queries'] == 0
    for key in ['elf_sha256', 'sidecar_without_verified_fingerprint_sha256']:
        assert row['artifact_identity'][key] == oracles[rel][key]
    counts['recorded_artifact_equal_to_clean_requests'] += 1
    if request == 'edit':
        assert identity(directory / 'program', directory / 'program.ll') == row['artifact_identity']
        counts['final_zil_artifacts_rehashed'] += 1
        counts['artifact_files_rehashed'] += 2
assert counts['zil_verify_requests'] == 330 and counts['rust_requests'] == 48 and counts['native_checks'] == 1134

cells, index = [], {}
for n in proto['files']:
    for k in proto['edits']:
        for state in proto['states']:
            arms = {}
            for arm in ['A', 'B', 'R']:
                group = [r for r in rows if (r['files'], r['edited_files'], r['state'], r['request'], r['arm']) == (n, k, state, 'edit', arm)]
                if not group:
                    continue
                walls = [r['elapsed_ns'] / 1e6 for r in group]
                cpus = [r['cpu_seconds'] * 1000 for r in group]
                rsskey = 'rss_kib' if state == 'resident-cli' else 'max_rss_kib'
                rss = [r[rsskey] for r in group]
                blockmeans = [statistics.mean(r['elapsed_ns'] / 1e6 for r in group if r['block'] == b) for b in range(proto['blocks'])]
                arms[arm] = dict(samples=len(group), wall_ms=walls, arithmetic_mean_wall_ms=statistics.mean(walls),
                                 median_block_mean_ms=statistics.median(blockmeans), observed_p95_ms=sorted(walls)[max(0, math.ceil(.95 * len(walls)) - 1)],
                                 mean_cpu_ms=statistics.mean(cpus), cpu_ms=cpus, rss_scope=rsskey, rss_kib=rss, mean_rss_kib=statistics.mean(rss))
                if state == 'resident-cli':
                    arms[arm]['lifetime_peak_rss_kib'] = [r['lifetime_peak_rss_kib'] for r in group]
            cell = dict(files=n, edited_files=k, state=state, arms=arms,
                        B_over_A_mean=arms['B']['arithmetic_mean_wall_ms'] / arms['A']['arithmetic_mean_wall_ms'],
                        B_minus_A_ms=arms['B']['arithmetic_mean_wall_ms'] - arms['A']['arithmetic_mean_wall_ms'],
                        B_over_A_cpu_mean=arms['B']['mean_cpu_ms'] / arms['A']['mean_cpu_ms'])
            if 'R' in arms:
                cell['B_over_R_mean'] = arms['B']['arithmetic_mean_wall_ms'] / arms['R']['arithmetic_mean_wall_ms']
            cells.append(cell)
            index[n, k, state] = cell
for original in summary['cells']:
    current = index[original['files'], original['edited_files'], original['state']]
    for arm, record in original['arms'].items():
        for key in ['arithmetic_mean_wall_ms', 'median_block_mean_ms', 'observed_p95_ms']:
            assert abs(record[key] - current['arms'][arm][key]) < 1e-8
weighted = {arm: sum(proto['file_count_weights'][str(c['files'])] * proto['edit_weights'][str(c['edited_files'])] * proto['scenario_weights'][c['state']] * c['arms'][arm]['arithmetic_mean_wall_ms'] for c in cells) for arm in ['A', 'B']}
for arm in weighted:
    assert abs(weighted[arm] - summary['fixed_frequency_weighted_mean_wall_ms'][arm]) < 1e-8
assert abs(weighted['B'] / weighted['A'] - summary['weighted_B_over_A']) < 1e-12
coldcpu = {arm: sum(proto['file_count_weights'][str(c['files'])] * proto['edit_weights'][str(c['edited_files'])] * c['arms'][arm]['mean_cpu_ms'] for c in cells if c['state'] == 'cold-cli') for arm in ['A', 'B']}
result = dict(audit_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), scope='Independent read-only recomputation; no compiler, tests or resampling',
              protocol_sha256=sha(root / 'protocol.json'), rows_sha256=sha(root / 'rows.jsonl'), execution_manifest_sha256=sha(root / 'execution-manifest.json'), summary_sha256=sha(root / 'summary.json'),
              counts=dict(counts), unit_receipt_kinds=dict(kinds), expected_order_and_complete_shape=True, binary_and_helper_hashes_match=True,
              summary_all_cells_recomputed_match=True, weighted_mean_wall_ms=weighted, weighted_delta_percent=(weighted['B'] / weighted['A'] - 1) * 100,
              faster_cells=sum(c['B_over_A_mean'] < 1 for c in cells), slower_cells=sum(c['B_over_A_mean'] > 1 for c in cells),
              cold_cpu_absolute_weighted_mean_ms=coldcpu, cold_cpu_delta_percent=(coldcpu['B'] / coldcpu['A'] - 1) * 100, cells=cells,
              limits=['Weights are hypothetical, not user workload distribution.', 'Frequency observed-uncontrolled; no scheduler/allocator/frequency causal claim.',
                      'No runtime-performance, off-policy or concurrency sampling in this run.', 'Recorded seed identity verified against clean; overwritten seed binaries cannot be rehashed.',
                      'CLI max_rss and resident current/lifetime RSS have different scopes.', 'Native I/O is functionality, not runtime performance.', 'A/B4 and Rust2 samples per cell; all long tails retained.'])
(out / 'after-a60-independent-audit.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({key: value for key, value in result.items() if key not in ['cells', 'limits']}, indent=2))
