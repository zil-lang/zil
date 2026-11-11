import collections, datetime, hashlib, json, math, os, pathlib, statistics

root = pathlib.Path('/workspace/scratch/abf690ce880c/out/bounded-imports/runs/runtime-1679')
out = root.parent.parent
spare = set(os.sched_getaffinity(0)) - {0, 1, 2, 3}
if spare:
    os.sched_setaffinity(0, spare)
sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
load = lambda p: json.loads(pathlib.Path(p).read_text())
protocol, summary = load(root / 'protocol.json'), load(root / 'summary.json')
rows = [json.loads(line) for line in (root / 'native-rows.jsonl').read_text().splitlines()]
counts, cases, artifact_hashes, oracle_outputs = collections.Counter(), [], {}, {}
assert sha(root / 'protocol.json') == '71e918a862acd5f0e7685e6286f906b971fc442fca1d19c2430258069579dee5'
assert summary['protocol_sha256'] == sha(root / 'protocol.json')
assert summary['status'] == 'Failed' and summary['gate_pass'] is False and len(rows) == 216
for name, digest in protocol['tools'].items():
    assert sha(out / 'runtime-frozen-1679' / name) == digest
for identity in [*protocol['binaries'].values(), protocol['clang']]:
    assert sha(identity['path']) == identity['sha256']
assert sha(protocol['reference_protocol']['path']) == protocol['reference_protocol']['sha256']
assert 'release: 1.99.1' in (root / 'rustc-version.txt').read_text()
assert set(protocol['formal_orders']) == {'ABR', 'ARB', 'BAR', 'BRA', 'RAB', 'RBA'}
for position in range(3):
    assert collections.Counter(order[position] for order in protocol['formal_orders']) == {'A': 2, 'B': 2, 'R': 2}

def value(n, selected, seed):
    total = 3 if 0 in selected else 0
    for file in range(1, n):
        for fn in range(8):
            current = seed
            delta = (1 if fn == 0 else 2 if fn == 7 else 0) if file in selected else 0
            for step in range(4):
                current = (current * (2 + (file * 3 + fn + step * 2) % 11) + 1 + (file * 7 + fn * 3 + step) % 37 + (delta if step == 0 else 0)) % 251
            total += current
    return total

def artifact(program, llvm):
    lines = llvm.read_bytes().splitlines(keepends=True)
    assert sum(line.startswith(b'; verified-fingerprint: ') for line in lines) == 1
    material = b''.join(line for line in lines if not line.startswith(b'; verified-fingerprint: '))
    return dict(elf_sha256=sha(program), sidecar_raw_sha256=sha(llvm),
                sidecar_without_verified_fingerprint_sha256=hashlib.sha256(material).hexdigest(), excluded_lines=1)

expected = []
for case, fixture in protocol['cases'].items():
    n, k = fixture['total_files_per_language'], fixture['changed_files_per_language']
    selected = {(1 + j * n // k) % n for j in range(k)} if k else set()
    assert sorted(selected) == fixture['selected_indices']
    table = [value(n, selected, seed) for seed in range(251)]
    assert table == fixture['runtime_oracle']['value_by_seed']
    assert sum(table) == fixture['runtime_oracle']['cycle_sum']
    assert fixture['runtime_oracle']['cycles'] == 398 and fixture['runtime_oracle']['remainder'] == 102
    expected_vectors = {}
    for byte in [0, 65, 255]:
        # Independent direct loop, not the runner's period-times-cycle formula.
        accumulator = 0
        for i in range(100000):
            accumulator = (accumulator + table[(byte + i) % 251]) % 1000003
        expected_vectors[bytes([byte]).hex()] = accumulator.to_bytes(4, 'little').hex()
    assert {vector['stdin_hex']: vector['stdout_hex'] for vector in fixture['native_vectors']} == expected_vectors
    oracle_outputs[case] = expected_vectors
    classification = load(root / case / 'artifact-classification.json')
    for arm in ['A', 'B', 'R']:
        directory = root / case / ('compile-' + arm)
        record, raw = load(directory / 'record.json'), load(directory / 'raw-metrics.json')
        assert load(directory / 'process-attempt.json') == raw
        assert all(record[key] == val for key, val in raw.items())
        assert raw['exit'] == 0 and raw['exception'] is None and not raw['timeout'] and raw['reaped'] and raw['cleanup'] == []
        assert record['valid'] and not record['errors'] and record['performance_sample'] is False
        assert raw['command'][0] == protocol['binaries'][arm]['path']
        program = directory / 'program'
        artifact_hashes[case, arm] = sha(program)
        counts['clean_compilations'] += 1
        source = pathlib.Path(raw['command'][1] if arm == 'R' else raw['command'][2]).parent
        for name, digest in fixture['source_hashes'].items():
            assert sha(source / name) == digest
        if arm == 'R':
            assert all(flag in raw['command'] for flag in ['opt-level=2', 'overflow-checks=yes', 'panic=abort', 'codegen-units=8'])
            assert 'linker=' + protocol['clang']['path'] in raw['command']
            counts['rust_compilations'] += 1
            continue
        assert '--object-cache' not in raw['command'] and '--semantic-cache' not in raw['command']
        assert raw['command'][raw['command'].index('--clang') + 1] == protocol['clang']['path']
        report = load(directory / 'stdout')
        assert report['status'] == 'Success' and report['verification_runs'] == 1 and len(report['source_files']) == n
        counts['current_zil_Verify'] += 1
        native = report['native_codegen']
        assert native['backend'] == 'llvm-api' and native['actual_units'] == 8 and native['jobs'] == 2
        assert native['object_cache']['requested'] is False
        for unit in native['object_reports']:
            assert unit['input_kind'] == 'builder' and unit['verify_before_after'] is True and unit['preflight_ns'] > 0
            assert unit['llvm_version'] == '23.1.4' and unit['pipeline'] == 'default<O2>|merge-functions=after|aliases=false'
            counts['current_full_machine_preflight_units'] += 1
            counts['current_LLVM_before_after_units'] += 1
        assert artifact(program, directory / 'program.ll') == classification['arms'][arm]
        counts['elf_and_sidecar_rehashed'] += 2
    for field in ['elf_sha256', 'sidecar_without_verified_fingerprint_sha256']:
        equal = classification['arms']['A'][field] == classification['arms']['B'][field]
        assert classification['cross_arm_equal'][field] == equal
        counts['equal_' + field] += int(equal)
    schedule = [('extra-functional', index, arm, byte) for byte in protocol['extra_functional_stdin_hex'] for index, arm in enumerate(protocol['warmup_order'])]
    schedule += [('warmup', 0, arm, protocol['stdin_timing_hex']) for arm in protocol['warmup_order']]
    schedule += [('formal', round, arm, protocol['stdin_timing_hex']) for round, order in enumerate(protocol['formal_orders']) for arm in order]
    expected.extend((case, kind, round, position, arm, byte) for position, (kind, round, arm, byte) in enumerate(schedule))

actual = [(r['case'], r['kind'], r['round'], r['position'], r['arm'], r['stdin_hex']) for r in rows]
assert actual == expected and len(set(actual)) == 216
for row in rows:
    case, arm = row['case'], row['arm']
    directory = root / case / 'native' / f"{row['position']:03d}-{row['kind']}-{row['round']}-{arm}-{row['stdin_hex']}"
    assert load(directory / 'record.json') == row
    raw = load(directory / 'raw-metrics.json')
    assert raw == load(directory / 'process-attempt.json')
    assert all(row[key] == val for key, val in raw.items())
    assert row['valid'] and row['exit'] == 0 and not row['timeout'] and row['exception'] is None and row['reaped'] and not row['cleanup']
    assert (directory / 'stdin').read_bytes() == bytes.fromhex(row['stdin_hex'])
    assert (directory / 'stdout').read_bytes().hex() == row['stdout_hex'] == row['expected_stdout_hex'] == oracle_outputs[case][row['stdin_hex']]
    assert (directory / 'stderr').read_bytes() == b'' and row['stderr_hex'] == ''
    assert row['artifact_sha256'] == artifact_hashes[case, arm]
    counts[row['kind']] += 1
    counts['native_IO_records'] += 1

by_summary = {case['case']: case for case in summary['cases']}
for case in protocol['cases']:
    arms = {}
    for arm in ['A', 'B', 'R']:
        formal = [row for row in rows if row['case'] == case and row['arm'] == arm and row['kind'] == 'formal']
        assert len(formal) == 6
        arms[arm] = {}
        for metric in ['wall_ms', 'cpu_ms', 'rss_kib']:
            values = [row[metric] for row in formal]
            rec = dict(values=values, mean=statistics.mean(values), median=statistics.median(values),
                       p95=sorted(values)[math.ceil(.95 * len(values)) - 1], max=max(values))
            assert rec == by_summary[case]['arms'][arm][metric]
            arms[arm][metric] = rec
    ratios = dict(B_over_A=arms['B']['wall_ms']['mean'] / arms['A']['wall_ms']['mean'],
                  Rust_over_B=arms['R']['wall_ms']['mean'] / arms['B']['wall_ms']['mean'])
    assert ratios == by_summary[case]['ratios']
    passed = ratios['B_over_A'] <= 1.10 and ratios['Rust_over_B'] >= .90
    assert passed == by_summary[case]['gate_pass'] and not passed
    assert by_summary[case]['all_native_rows_valid'] and by_summary[case]['long_tails_and_failures_removed'] == 0
    cases.append(dict(case=case, arms=arms, ratios=ratios, gate_pass=passed,
                      cpu_B_over_A=arms['B']['cpu_ms']['mean'] / arms['A']['cpu_ms']['mean'],
                      elf_equal=artifact_hashes[case, 'A'] == artifact_hashes[case, 'B']))
assert counts['clean_compilations'] == 24 and counts['formal'] == 144 and counts['warmup'] == 24 and counts['extra-functional'] == 48
assert counts['current_zil_Verify'] == 16 and counts['current_full_machine_preflight_units'] == 128
assert summary['failed_cases'] == list(protocol['cases']) and summary['outliers_removed'] == 0
assert summary['all_native_outputs_valid'] is True
for key, value in summary['geomeans_additional_only'].items():
    assert abs(value - math.exp(statistics.mean(math.log(case['ratios'][key]) for case in cases))) < 1e-12
manifest = load(root / 'execution-manifest.json')
assert manifest['status'] == 'Failed' and manifest['completed'] is True
assert not (root / 'fatal-failure.json').exists()
result = dict(audit_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), decision='NoGo; all eight cases fail B/A <= 1.10',
              protocol_sha256=sha(root / 'protocol.json'), rows_sha256=sha(root / 'native-rows.jsonl'), summary_sha256=sha(root / 'summary.json'),
              counts=dict(counts), protocol_tools_binaries_clang_match=True, all_raw_IO_oracles_match=True,
              exact_six_permutations_and_full_schedule=True, all_summary_statistics_match=True, outliers_removed=0,
              no_program_or_compiler_executed=True, cases=cases,
              limitations=['All RSS values retained as wait4 observations, not asserted live-memory measurements.',
                           'Material sidecar is pre-partition whole-module IR; equality does not imply equal actual native-unit optimization inputs.',
                           'Rust90 passing cannot override the failed baseline-relative runtime gate.',
                           'Candidate compilation/concurrency measurements remain unexecuted; baseline A/A concurrency is a separate diagnostic.'])
(out / 'runtime-result-independent-audit.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({key: val for key, val in result.items() if key not in ['cases', 'limitations']}, indent=2))
print(json.dumps([dict(case=c['case'], wall={a:round(c['arms'][a]['wall_ms']['mean'], 6) for a in ['A','B','R']},
                      cpu={a:round(c['arms'][a]['cpu_ms']['mean'], 6) for a in ['A','B','R']}, ratios=c['ratios'], cpu_B_over_A=c['cpu_B_over_A']) for c in cases], indent=2))
