from pathlib import Path
import hashlib, json, os, subprocess, tempfile

sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
plan_path = Path('/workspace/scratch/ac5966c4c370/runs/rust-o0-cold-e8-plan/plan.json')
plan = json.loads(plan_path.read_text())
base = plan_path.parent
case = next(c for c in plan['cases'] if c['id'] == 'tree1024x8')
binary = Path('/workspace/scratch/ac5966c4c370/binaries/cp12-397bf24fe')
source = base / case['root'] / 'base/zil/main.zil'
work = Path(tempfile.mkdtemp(prefix='cp12-397bf24-tree1024x8-'))
argv = [str(binary), 'build', str(source), '-o', str(work/'program'), '--backend', 'llvm-api',
        '--clang', os.environ['ZIL_CLANG'], '--opt', 'O0', '--codegen-units', '8', '--codegen-jobs', '2',
        '--query-cache', str(work/'queries'), '--object-cache', str(work/'objects'), '--json']
receipt = {'source_commit': '397bf24fef0d7f58aaefb444c17c733652b5e12f', 'binary': str(binary),
           'binary_sha256': sha(binary), 'oracle_plan': str(plan_path), 'oracle_plan_sha256': sha(plan_path),
           'case': 'tree1024x8', 'source': str(source), 'source_files': {
               str(p.relative_to(source.parent)): sha(p) for p in sorted(source.parent.rglob('*.zil'))},
           'work': str(work), 'argv': argv, 'limits': 'all eleven compiler defaults, unmodified',
           'sample_kind': 'functional-smoke-unscored', 'concurrent_builds_possible': True,
           'builds': []}
for index in range(2):
    with (work/f'build-{index}.json').open('wb') as out, (work/f'build-{index}.stderr').open('wb') as err:
        build = subprocess.run(argv, stdout=out, stderr=err, timeout=180)
    report = json.loads((work/f'build-{index}.json').read_text())
    row = {'request': 'cold' if index == 0 else 'unchanged', 'exit': build.returncode,
           'status': report['status'], 'verification_runs': report['verification_runs'],
           'query_cache': report['query_cache'], 'native_codegen': report.get('native_codegen'),
           'report_sha256': sha(work/f'build-{index}.json'), 'stderr_sha256': sha(work/f'build-{index}.stderr'),
           'oracles': []}
    if build.returncode == 0:
        samples = [(s['path'], s['expected']['base']) for s in case['inputs']]
        samples.append((case['input'],case['expected']))
        for n,(input_path,expected_path) in enumerate(samples):
            with (base/input_path).open('rb') as input_file:
                result = subprocess.run([str(work/'program')], stdin=input_file, capture_output=True, timeout=30)
            (work/f'run-{index}-{n}.stdout').write_bytes(result.stdout)
            (work/f'run-{index}-{n}.stderr').write_bytes(result.stderr)
            row['oracles'].append({'input':input_path,'expected':expected_path,'exit':result.returncode,
                  'input_sha256':sha(base/input_path),'expected_sha256':sha(base/expected_path),
                  'stdout_sha256':sha(work/f'run-{index}-{n}.stdout'),'stderr_sha256':sha(work/f'run-{index}-{n}.stderr'),
                  'pass':result.returncode==0 and result.stdout==(base/expected_path).read_bytes() and not result.stderr})
    receipt['builds'].append(row)
    print(json.dumps({'work':str(work),'build':index,'status':row['status'],
                     'verification_runs':row['verification_runs'],'query_cache':row['query_cache'],
                     'object_cache':(row['native_codegen'] or {}).get('object_cache'),
                     'oracles':[(o['input'],o['pass']) for o in row['oracles']]}),flush=True)
receipt['checkpoints'] = []
for p in sorted((work/'queries').rglob('*.qcp')):
    data=p.read_bytes()
    receipt['checkpoints'].append({'path':str(p),'bytes':len(data),'payload_bytes':int.from_bytes(data[72:80],'little'),
             'sha256':sha(p),'payload_sha256':hashlib.sha256(data[80:]).hexdigest(),
             'payload_checksum_matches':hashlib.sha256(data[80:]).digest()==data[40:72]})
receipt['pass'] = (len(receipt['builds'])==2 and all(r['status']=='Success' and r['verification_runs']==1
     and len(r['oracles'])==4 and all(o['pass'] for o in r['oracles']) for r in receipt['builds'])
     and receipt['builds'][0]['query_cache']['state']=='miss' and receipt['builds'][0]['query_cache']['stored']
     and receipt['builds'][1]['query_cache']['state']=='hit' and len(receipt['checkpoints'])==1)
(work/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
Path('/tmp/cp12-397bf24-release-receipt/smoke-path.txt').write_text(str(work)+'\n')
print(json.dumps({'pass':receipt['pass'],'receipt':str(work/'receipt.json'),'checkpoints':receipt['checkpoints']}),flush=True)
