import collections, datetime, hashlib, json, math, os, pathlib, statistics

base = pathlib.Path('/workspace/scratch/abf690ce880c/out/multi-edit')
protocol_path = base / 'main-concurrency-protocol-990/protocol.json'
load = lambda p: json.loads(pathlib.Path(p).read_text())
sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
spare = set(os.sched_getaffinity(0)) - {0, 1, 2, 3}
if spare:
    os.sched_setaffinity(0, spare)
plan = load(protocol_path)
root = pathlib.Path(plan['out'])
rows = [json.loads(line) for line in (root / 'batches.jsonl').read_text().splitlines()]
interpretation = load(protocol_path.parent / 'interpretation.json')
assert sha(protocol_path) == '770d5805601c87ed08e220b8ba1aa7ab95684fac32f29bc8524edede7954d25b'
assert sha(root / 'protocol.json') == sha(protocol_path) == interpretation['protocol_sha256']
assert plan['tools']['A'] == plan['tools']['B']
assert plan['source_identities'] == dict(A='99035931d358add0eaed6d4535035c38413701dd', B='99035931d358add0eaed6d4535035c38413701dd')
for path, digest in plan['pins'].items():
    assert sha(path) == digest
for identity in plan['tools'].values():
    assert sha(identity['path']) == identity['sha256']
assert plan['cpus'] == [0, 1, 2, 3] and plan['files'] == [256]
assert plan['edits'] == [16, 64] and plan['concurrency'] == [1, 2, 4]
assert plan['blocks'] == 1 and plan['order'] == list('ARBBRA')
assert 'release: 1.99.1' in (root / 'rustc-version.txt').read_text()
completion = load(root / 'completion.json')
assert completion['complete'] and completion['failure'] is None and completion['batches'] == 36
assert completion['protocol_sha256'] == sha(protocol_path)
expected = [(n, k, c, b, p, arm) for n in plan['files'] for k in plan['edits'] for c in plan['concurrency'] for b in range(plan['blocks']) for p, arm in enumerate(plan['order'])]
assert [(r['files'], r['edited_files'], r['concurrency'], r['block'], r['position'], r['arm']) for r in rows] == expected
counts, kinds, native_expected, used_paths = collections.Counter(), collections.Counter(), {}, set()

def model(n, k, salt, seed):
    selected = {(1 + j * n // k) % n for j in range(k)} if k else set()
    total = 3 * salt if 0 in selected else 0
    for file in range(1, n):
        for function in range(8):
            delta = salt * (1 if function == 0 else 2 if function == 7 else 0) if file in selected else 0
            current = seed
            for step in range(4):
                current = (current * (2 + (file * 3 + function + step * 2) % 11) + 1 + (file * 7 + function * 3 + step) % 37 + (delta if step == 0 else 0)) % 251
            total += current
    return total.to_bytes(4, 'little').hex()

for rel, fixture in plan['fixtures'].items():
    n, k, salt = fixture['total_files_per_language'], fixture['changed_files_per_language'], fixture.get('candidate_salt', 1)
    selected = sorted({(1 + j * n // k) % n for j in range(k)}) if k else []
    assert fixture['selected_indices'] == selected
    native_expected[rel] = {bytes([seed]).hex(): model(n, k, salt, seed) for seed in [0, 65, 255]}
    assert {v['stdin_hex']: v['stdout_hex'] for v in fixture['native_vectors']} == native_expected[rel]
    for name, digest in fixture['source_hashes'].items():
        assert sha(protocol_path.parent / rel / name) == digest
for k in plan['edits']:
    signatures = [tuple(plan['fixtures'][f'n256/edit{k}/candidate{salt}']['source_hashes'].items()) for salt in [1, 2, 3, 4]]
    assert len(set(signatures)) == 4

def artifact(program, llvm):
    lines = llvm.read_bytes().splitlines(keepends=True)
    assert sum(line.startswith(b'; verified-fingerprint: ') for line in lines) == 1
    material = b''.join(line for line in lines if not line.startswith(b'; verified-fingerprint: '))
    return dict(elf_sha256=sha(program), sidecar_raw_sha256=sha(llvm),
                sidecar_without_verified_fingerprint_sha256=hashlib.sha256(material).hexdigest(), excluded_lines=1)

def attempt(record, log):
    data = load(log / 'batch-attempt.json')
    assert data['complete'] and data['error'] is None
    assert data['unfinished_at_cleanup'] == data['cleanup_rows'] == data['cleanup_errors'] == []
    assert data['reaped_sum_cpu_ns'] == record['sum_cpu_ns'] and data['cleanup_sum_cpu_ns'] == 0
    assert len(data['reaped_rows']) == len(record['candidates'])
    assert data['wall_so_far_ns'] >= record['wall_ns']
    for raw, row in zip(data['reaped_rows'], record['candidates']):
        assert all(row[key] == value for key, value in raw.items())
    counts['complete_batch_attempts'] += 1

def validate(child, directory, log, log_index, arm, fixture_key, cpus, jobs):
    assert child['exit'] == 0 and not child['errors'] and child['cpus'] == cpus
    assert child['argv'][0] == plan['tools'][arm]['path']
    assert child['batch_latency_ns'] == child['launch_latency_ns'] + child['launch_offset_ns']
    assert len(child['native']) == 3
    for native in child['native']:
        assert native['exit'] == 0 and native['stderr_hex'] == ''
        assert native['stdout_hex'] == native_expected[fixture_key][native['stdin_hex']]
        counts['native_IO_records'] += 1
    if arm == 'R':
        if fixture_key != 'n256/base':
            assert sha(directory / 'program') == child['artifact']['elf_sha256']
        assert all(flag in child['argv'] for flag in ['opt-level=2', 'overflow-checks=yes', 'panic=abort', 'codegen-units=8'])
        assert 'linker=' + plan['tools']['clang']['path'] in child['argv']
        counts['rust_compilations'] += 1
        return
    report = load(log / f'{log_index}.stdout')
    assert report['status'] == 'Success' and report['verification_runs'] == 1 and len(report['source_files']) == 256
    counts['current_ZIL_Verify'] += 1
    native = report['native_codegen']
    assert native['backend'] == 'llvm-api' and native['actual_units'] == 8 and native['jobs'] == jobs
    assert child['argv'][child['argv'].index('--codegen-jobs') + 1] == str(jobs)
    assert child['argv'][child['argv'].index('--clang') + 1] == plan['tools']['clang']['path']
    for unit in native['object_reports']:
        assert unit['preflight_ns'] > 0 and unit['pipeline'] == 'default<O2>|merge-functions=after|aliases=false' and unit['llvm_version'] == '23.1.4'
        if unit['input_kind'] == 'object-cache':
            assert unit['producer_verified_before_after'] is True
        else:
            assert unit['verify_before_after'] is True
        if unit['input_kind'] == 'object-patch':
            assert unit['current_full_machine_preflight'] is True
        kinds[unit['input_kind']] += 1
        counts['current_full_preflight_units'] += 1
    # Seed artifacts are overwritten by final and cannot be independently rehashed.
    if fixture_key != 'n256/base':
        assert artifact(directory / 'program', directory / 'program.ll') == child['artifact']
        counts['ZIL_artifact_files_rehashed'] += 2

for batch in rows:
    n, k, c, b, p, arm = (batch[key] for key in ['files', 'edited_files', 'concurrency', 'block', 'position', 'arm'])
    cell = root / f'n{n}-k{k}-c{c}-b{b}-p{p}-{arm}'
    assert load(cell / 'batch.json') == batch
    raw = load(cell / 'batch-raw.json')
    assert all(batch[key] == value for key, value in raw.items() if key != 'candidates')
    for before, after in zip(raw['candidates'], batch['candidates']):
        assert all(after[key] == value for key, value in before.items())
    attempt(batch, cell / 'final-log')
    assert batch['sum_cpu_ns'] == sum(child['cpu_ns'] for child in batch['candidates'])
    jobs = 4 // c
    slices = [plan['cpus'][i * jobs:(i + 1) * jobs] for i in range(c)]
    assert len(batch['candidates']) == c and batch['jobs_per_candidate'] == jobs
    assert sorted(x for subset in slices for x in subset) == [0, 1, 2, 3]
    for index, child in enumerate(batch['candidates']):
        assert child['candidate'] == index + 1
        assert child['batch_latency_ns'] <= batch['wall_ns']
        item = cell / f'candidate{index + 1}'
        source, build = item / 'source', item / 'build'
        assert str(source) not in used_paths
        used_paths.add(str(source))
        ext = 'rs' if arm == 'R' else 'zil'
        rel = f'n{n}/edit{k}/candidate{index + 1}'
        fixture = plan['fixtures'][rel]
        assert {f.name: sha(f) for f in source.glob('*.' + ext)} == {name: digest for name, digest in fixture['source_hashes'].items() if name.endswith('.' + ext)}
        validate(child, build, cell / 'final-log', index, arm, rel, slices[index], jobs)
        clean, seed = load(item / 'oracle.json'), load(item / 'seed.json')
        assert clean['logical_candidate'] == seed['logical_candidate'] == index + 1
        for record, phase, directory, key in [(clean, 'oracle', item / 'oracle', rel), (seed, 'seed', build, 'n256/base')]:
            before = load(item / (phase + '-raw.json'))
            assert all(record[name] == value for name, value in before.items() if name != 'candidates')
            for old, new in zip(before['candidates'], record['candidates']):
                assert all(new[name] == value for name, value in old.items())
            attempt(record, item / (phase + '-log'))
            expected_arm = ('R' if arm == 'R' else 'A') if phase == 'oracle' else arm
            validate(record['candidates'][0], directory, item / (phase + '-log'), 0, expected_arm, key, slices[index], jobs)
            counts[phase + '_compilations'] += 1
        assert child['clean_oracle_identity'] == clean['candidates'][0]['artifact']
        same = child['artifact']['elf_sha256'] == clean['candidates'][0]['artifact']['elf_sha256']
        assert child['elf_equal_to_clean'] == same
        if arm != 'R':
            assert same
            assert child['artifact']['sidecar_without_verified_fingerprint_sha256'] == clean['candidates'][0]['artifact']['sidecar_without_verified_fingerprint_sha256']
            counts['ZIL_final_same_ELF_and_material_IR_to_own_binary_clean'] += 1
        else:
            counts['Rust_final_ELF_equal_to_clean'] += int(same)
            counts['Rust_final_ELF_different_from_clean'] += int(not same)
        counts['final_children'] += 1
    counts['final_batches'] += 1

assert counts['final_batches'] == 36 and counts['final_children'] == 84
assert counts['oracle_compilations'] == counts['seed_compilations'] == 84
assert counts['current_ZIL_Verify'] == 168 and counts['rust_compilations'] == 84
assert counts['current_full_preflight_units'] == 1344 and counts['native_IO_records'] == 756
assert counts['complete_batch_attempts'] == 204

cells = []
for k in [16, 64]:
    for c in [1, 2, 4]:
        arms = {}
        for arm in ['A', 'B', 'R', 'current-main']:
            group = [r for r in rows if r['edited_files'] == k and r['concurrency'] == c and (r['arm'] in ['A', 'B'] if arm == 'current-main' else r['arm'] == arm)]
            wall = [r['wall_ns'] / 1e6 for r in group]
            cpu = [r['sum_cpu_ns'] / 1e6 for r in group]
            launch = [x['launch_latency_ns'] / 1e6 for r in group for x in r['candidates']]
            latency = [x['batch_latency_ns'] / 1e6 for r in group for x in r['candidates']]
            arms[arm] = dict(batches=len(group), tasks=len(group) * c, batch_wall_ms=wall,
                             mean_batch_wall_ms=statistics.mean(wall), mean_sum_cpu_ms=statistics.mean(cpu), sum_cpu_ms=cpu,
                             mean_task_launch_to_reap_ms=statistics.mean(launch), task_launch_to_reap_ms=launch,
                             mean_task_batch_start_to_reap_ms=statistics.mean(latency), task_batch_start_to_reap_ms=latency,
                             throughput_tasks_per_second=len(group) * c / (sum(wall) / 1000),
                             RSS_individual_process_kib=[x['individual_process_rss_kib'] for r in group for x in r['candidates']])
        cells.append(dict(files=256, edited_files=k, concurrency=c, jobs_per_task=4 // c, arms=arms,
                          AA_B_over_A=arms['B']['mean_batch_wall_ms'] / arms['A']['mean_batch_wall_ms'],
                          AA_drift_percent=(arms['B']['mean_batch_wall_ms'] / arms['A']['mean_batch_wall_ms'] - 1) * 100,
                          current_main_over_Rust_wall=arms['current-main']['mean_batch_wall_ms'] / arms['R']['mean_batch_wall_ms']))
for original in load(root / 'cells.json'):
    recomputed = next(c for c in cells if c['edited_files'] == original['edited_files'] and c['concurrency'] == original['concurrency'])['arms'][original['arm']]
    assert original['batch_wall_ns'] == [int(round(v * 1e6)) for v in recomputed['batch_wall_ms']]
    group = [r for r in rows if (r['files'], r['edited_files'], r['concurrency'], r['arm']) == (original['files'], original['edited_files'], original['concurrency'], original['arm'])]
    assert original['sum_cpu_ns'] == [r['sum_cpu_ns'] for r in group]
    assert original['candidate_batch_latency_ns'] == [[c['batch_latency_ns'] for c in r['candidates']] for r in group]
    assert original['individual_process_rss_kib'] == [[c['individual_process_rss_kib'] for c in r['candidates']] for r in group]
cross_lane_pairs = 0
for a in rows:
    if a['arm'] != 'A':
        continue
    for b in rows:
        if b['arm'] != 'B' or (a['edited_files'], a['concurrency']) != (b['edited_files'], b['concurrency']):
            continue
        for x, y in zip(a['candidates'], b['candidates']):
            for key in ['elf_sha256', 'sidecar_without_verified_fingerprint_sha256']:
                assert x['artifact'][key] == y['artifact'][key]
            cross_lane_pairs += 1
assert cross_lane_pairs == 56
result = dict(audit_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), scope='Unchanged main A/A plus Rust, no optimization candidate',
              protocol_sha256=sha(protocol_path), batches_sha256=sha(root / 'batches.jsonl'), completion_sha256=sha(root / 'completion.json'),
              A_and_B_exact_same_binary_and_source=True, counts=dict(counts), unit_receipt_kinds=dict(kinds),
              all_frozen_tools_helpers_sources_verified=True, all_recorded_native_IO_matches_independent_oracle=True,
              all_ZIL_final_artifacts_match_own_binary_clean=True, all_attempts_complete_no_cleanup=True,
              cross_lane_same_source_ELF_material_pairs=56, no_compiler_program_or_sampler_executed=True, cells=cells,
              limits=['A and B are identical-binary repetition controls; their difference is not an optimization.',
                      'c=1/2/4 changes work count under fixed total four CPUs; batch walls do not compare equal work.',
                      'Throughput is total completed tasks divided by summed batch wall, not mean of individual reciprocal times.',
                      'RSS is wait4 individual-process high-water observation, not aggregate concurrent peak; launcher/fork high-water floor may affect it.',
                      'Rust clean vs incremental ELF differences are observed and allowed by the frozen protocol, not silently reported equal.',
                      'Native IO is captured in JSON records; separate native invocation stdout files were not produced by this protocol.',
                      'Seed binaries and source bytes were overwritten by final; original records verified but those past artifacts cannot be rehashed.',
                      'One block: four pooled main batches and two Rust batches per cell; no causal or general workload claim.'])
(base / 'main-concurrency-result-independent-audit.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({k: v for k, v in result.items() if k not in ['cells', 'limits']}, indent=2))
print(json.dumps([dict(k=c['edited_files'], c=c['concurrency'], A=c['arms']['A']['mean_batch_wall_ms'], B=c['arms']['B']['mean_batch_wall_ms'],
                      main=c['arms']['current-main']['mean_batch_wall_ms'], Rust=c['arms']['R']['mean_batch_wall_ms'],
                      main_latency=c['arms']['current-main']['mean_task_launch_to_reap_ms'], Rust_latency=c['arms']['R']['mean_task_launch_to_reap_ms'],
                      main_throughput=c['arms']['current-main']['throughput_tasks_per_second'], Rust_throughput=c['arms']['R']['throughput_tasks_per_second'],
                      main_cpu=c['arms']['current-main']['mean_sum_cpu_ms'], Rust_cpu=c['arms']['R']['mean_sum_cpu_ms'], AA_drift=c['AA_drift_percent']) for c in cells], indent=2))
