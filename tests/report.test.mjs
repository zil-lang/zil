import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {test} from 'node:test';

const root = fileURLToPath(new URL('../', import.meta.url));
const helper = new URL('./report.mjs', import.meta.url).href;
function fixture(t) {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'zil-report-test-'));
    t.after(() => fs.rmSync(directory, {recursive: true, force: true}));
    let serial = 0;
    return {directory, run(program, args, extra = {}) {
        const output = path.join(directory, `stdout-${serial}`), errors = path.join(directory, `stderr-${serial++}`);
        const handles = [fs.openSync(output, 'wx'), fs.openSync(errors, 'wx')];
        const env = {...process.env}; delete env.ZIL_TEST_REPORT_DIR; delete env.ZIL_HANDOFF_REPORT;
        // These are ordinary CLI probes, not child nodes in the test runner's
        // private reporting channel.
        delete env.NODE_TEST_CONTEXT;
        let result;
        try { result = spawnSync(program, args, {cwd: root, env: {...env, ...extra}, stdio: ['ignore', ...handles], timeout: 15000}); }
        finally { handles.forEach(fd => fs.closeSync(fd)); }
        if (result.error) throw result.error;
        return {status: result.status, stdout: fs.readFileSync(output, 'utf8'), stderr: fs.readFileSync(errors, 'utf8')};
    }};
}
const script = `import {writeReport} from ${JSON.stringify(helper)}; console.log(JSON.stringify({file:writeReport('fixture.json',{status:'Fail',executed:3},{file:process.env.ZIL_HANDOFF_REPORT})}));`;

test('default reports use unique ignored run directories and preserve verdict values', t => {
    const f = fixture(t), outputs = [];
    for (let i = 0; i < 2; i++) {
        const result = f.run(process.execPath, ['--input-type=module', '-e', script]);
        assert.equal(result.status, 0, result.stderr);
        const file = JSON.parse(result.stdout).file;
        assert.ok(file.startsWith(path.join(root, 'out/runs/tool-reports') + path.sep));
        assert.deepEqual(JSON.parse(fs.readFileSync(file)), {status: 'Fail', executed: 3});
        outputs.push(file);
    }
    assert.notEqual(outputs[0], outputs[1]);
});

test('configured directory refuses overwrite and handoff-specific file still wins', t => {
    const f = fixture(t), directory = path.join(f.directory, 'configured');
    const first = f.run(process.execPath, ['--input-type=module', '-e', script], {ZIL_TEST_REPORT_DIR: directory});
    assert.equal(first.status, 0, first.stderr);
    const file = JSON.parse(first.stdout).file;
    assert.equal(file, path.join(directory, 'fixture.json'));
    const bytes = fs.readFileSync(file);
    const duplicate = f.run(process.execPath, ['--input-type=module', '-e', script], {ZIL_TEST_REPORT_DIR: directory});
    assert.notEqual(duplicate.status, 0);
    assert.match(duplicate.stderr, /EEXIST/);
    assert.deepEqual(fs.readFileSync(file), bytes);
    const explicit = path.join(f.directory, 'handoff', 'exact.json');
    const chosen = f.run(process.execPath, ['--input-type=module', '-e', script], {ZIL_TEST_REPORT_DIR: directory, ZIL_HANDOFF_REPORT: explicit});
    assert.equal(chosen.status, 0, chosen.stderr);
    assert.equal(JSON.parse(chosen.stdout).file, explicit);
});

test('actual model runner keeps its 23 anchors and leaves historical receipt unchanged', t => {
    const f = fixture(t), historical = path.join(root, 'evidence/impl/004/model-check.json');
    const before = fs.readFileSync(historical);
    const directory = path.join(f.directory, 'model');
    const result = f.run(process.execPath, ['--experimental-transform-types', '--import', './tools/roam/register.mjs', 'tests/oracle/model-check.mjs'], {ZIL_TEST_REPORT_DIR: directory});
    assert.equal(result.status, 0, result.stderr);
    const report = JSON.parse(result.stdout);
    assert.equal(report.status, 'Pass'); assert.equal(report.executed, 23); assert.equal(report.assertions, 26);
    assert.equal(report.report_path, path.join(directory, 'model-check.json'));
    assert.deepEqual(fs.readFileSync(historical), before);
});

test('explicit summary publication preserves raw failure and rejects receipt reuse', t => {
    const f = fixture(t);
    const code = String.raw`
import importlib.util,json,pathlib,sys
spec=importlib.util.spec_from_file_location('draft',pathlib.Path('tools/validate-draft.py'))
draft=importlib.util.module_from_spec(spec);spec.loader.exec_module(draft)
root=pathlib.Path(sys.argv[1])/'summary-fixture';raw=root/'out/runs/validation/fixture/result.json';raw.parent.mkdir(parents=True)
report={'run_id':'fixture','source_commit':'synthetic-fixture','draft_gate':'Fail','full_language_gate':'NotSatisfied','product_gate':'NotRun','required_checks':23,'required_passed':21,'result_path':'out/runs/validation/fixture/result.json'}
raw.write_text(json.dumps(report));before=raw.read_bytes()
assert not (root/'evidence/final-summary.json').exists()
published=draft.publish_summary(root,report)
assert raw.read_bytes()==before and published['draft_gate']=='Fail' and published['required_passed']==21
assert published['raw_result_path']==report['result_path'] and published['result_path']=='evidence/validation/fixture/result.json'
assert json.loads((root/'evidence/final-summary.json').read_text())==published
pointer=(root/'evidence/final-summary.json').read_bytes()
try:draft.publish_summary(root,report)
except FileExistsError:pass
else:raise AssertionError('existing receipt overwritten')
assert (root/'evidence/final-summary.json').read_bytes()==pointer
try:draft.publish_summary(root,{**report,'run_id':'../escape'})
except ValueError:pass
else:raise AssertionError('unsafe RunId accepted')
print(json.dumps({'status':'Pass','scope':'Publication fixture only; no gate execution','raw_unchanged':True,'failure_preserved':True}))
`;
    const result = f.run(process.env.PYTHON ?? 'python3', ['-c', code, f.directory]);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(JSON.parse(result.stdout).status, 'Pass');
});


test('conflicting legacy raw root and summary publication reject before a run starts', t => {
    const f = fixture(t), rootPath = path.join(root, 'evidence/validation');
    const before = fs.readdirSync(rootPath).sort();
    const result = f.run(process.env.PYTHON ?? 'python3', ['tools/validate-draft.py', '--out-root', 'evidence/validation', '--publish-summary']);
    assert.equal(result.status, 2);
    assert.match(result.stderr, /raw output root distinct/);
    assert.equal(result.stdout, '');
    assert.deepEqual(fs.readdirSync(rootPath).sort(), before);
});
