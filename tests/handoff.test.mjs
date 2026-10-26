import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import childProcess from 'node:child_process';
import {syncBuiltinESMExports} from 'node:module';
import {fileURLToPath} from 'node:url';
import {test, after} from 'node:test';
import {inspectHandoff} from '../tools/handoff.mjs';
import {writeReport} from './report.mjs';

const project = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const script = path.join(project, 'tools/handoff.mjs');
const results = [];
let assertions = 0;
const check = (actual, expected = true) => { assertions++; assert.deepEqual(actual, expected); };
const sha = value => createHash('sha256').update(value).digest('hex');
function capture(command, args, options = {}) {
  const area = fs.mkdtempSync(path.join(os.tmpdir(), 'zil-handoff-capture-')), handles = [];
  try {
    for (const name of ['stdout', 'stderr']) handles.push(fs.openSync(path.join(area, name), 'w+'));
    const result = spawnSync(command, args, {...options, stdio: ['ignore', ...handles]});
    assert.equal(result.error, undefined, result.error?.message);
    return {...result, stdout: fs.readFileSync(path.join(area, 'stdout'), 'utf8'), stderr: fs.readFileSync(path.join(area, 'stderr'), 'utf8')};
  } finally {
    for (const handle of handles) fs.closeSync(handle);
    fs.rmSync(area, {recursive: true, force: true});
  }
}
function git(root, ...args) {
  const result = capture('git', ['-C', root, ...args], {timeout: 10000});
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}
function put(root, name, data) {
  const target = path.join(root, name);
  fs.mkdirSync(path.dirname(target), {recursive: true});
  fs.writeFileSync(target, typeof data === 'string' ? data : JSON.stringify(data, null, 2) + '\n');
}
function commit(root, message) { git(root, 'add', '.'); git(root, 'commit', '--quiet', '-m', message); return git(root, 'rev-parse', 'HEAD'); }
function fixture(t) {
  const area = fs.mkdtempSync(path.join(os.tmpdir(), 'zil-handoff-'));
  t.after(() => fs.rmSync(area, {recursive: true, force: true}));
  const root = path.join(area, 'main'); fs.mkdirSync(root);
  git(root, 'init', '--quiet', '--initial-branch=main');
  git(root, 'config', 'user.name', 'Handoff fixture'); git(root, 'config', 'user.email', 'fixture@example.invalid');
  const {spec_hash: specHash, normative_hash: norm} = writeSpecification(root, '# Fixture specification\n');
  const baseline = commit(root, 'fixture baseline');
  const formal = {id: 'B0', status: 'Doing', spec_hash: specHash, normative_hash: norm, base_revision: baseline, blocked_by: [], evidence: []};
  put(root, '.task/B0.md', '# B0 fixture\n\n```json\n' + JSON.stringify(formal) + '\n```\n');
  const task = {id: '003', parent_task: 'B0', status: '施工', baseline, spec_hash: specHash, normative_hash: norm, authoritative_task: '/previous/computer/zil/.task/impl/003.md', write_set: ['tools/handoff.mjs'], implementation_author: 'builder', implementation_commit: null, integration_commit: null, review: null, evidence: []};
  const save = () => put(root, '.task/impl/003.md', `${task.status}\n# 003 交接\n\n\`\`\`json\n${JSON.stringify(task, null, 2)}\n\`\`\`\n`);
  save(); commit(root, 'assign fixture');
  return {area, root, baseline, task, save};
}
function merged(f) {
  put(f.root, 'evidence/impl/003/build.json', {status: 'Pass', executed: 1, assertions: 1});
  f.task.implementation_commit = commit(f.root, 'implementation');
  put(f.root, 'evidence/impl/003/review.json', {verdict: 'Pass', reviewer: 'reviewer', implementation_commit: f.task.implementation_commit});
  commit(f.root, 'independent review');
  f.task.status = '已合入'; f.task.integration_commit = f.task.implementation_commit;
  f.task.evidence = ['evidence/impl/003/build.json'];
  f.task.review = {reviewer: 'reviewer', verdict: 'Pass', implementation_commit: f.task.implementation_commit, evidence: ['evidence/impl/003/review.json']};
  f.save(); commit(f.root, 'record integration');
}
function scenario(id, title, body) {
  test(`${id} ${title}`, t => {
    const before = assertions;
    try { body(t); results.push({id, title, status: 'Pass', assertions: assertions - before}); }
    catch (error) { results.push({id, title, status: 'Fail', assertions: assertions - before, reason: error.message}); throw error; }
  });
}
const hasError = (result, text) => result.errors.some(e => e.reason.includes(text));

scenario('handoff-001', '分树读取主树工单', t => {
  const f = fixture(t), child = path.join(f.area, '003');
  git(f.root, 'worktree', 'add', '--quiet', '-b', '003', child);
  put(child, '.task/impl/003.md', '错误副本\n');
  const result = inspectHandoff(child);
  check(result.status, 'Pass'); check(result.authority.current_is_main, false);
  check(result.slices[0].status, '施工'); check(result.runtime.main_tree, fs.realpathSync(f.root));
  check(result.next, [{id: '003', status: '施工', authoritative_task: '.task/impl/003.md'}]);
  const attempt = capture(process.execPath, [script], {cwd: child});
  check(attempt.status, 1); check(fs.existsSync(path.join(f.root, 'data/handoff.json')), false);
});

scenario('handoff-002', '缺基线与状态冲突拒绝', t => {
  const f = fixture(t);
  for (const bad of [null, 'a'.repeat(40)]) {
    f.task.baseline = bad; f.save(); check(hasError(inspectHandoff(f.root), 'baseline must identify'), true);
  }
  f.task.baseline = f.baseline; f.task.status = '任意状态'; f.save(); check(hasError(inspectHandoff(f.root), 'Unknown task state'));
  f.task.status = '施工'; f.save();
  const file = path.join(f.root, '.task/impl/003.md'); fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace(/^施工/, '已合入'));
  check(hasError(inspectHandoff(f.root), 'First-line state differs'));
});

scenario('handoff-003', '未跟踪证据不能交付', t => {
  const f = fixture(t); f.task.status = '待验'; f.task.implementation_commit = f.baseline;
  for (const refs of [[], ['evidence/absent.json'], ['../outside.json'], ['/tmp/outside.json']]) {
    f.task.evidence = refs; f.save(); check(inspectHandoff(f.root).status, 'Fail');
  }
  put(f.root, 'evidence/impl/003/new.json', {status: 'Pass'}); f.task.evidence = ['evidence/impl/003/new.json']; f.save();
  check(hasError(inspectHandoff(f.root), 'uncommitted'));
});

scenario('handoff-004', '合入必须独立审核', t => {
  const f = fixture(t); merged(f); check(inspectHandoff(f.root).status, 'Pass');
  const review = structuredClone(f.task.review);
  for (const bad of [null, {...review, reviewer: 'builder'}, {...review, reviewer: ' builder '}, {...review, verdict: 'Fail'}, {...review, implementation_commit: f.baseline}]) {
    f.task.review = bad; f.save(); check(hasError(inspectHandoff(f.root), 'independent passing review'));
  }
  f.task.review = {...review, evidence: []}; f.save(); check(hasError(inspectHandoff(f.root), 'Evidence references are missing'));
  f.task.review = review; f.task.integration_commit = 'b'.repeat(40); f.save(); check(hasError(inspectHandoff(f.root), 'integration_commit must identify'));
});

scenario('handoff-005', '证据失败不可冒充通过', t => {
  const f = fixture(t); merged(f);
  for (const value of [{status: 'Fail', executed: 1, assertions: 0}, {status: 'Fail', ok: true}, {status: 'Pass', executed: 0, assertions: 0}]) {
    put(f.root, 'evidence/impl/003/build.json', value); commit(f.root, 'record observed failure');
    check(hasError(inspectHandoff(f.root), 'Evidence has no passing outcome'));
  }
  f.task.review.evidence = ['evidence/impl/003/build.json']; f.save(); commit(f.root, 'build result is not review');
  check(hasError(inspectHandoff(f.root), 'No review evidence matches'));
  f.task.review.evidence = ['evidence/absent-review.json']; f.save(); commit(f.root, 'missing review reference');
  check(hasError(inspectHandoff(f.root), 'missing, empty, or uncommitted'));
});

scenario('handoff-006', '摘要可迁移且检查只读', t => {
  const f = fixture(t); merged(f);
  const created = capture(process.execPath, [script], {cwd: f.root});
  check(created.status, 0);
  const file = path.join(f.root, 'data/handoff.json'), bytes = fs.readFileSync(file, 'utf8');
  const snapshot = JSON.parse(bytes); check(snapshot.status, 'Pass'); check(bytes.includes(f.root), false);
  check(snapshot.measured, null); check(snapshot.snapshot_only);
  const checked = capture(process.execPath, [script, '--check'], {cwd: f.root});
  check(checked.status, 0); check(fs.readFileSync(file, 'utf8'), bytes);
  const bundle = path.join(f.area, 'source.bundle'), clone = path.join(f.area, 'elsewhere');
  git(f.root, 'bundle', 'create', bundle, 'main', 'HEAD'); git(f.area, 'clone', '--quiet', '--branch', 'main', bundle, clone);
  check(inspectHandoff(clone).status, 'Pass'); check(inspectHandoff(clone).runtime.main_tree, fs.realpathSync(clone));
});

scenario('handoff-007', '缺Git给出恢复命令', t => {
  const f = fixture(t), archive = path.join(f.area, 'archive');
  fs.cpSync(f.root, archive, {recursive: true, filter: file => path.basename(file) !== '.git'});
  const result = inspectHandoff(archive);
  check(result.status, 'Fail'); check(hasError(result, 'git clone --branch main HISTORY_BUNDLE_PATH zil-checkout'));
  const other = fixture(t); check(hasError(inspectHandoff(f.root, other.root), 'another Git repository'));
});

scenario('handoff-008', '进程状态零不能覆盖基础设施错误', t => {
  const f = fixture(t);
  const mocked = t.mock.method(childProcess, 'spawnSync', () => ({status: 0, stdout: f.root, stderr: '', error: Object.assign(new Error('spawnSync git EPERM'), {code: 'EPERM'})}));
  syncBuiltinESMExports();
  try {
    assertions++; assert.throws(() => inspectHandoff(f.root), /Git infrastructure failure \(EPERM\)/);
  } finally { mocked.mock.restore(); syncBuiltinESMExports(); }
});

scenario('handoff-009', '交付历史保留精确审核提交', t => {
  const f = fixture(t);
  git(f.root, 'switch', '--quiet', '-c', '003');
  put(f.root, 'evidence/impl/003/build.json', {status: 'Pass', executed: 1, assertions: 1});
  const implementation = commit(f.root, 'reviewed implementation');
  git(f.root, 'switch', '--quiet', 'main');
  put(f.root, 'main-progress.txt', 'main progressed\n'); commit(f.root, 'main progress');
  git(f.root, 'cherry-pick', implementation);
  f.task.status = '已合入'; f.task.implementation_commit = implementation; f.task.integration_commit = git(f.root, 'rev-parse', 'HEAD');
  f.task.evidence = ['evidence/impl/003/build.json'];
  f.task.review = {reviewer: 'reviewer', verdict: 'Pass', implementation_commit: implementation, evidence: ['evidence/impl/003/review.json']};
  put(f.root, 'evidence/impl/003/review.json', f.task.review);
  f.save(); commit(f.root, 'record cherry-picked integration');
  check(hasError(inspectHandoff(f.root), 'Implementation commit is not in main history'));
  git(f.root, 'merge', '--no-ff', '-m', 'retain reviewed commit', '003');
  check(inspectHandoff(f.root).status, 'Pass');
});

function writeSpecification(root, text) {
  put(root, '_zil_design.md', text);
  put(root, 'spec/grammar.ebnf', 'Module = ;\n');
  const required = ['_zil_design.md', 'spec/grammar.ebnf', 'spec/interfaces.json'];
  put(root, 'spec/interfaces.json', {normative_inputs: required});
  const files = Object.fromEntries(required.sort().map(name => [name, sha(fs.readFileSync(path.join(root, name)))]));
  const norm = sha(Object.entries(files).map(([name, hash]) => `${name}\0${hash}\n`).join(''));
  put(root, 'data/normative-inputs.json', {files, normative_hash: norm});
  return {spec_hash: files['_zil_design.md'], normative_hash: norm};
}

function advanceSpecification(f, text = '# Next specification\n') {
  const {spec_hash: spec, normative_hash: norm} = writeSpecification(f.root, text);
  const file = path.join(f.root, '.task/B0.md');
  const current = fs.readFileSync(file, 'utf8');
  const formal = JSON.parse(current.match(/```json\n([\s\S]*?)\n```/)[1]);
  formal.spec_hash = spec; formal.normative_hash = norm;
  put(f.root, '.task/B0.md', '# B0 fixture\n\n```json\n' + JSON.stringify(formal) + '\n```\n');
  commit(f.root, 'publish next specification');
  return {spec_hash: spec, normative_hash: norm};
}

scenario('handoff-010', '设计迭代保留精确历史规范', t => {
  const f = fixture(t); merged(f);
  const before = fs.readFileSync(path.join(f.root, '.task/impl/003.md'));
  const next = advanceSpecification(f);
  const result = inspectHandoff(f.root);
  check(result.status, 'Pass');
  check(result.slices[0].specification.source, 'implementation_commit');
  check(result.slices[0].specification.spec_hash, f.task.spec_hash);
  check(fs.readFileSync(path.join(f.root, '.task/impl/003.md')), before);
  Object.assign(f.task, next); f.save(); commit(f.root, 'incorrectly rewrite old task snapshot');
  check(hasError(inspectHandoff(f.root), 'differs from its implementation commit'));
});

scenario('handoff-011', '历史摘要不能替代实际规范输入', t => {
  for (const corruption of ['changed', 'missing', 'escape']) {
    const f = fixture(t);
    if (corruption === 'changed') put(f.root, '_zil_design.md', '# Changed without updating registry\n');
    else {
      const file = path.join(f.root, 'data/normative-inputs.json');
      const registry = JSON.parse(fs.readFileSync(file, 'utf8'));
      registry.files[corruption === 'missing' ? 'spec/missing.json' : '../outside.json'] = sha('missing');
      registry.normative_hash = sha(Object.entries(registry.files).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([name, hash]) => `${name}\0${hash}\n`).join(''));
      put(f.root, 'data/normative-inputs.json', registry);
      f.task.normative_hash = registry.normative_hash;
    }
    merged(f); advanceSpecification(f);
    const result = inspectHandoff(f.root);
    check(result.status, 'Fail');
    check(hasError(result, corruption === 'escape' ? 'Historical normative input is invalid' : 'Historical normative input does not match'));
  }
});

scenario('handoff-012', '在途工单仍锁当前规范', t => {
  const f = fixture(t), next = advanceSpecification(f);
  check(hasError(inspectHandoff(f.root), 'Task specification snapshot is stale'));
  Object.assign(f.task, next); f.save(); commit(f.root, 'explicitly migrate active slice');
  const result = inspectHandoff(f.root);
  check(result.status, 'Pass'); check(result.slices[0].specification.source, 'current');
  put(f.root, '_zil_design.md', '# Unregistered working change\n');
  check(hasError(inspectHandoff(f.root), 'Changed or missing input'));
});

scenario('handoff-013', '历史注册集合必须完整且合法', t => {
  for (const corruption of ['omission', 'duplicate', 'escape', 'missing-contract', 'missing-design']) {
    const f = fixture(t);
    const file = path.join(f.root, 'data/normative-inputs.json');
    const registry = JSON.parse(fs.readFileSync(file, 'utf8'));
    if (corruption === 'omission') delete registry.files['spec/grammar.ebnf'];
    else if (corruption === 'missing-contract') fs.rmSync(path.join(f.root, 'spec/interfaces.json'));
    else {
      const contract = {normative_inputs: ['_zil_design.md', 'spec/grammar.ebnf', 'spec/interfaces.json']};
      if (corruption === 'duplicate') contract.normative_inputs.push('spec/grammar.ebnf');
      if (corruption === 'escape') contract.normative_inputs.push('../outside.json');
      if (corruption === 'missing-design') contract.normative_inputs.shift();
      put(f.root, 'spec/interfaces.json', contract);
      registry.files['spec/interfaces.json'] = sha(fs.readFileSync(path.join(f.root, 'spec/interfaces.json')));
    }
    registry.normative_hash = sha(Object.entries(registry.files).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([name, hash]) => `${name}\0${hash}\n`).join(''));
    put(f.root, 'data/normative-inputs.json', registry);
    f.task.normative_hash = registry.normative_hash;
    merged(f); advanceSpecification(f);
    const result = inspectHandoff(f.root);
    check(result.status, 'Fail');
    check(hasError(result, corruption === 'omission' ? 'registry differs from the required input set' : 'contract has a missing or invalid required input set'));
  }
});

after(() => {
  const baseline = git(project, 'rev-parse', 'HEAD');
  const report = {schema_version: 1, scope: 'Real temporary Git repositories and handoff CLI; not ZIL language acceptance.', status: results.length === 13 && results.every(x => x.status === 'Pass') ? 'Pass' : 'Fail', executed: results.length, assertions, baseline_commit: baseline, source_hashes: Object.fromEntries(['tools/handoff.mjs', 'tests/handoff.test.mjs'].map(name => [name, sha(fs.readFileSync(path.join(project, name)))])), node: process.version, results};
  const output = writeReport('handoff-tests.json', report, {file: process.env.ZIL_HANDOFF_REPORT});
  console.log(JSON.stringify({handoff_report: output, status: report.status, executed: report.executed, assertions: report.assertions}));
});
