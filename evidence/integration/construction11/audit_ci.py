from pathlib import Path
import argparse, hashlib, json, re, subprocess

p = argparse.ArgumentParser()
p.add_argument('directory', type=Path)
p.add_argument('--run', type=int, required=True)
p.add_argument('--expected-head', required=True)
p.add_argument('--repo', type=Path, required=True)
p.add_argument('--candidate', required=True)
p.add_argument('--require-success', action='store_true')
a = p.parse_args()
root = a.directory
receipts = root / 'final' / 'receipts'
jobs = json.loads((root / 'jobs.json').read_text())['jobs']
assert len(jobs) == 1
job = jobs[0]
assert job['run_id'] == a.run and job['head_sha'] == a.expected_head
checkout = (receipts / 'commit.txt').read_text().strip()
assert re.fullmatch('[0-9a-f]{40}', checkout)

def counts(path):
    text = path.read_text()
    rows = [tuple(map(int, m)) for m in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;', text)]
    assert rows
    return dict(groups=len(rows), **{name: sum(row[i] for row in rows)
                for i, name in enumerate(['passed', 'failed', 'ignored', 'measured', 'filtered'])})

def tree(text):
    result = {}
    for line in text.splitlines():
        meta, name = line.split('\t', 1)
        mode, kind, sha = meta.split()
        result[name] = [mode, kind, sha]
    return result

observed = tree((receipts / 'source-tree.txt').read_text())
candidate = tree(subprocess.check_output(['git', '-C', str(a.repo), 'ls-tree', '-r', a.candidate], text=True))
changed = sorted(k for k in set(observed) | set(candidate) if observed.get(k) != candidate.get(k))
allowed_exact = {
    '_zil_goal.md', 'data/handoff.json', 'data/construction-progress.json',
    'docs/CURRENT-HANDOFF.md', 'docs/CONSTRUCTION-PROGRESS.md',
    'docs/LANGUAGE-CONSTRUCTION11.md', 'docs/RUST-GOALS.md', 'docs/IMPLEMENTATION-STATUS.md',
}
def metadata_only(name):
    return name.startswith(('evidence/', '.task/impl/')) or name in allowed_exact
assert all(metadata_only(name) for name in changed), changed

consumer = json.loads((receipts / 'consumer209.json').read_text())
assert consumer['status'] == 'Pass' and consumer['source_commit'] == checkout
builds = consumer['builds']; observations = consumer['observations']
assert all(b['verification_runs'] == 1 and b['codegen']['backend'] == 'llvm-api'
           and b['codegen']['fallback'] is None
           and all(o['llvm_version'] == '23.1.4' and o['input_kind'] == 'builder'
                   and o['parse_ns'] == 0 and o['verify_before_after'] is True
                   for o in b['codegen']['object_reports']) for b in builds)
allocation = [o['allocation'] for o in observations if o['allocation'] is not None]
assert all(x['live'] == 0 and x['allocated'] == x['frees'] for x in allocation)
assert len({(o['case'], o['fail_at']) for o in observations}) == len(observations)

all_steps_success = all(s['status'] == 'completed' and s['conclusion'] == 'success' for s in job['steps'])
migration_text = (receipts / 'migration-runner.log').read_text()
migration_count = re.search(r'Ran (\d+) tests? in ', migration_text)
assert migration_count and re.search(r'^OK\s*$', migration_text, re.MULTILINE)
summary = {
    'schema_version': 1,
    'status': 'Pass' if job['conclusion'] == 'success' and all_steps_success else 'Fail',
    'run_id': a.run, 'run_attempt': job['run_attempt'], 'job_id': job['id'],
    'head_candidate': a.expected_head, 'actual_checkout': checkout,
    'compared_candidate': a.candidate, 'metadata_only_tree_difference': changed,
    'compiler_test_tool_and_normative_inputs_identical': True,
    'workspace': counts(receipts / 'workspace.log'),
    'reference': counts(receipts / 'reference.log'),
    'text_reference': counts(receipts / 'text-reference.log'),
    'migration_runner': {'status': 'Pass', 'passed': int(migration_count[1])},
    'all_required_steps_success': all_steps_success,
    'successful_step_count': sum(s['conclusion'] == 'success' for s in job['steps']),
    'consumer209': {
        'status': consumer['status'], 'builds': len(builds),
        'objects': sum(len(b['codegen']['object_reports']) for b in builds),
        'observations': len(observations), 'allocation_observations': len(allocation),
        'actual_failure_injections': sum(x['failures'] for x in allocation),
        'all_observed_live_zero': True, 'backend': 'llvm-api', 'llvm_version': '23.1.4',
        'fallback': None,
    },
    'receipt_hashes': {str(f.relative_to(receipts)): {'bytes': f.stat().st_size,
                       'sha256': hashlib.sha256(f.read_bytes()).hexdigest()}
                       for f in sorted(receipts.iterdir()) if f.is_file() and f.name != 'repository.bundle'},
    'boundaries': {'complete_language': 'NotSatisfied', 'product_gates': 'NotRun',
                   'native_Windows': 'NotRun', 'default_and_reference_counts_not_added': True},
}
if a.require_success:
    assert summary['status'] == 'Pass', summary
    assert all(summary[k]['passed'] > 0 and summary[k]['failed'] == 0
               and summary[k]['ignored'] == 0 and summary[k]['filtered'] == 0
               for k in ['workspace', 'reference', 'text_reference'])
(root / 'AUDIT.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps({k: v for k, v in summary.items() if k != 'receipt_hashes'}, indent=2))
