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

const migrationFile = 'docs/validation/handoff-migration-326.json';
function migrate(f, {names = ['.task/impl/003.md'], historical = true, adapters = []} = {}) {
  const source = git(f.root, 'rev-parse', 'HEAD');
  const availability = value => /^[0-9a-f]{40}$/.test(value ?? '') ? capture('git', ['-C', f.root, 'cat-file', '-e', `${value}^{commit}`]).status === 0 ? 'Present' : 'Missing' : 'Invalid';
  const records = names.map(name => {
    const bytes = fs.readFileSync(path.join(f.root, name));
    let task;
    try { task = JSON.parse(bytes.toString().match(/```json\n([\s\S]*?)\n```/)[1]); } catch { task = null; }
    const refs = [];
    for (const field of ['baseline', 'base_revision', 'implementation_commit', 'integration_commit']) if (task?.[field]) refs.push({field, commit: task[field], availability: availability(task[field])});
    for (const field of ['implementation_commit', 'review_commit']) if (task?.review?.[field]) refs.push({field: `review.${field}`, commit: task.review[field], availability: availability(task.review[field])});
    const evidence = [];
    for (const [field, values] of [['evidence', task?.evidence], ['review.evidence', task?.review?.evidence]]) for (const ref of values ?? []) {
      const name = typeof ref === 'string' ? ref : ref.result_path;
      const bytes = capture('git', ['-C', f.root, 'show', `${source}:${name}`]);
      const present = bytes.status === 0 && bytes.stdout.length > 0;
      evidence.push({field, path: name, availability: present ? 'PresentAtSnapshot' : 'MissingAtSnapshot', sha256: present ? sha(bytes.stdout) : null});
    }
    return {path: name, sha256: sha(bytes), recorded_state: task?.status ?? bytes.toString().split('\n')[0], classification: historical ? 'HistoricalRecord' : 'Current', reason: 'Explicit fixture migration', object_references: refs, evidence_references: evidence};
  });
  const policy = {schema_version: 1, kind: 'HandoffHistoricalMigration', source_commit: source, approved_by: '/root', claims_revalidated: false, records, review_adapters: adapters};
  put(f.root, migrationFile, policy); commit(f.root, 'explicit historical migration');
  return policy;
}

scenario('handoff-014', '历史Unavailable与当前门及只读audit分离', t => {
  const f = fixture(t); f.task.baseline = 'a'.repeat(40); f.save(); commit(f.root, 'retain lost original baseline'); migrate(f);
  const report = inspectHandoff(f.root);
  check(report.status, 'CurrentPass'); check(report.current_gate.status, 'CurrentPass');
  check(report.historical_status.status, 'Unavailable'); check(report.historical_status.claims_revalidated, false); check(report.next, []);
  check(report.slices[0].status, '施工'); check(report.slices[0].validation.availability, 'Unavailable');
  const created = capture(process.execPath, [script], {cwd: f.root}); check(created.status, 0);
  const saved = fs.readFileSync(path.join(f.root, 'data/handoff.json'));
  const legacy = capture(process.execPath, [script, '--legacy-audit'], {cwd: f.root});
  check(legacy.status, 1); const audit = JSON.parse(legacy.stdout);
  check(audit.status, 'LegacyAuditFail'); check(audit.current_gate.status, 'CurrentPass'); check(audit.legacy_audit.status, 'Fail');
  check(audit.legacy_audit.errors.some(e => e.reason.includes('baseline must identify')), true);
  check(fs.readFileSync(path.join(f.root, 'data/handoff.json')), saved);
});

scenario('handoff-015', '未列当前错误与在途hash不能被历史清单遮蔽', t => {
  const f = fixture(t); migrate(f);
  const current = {...f.task, id: '004', baseline: 'b'.repeat(40)};
  put(f.root, '.task/impl/004.md', `施工\n\`\`\`json\n${JSON.stringify(current)}\n\`\`\`\n`); commit(f.root, 'new current task with a missing baseline');
  let report = inspectHandoff(f.root); check(report.status, 'CurrentFail'); check(report.errors.some(e => e.id === '004' && e.reason.includes('baseline must identify')));
  current.baseline = f.baseline; current.spec_hash = '0'.repeat(64);
  put(f.root, '.task/impl/004.md', `施工\n\`\`\`json\n${JSON.stringify(current)}\n\`\`\`\n`); commit(f.root, 'current stale rules');
  report = inspectHandoff(f.root); check(report.status, 'CurrentFail'); check(report.errors.some(e => e.id === '004' && e.reason.includes('snapshot is stale')));
});

scenario('handoff-016', '冻结路径字节对象与未提交清单篡改拒绝', t => {
  for (const corruption of ['bytes', 'object', 'path', 'uncommitted']) {
    const f = fixture(t), policy = migrate(f);
    if (corruption === 'bytes') policy.records[0].sha256 = '0'.repeat(64);
    if (corruption === 'object') policy.records[0].object_references[0].availability = 'Missing';
    if (corruption === 'path') policy.records[0].path = '../outside.md';
    if (corruption === 'uncommitted') policy.claims_revalidated = true;
    put(f.root, migrationFile, policy);
    if (corruption !== 'uncommitted') commit(f.root, `retain invalid ${corruption} migration`);
    const report = inspectHandoff(f.root); check(report.current_gate.status, 'CurrentFail'); check(report.errors.some(e => e.id === 'migration'));
  }
});

scenario('handoff-017', '归档原字节不可改删', t => {
  for (const removed of [false, true]) {
    const f = fixture(t); migrate(f);
    if (removed) fs.unlinkSync(path.join(f.root, '.task/impl/003.md'));
    else put(f.root, '.task/impl/003.md', 'changed historical record\n');
    commit(f.root, 'retain detected historical edit');
    check(hasError(inspectHandoff(f.root), 'Historical record was removed, changed or uncommitted'));
  }
});

scenario('handoff-018', '历史笔记不伪造JSON且新任务不能加入旧快照', t => {
  const f = fixture(t); put(f.root, '.task/impl/153.md', '施工\n# Historical note; no independent review.\n'); commit(f.root, 'original note');
  const policy = migrate(f, {names: ['.task/impl/153.md']});
  check(inspectHandoff(f.root).status, 'CurrentPass');
  check(inspectHandoff(f.root, undefined, {legacyAudit: true}).legacy_audit.errors.some(e => e.reason.includes('no valid JSON')));
  put(f.root, '.task/impl/154.md', '施工\n# Newly created note\n'); commit(f.root, 'new note outside cutoff');
  policy.records.push({...policy.records[0], path: '.task/impl/154.md', sha256: sha(fs.readFileSync(path.join(f.root, '.task/impl/154.md')))});
  put(f.root, migrationFile, policy); commit(f.root, 'invalid later archive');
  check(hasError(inspectHandoff(f.root), 'Frozen task bytes do not match'));
});

scenario('handoff-019', '真实work权威与分树假权威明确区分', t => {
  const f = fixture(t); git(f.root, 'branch', '-m', 'work');
  let report = inspectHandoff(f.root); check(report.status, 'Pass'); check(report.authority.verified); check(report.authority.branch, 'work');
  check(inspectHandoff(f.root, f.root).authority.current_is_main);
  const child = path.join(f.area, '003'); git(f.root, 'worktree', 'add', '--quiet', '-b', '003', child);
  check(inspectHandoff(child).runtime.main_tree, fs.realpathSync(f.root));
  report = inspectHandoff(child, child); check(report.authority.verified, false); check(report.authority.current_is_main, false);
  git(f.root, 'branch', '-m', 'unknown');
  report = inspectHandoff(f.root); check(report.authority.verified, false); check(report.authority.current_is_main, false);
});

scenario('handoff-020', 'docs精简证据仍需提交且不能符号链接逃逸', t => {
  const f = fixture(t); merged(f);
  const name = 'docs/validation/receipt.json'; f.task.evidence = [name]; f.save();
  put(f.root, name, {status: 'Pass', executed: 1, assertions: 1});
  check(hasError(inspectHandoff(f.root), 'uncommitted'));
  commit(f.root, 'compact committed receipt'); check(inspectHandoff(f.root).status, 'Pass');
  put(f.root, name, {status: 'Fail', ok: true}); commit(f.root, 'failed compact receipt');
  check(hasError(inspectHandoff(f.root), 'Evidence has no passing outcome'));
  const outside = path.join(f.area, 'outside.json'); fs.writeFileSync(outside, '{"status":"Pass"}');
  fs.unlinkSync(path.join(f.root, name)); fs.symlinkSync(outside, path.join(f.root, name)); commit(f.root, 'escaping symlink');
  check(hasError(inspectHandoff(f.root), 'Evidence reference must stay under'));
});

scenario('handoff-021', '显式review selector精确提交与非作者核验', t => {
  const f = fixture(t); merged(f);
  const name = 'docs/validation/review.json';
  put(f.root, name, {reviewer: 'reviewer', cases: {'003': {verdict: 'Pass', implementation_commit: f.task.implementation_commit}, wrong: {verdict: 'Pass', implementation_commit: f.baseline}}});
  const revision = commit(f.root, 'exact compact review');
  f.task.review.review_commit = revision; f.task.review.evidence = [{result_path: name, selector: '/cases/003'}]; f.save(); commit(f.root, 'bind exact selector');
  check(inspectHandoff(f.root).status, 'Pass');
  f.task.review.evidence[0].selector = '/cases/wrong'; f.save(); commit(f.root, 'wrong case');
  check(hasError(inspectHandoff(f.root), 'No review evidence matches'));
  f.task.review.evidence[0].selector = '/cases/003'; f.task.review.reviewer = 'builder'; f.save(); commit(f.root, 'self review');
  check(hasError(inspectHandoff(f.root), 'independent passing review'));
});

scenario('handoff-022', '有限review不晋升正式Done', t => {
  const f = fixture(t); merged(f);
  f.task.scope_contract = 'Finite fixture only'; f.task.review.verdict = 'PassLimited';
  put(f.root, 'docs/validation/limited.json', {verdict: 'PassLimited', reviewer: 'reviewer', implementation_commit: f.task.implementation_commit});
  f.task.review.evidence = ['docs/validation/limited.json']; f.save(); commit(f.root, 'finite review');
  check(hasError(inspectHandoff(f.root), 'independent passing review'));
  migrate(f, {historical: false}); check(inspectHandoff(f.root).status, 'CurrentPass');
  const formal = JSON.parse(fs.readFileSync(path.join(f.root, '.task/B0.md'), 'utf8').match(/```json\n([\s\S]*?)\n```/)[1]);
  formal.status = 'Done'; formal.acceptance_record = 'docs/validation/limited.json'; formal.evidence = ['docs/validation/limited.json'];
  put(f.root, '.task/B0.md', `# B0\n\`\`\`json\n${JSON.stringify(formal)}\n\`\`\`\n`); commit(f.root, 'invalid full Done from limited review');
  check(hasError(inspectHandoff(f.root), 'Evidence has no passing outcome'));
});

scenario('handoff-023', 'metadata-only等价只允许明确文档差异', t => {
  for (const sourceChanged of [false, true]) {
    const f = fixture(t);
    put(f.root, 'src/main.rs', 'fn main() {}\n'); put(f.root, 'docs/validation/build.json', {status: 'Pass', executed: 1, assertions: 1});
    const reviewed = commit(f.root, 'reviewed source');
    put(f.root, 'changes/delivery.md', 'Delivery notes only.\n');
    if (sourceChanged) put(f.root, 'src/main.rs', 'fn main() { panic!() }\n');
    const delivery = commit(f.root, 'delivery');
    const name = 'docs/validation/review.json'; put(f.root, name, {reviewer: 'reviewer', cases: {'003': {verdict: 'Pass', implementation_commit: reviewed}}});
    const reviewCommit = commit(f.root, 'review receipt');
    Object.assign(f.task, {status: '已合入', implementation_commit: delivery, integration_commit: delivery, evidence: ['docs/validation/build.json'], review: {reviewer: 'reviewer', verdict: 'Pass', implementation_commit: delivery, review_commit: reviewCommit, evidence: [name]}});
    f.save(); commit(f.root, 'record exact delivery');
    migrate(f, {historical: false, adapters: [{task_id: '003', path: name, selector: '/cases/003', commit_field: 'implementation_commit', metadata_only: {reviewed_commit: reviewed, delivery_commit: delivery, allowed_paths: ['changes/delivery.md']}}]});
    const report = inspectHandoff(f.root);
    check(report.status, sourceChanged ? 'CurrentFail' : 'CurrentPass');
    if (sourceChanged) check(hasError(report, 'Metadata-only review binding changes source/tests'));
    else check(report.slices[0].review.evidence[0].binding, 'MetadataOnlyEquivalent');
  }
});

scenario('handoff-024', '当前规范必需集合不能被较小注册表冒充', t => {
  const f = fixture(t), registry = JSON.parse(fs.readFileSync(path.join(f.root, 'data/normative-inputs.json')));
  delete registry.files['spec/grammar.ebnf']; registry.normative_hash = sha(Object.entries(registry.files).sort(([a], [b]) => a.localeCompare(b)).map(([name, hash]) => `${name}\0${hash}\n`).join(''));
  put(f.root, 'data/normative-inputs.json', registry); commit(f.root, 'incomplete current registry');
  check(hasError(inspectHandoff(f.root), 'Current normative registry differs'));
  const other = fixture(t);
  put(other.root, 'spec/interfaces.json', {normative_inputs: ['_zil_design.md', 'spec/./grammar.ebnf', 'spec/interfaces.json']});
  const files = Object.fromEntries(['_zil_design.md', 'spec/./grammar.ebnf', 'spec/interfaces.json'].map(name => [name, sha(fs.readFileSync(path.join(other.root, name)))]));
  put(other.root, 'data/normative-inputs.json', {files, normative_hash: sha(Object.entries(files).sort(([a], [b]) => a.localeCompare(b)).map(([name, hash]) => `${name}\0${hash}\n`).join(''))});
  commit(other.root, 'noncanonical current normative aliases');
  check(hasError(inspectHandoff(other.root), 'Current normative registry differs'));
});

scenario('handoff-025', '精简本地验收仍绑定实际交付和非零门', t => {
  const f = fixture(t); merged(f);
  f.task.scope_contract = 'Finite implementation fixture'; f.task.accepted_candidate = f.task.implementation_commit;
  const name = 'docs/validation/integration.json';
  const receipt = {status: 'LocalLimitedAcceptance', tested_commit: f.task.accepted_candidate, implementation_commits: {'003': f.task.implementation_commit}, gates: [{gate: 'workspace', status: 'Pass', exit_code: 0, passed: 2, failed: 0}]};
  put(f.root, name, receipt); f.task.evidence = [name]; f.save(); commit(f.root, 'bounded compact local acceptance');
  migrate(f, {historical: false}); check(inspectHandoff(f.root).status, 'CurrentPass');
  for (const corrupt of ['zero', 'wrong-implementation', 'failed-gate']) {
    const bad = structuredClone(receipt);
    if (corrupt === 'zero') bad.gates[0].passed = 0;
    if (corrupt === 'wrong-implementation') bad.implementation_commits['003'] = f.baseline;
    if (corrupt === 'failed-gate') bad.gates[0].status = 'Fail';
    put(f.root, name, bad); commit(f.root, corrupt);
    check(hasError(inspectHandoff(f.root), 'Evidence has no passing outcome'));
  }
});

after(() => {
  const baseline = git(project, 'rev-parse', 'HEAD');
  const expectedIds = Array.from({length: 25}, (_, n) => `handoff-${String(n + 1).padStart(3, '0')}`);
  const report = {schema_version: 1, scope: 'Real temporary Git repositories and handoff CLI; not ZIL language acceptance.', status: results.length === expectedIds.length && expectedIds.every(id => results.some(row => row.id === id)) && results.every(x => x.status === 'Pass') ? 'Pass' : 'Fail', executed: results.length, assertions, baseline_commit: baseline, source_hashes: Object.fromEntries(['tools/handoff.mjs', 'tests/handoff.test.mjs'].map(name => [name, sha(fs.readFileSync(path.join(project, name)))])), node: process.version, results};
  const output = writeReport('handoff-tests.json', report, {file: process.env.ZIL_HANDOFF_REPORT});
  console.log(JSON.stringify({handoff_report: output, status: report.status, executed: report.executed, assertions: report.assertions}));
});
