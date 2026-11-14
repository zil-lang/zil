from pathlib import Path
import subprocess, json, time, hashlib, datetime, sys

root = Path('/workspace/zil-push-main')
toolrepo = Path('/workspace/zil-measure-ad814')
stock = '30c0009ff94b3476c15f4afe2ba5d067f7f78cbb'
candidate = '1de00f63f80cf4534ec995dc51e106005b270ddb'
main = '9170bb4210554392e36eb7528596612596ebef80'
done = root/('out/runs/large-source-native-release-17-' + candidate[:12] + '/completion.json')
while not done.exists():
    time.sleep(3)
assert json.loads(done.read_text())['all_gates_valid'], 'current immutable release gate failed'
import shutil
binary = root/'out/binaries'/candidate/'zil'
binary.parent.mkdir(parents=True, exist_ok=False)
shutil.copy2('/workspace/zil-target-closed-body/release/zil', binary)
restored = json.loads(Path('/workspace/zil-recovery/large-project-pain-fresh-01.log').read_text().splitlines()[-1])
assert restored['status'] == 'verified' and restored['sha256'] == 'ead43347eee5bd9b65d5abf9d1bc66eb1b5a55a5b87db7cfe33c4de03a00d3a6'
sha = lambda p: hashlib.file_digest(open(p, 'rb'), 'sha256').hexdigest()
load = lambda p: json.loads(Path(p).read_text())
save = lambda p, v: Path(p).write_text(json.dumps(v, indent=2) + '\n')
control = root/'out/runs/main9170-host-release-control-01/release.json'
baseline = load(control)
assert baseline['exit_code'] == 0 and baseline['source'] == main
basepins = load(root/'out/runs/large-project-stock-pins-01.json')
basepins['baseline'].update(binary=baseline['binary'], binary_sha256=baseline['sha256'], build_receipt=str(control))
receipts = {
    'stock': root/('out/runs/compiler-integration-direct-ssa-04-' + stock[:12] + '/release.json'),
    'push': root/('out/runs/large-source-native-release-17-' + candidate[:12] + '/release.json'),
}
pinpaths = {}
for policy, source, worktree, lib in [
    ('stock', stock, '/workspace/zil-compiler-integration', '/workspace/zil-recovery/baseline'),
    ('push', candidate, '/workspace/zil-research-measure-source1de', '/workspace/.zil-environment/llvm-function-products-v18/lib'),
]:
    pins = json.loads(json.dumps(basepins))
    receipt = load(receipts[policy])
    assert receipt['source'] == source and receipt['exit'] == 0 and receipt['valid']
    binary = root/'out/binaries'/source/'zil'
    digest = sha(binary)
    assert digest == receipt.get('binary_sha256', receipt.get('sha256'))
    adapter = root/('out/runs/large-project-current-' + policy + '-release-adapter-01.json')
    receipt.update(sha256=digest, exit_code=0, binary=str(binary),
                   original_receipt=str(receipts[policy]), original_receipt_sha256=sha(receipts[policy]),
                   adaptation='Schema fields only; actual source/tree/command/binary retained.')
    save(adapter, receipt)
    pins['candidate'].update(source=source, worktree=worktree, binary=str(binary), binary_sha256=digest,
                             build_receipt=str(adapter), llvm_library_path=lib,
                             llvm_dso_sha256=sha(Path(lib)/'libLLVM.so.23.1'))
    pinpath = root/('out/runs/large-project-current-' + policy + '-pins-01.json')
    save(pinpath, pins)
    pinpaths[policy] = pinpath

summary = []
def run(label, command, out):
    assert subprocess.check_output(['git','ls-remote','origin','refs/heads/main'], cwd=root, text=True).split()[0] == main
    print(label, 'START', datetime.datetime.now(datetime.timezone.utc).isoformat(), flush=True)
    with Path('/workspace/zil-recovery/' + label + '.log').open('x') as log:
        result = subprocess.run(command, cwd=toolrepo, stdout=log, stderr=subprocess.STDOUT)
    summary.append(dict(label=label, command=command, exit=result.returncode, out=str(out)))
    save(root/'out/runs/large-project-current-batch-02-status.json', summary)
    print(label, 'EXIT', result.returncode, flush=True)

capacity = root/('out/runs/large-project-original5105-current-01-' + candidate[:12])
run('large-project-original5105-current-01', [sys.executable,'-B','/workspace/zil-recovery/large-capacity-current.py',
    '--candidate',str(root/'out/binaries'/candidate/'zil'),'--source',candidate,'--out',str(capacity),'--repo',str(toolrepo)], capacity)

runs = [
    ('large2500', 'push', '2500','simple','wide',8,'check'),
    ('large2500', 'push', '2500','simple','wide',8,'o2'),
    ('large1000-edits', 'push', '1000','short','wide',32,'check'),
    ('large1000-edits', 'push', '1000','short','wide',32,'o2'),
    ('stock-large2500', 'stock', '2500','simple','wide',8,'check'),
    ('stock-large2500', 'stock', '2500','simple','wide',8,'o2'),
    ('stock-large1000', 'stock', '1000','short','wide',8,'o2'),
]
for label, policy, preset, shape, graph, epochs, mode in runs:
    label = 'large-project-' + label + '-02-' + mode
    out = root/'out/runs'/label
    command = [sys.executable,'-B',str(toolrepo/'tools/research/incremental-demand/large_project_matrix.py'),
        'measure','--repo',str(toolrepo),'--out',str(out),'--preset',preset,'--shape',shape,'--graph',graph,
        '--epochs',str(epochs),'--blocks','2','--mode',mode,'--candidate-policy',policy,'--pins',str(pinpaths[policy]),
        '--cargo','/workspace/toolchains/cargo/bin/cargo','--rustc','/workspace/toolchains/cargo/bin/rustc']
    run(label, command, out)
print('DONE', flush=True)
