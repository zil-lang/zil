#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const digest = value => createHash('sha256').update(value).digest('hex');
const commitPattern = /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/;
const sliceStates = new Set(['施工', '待验', '退回', '已合入', '卡住']);
const formalStates = new Set(['Planned', 'Ready', 'Doing', 'Review', 'Done', 'Cancelled']);
const activeStates = new Set(['Ready', 'Doing', 'Review', 'Done']);
const nonempty = value => typeof value === 'string' && value.trim().length > 0;
const git = (cwd, ...args) => {
  const area = fs.mkdtempSync(path.join(os.tmpdir(), 'zil-handoff-git-'));
  const handles = [];
  try {
    for (const name of ['stdout', 'stderr']) handles.push(fs.openSync(path.join(area, name), 'w+'));
    const result = spawnSync('git', ['-C', cwd, ...args], {stdio: ['ignore', ...handles], timeout: 10000});
    if (result.error) throw new Error(`Git infrastructure failure (${result.error.code ?? 'spawnSync'}): ${result.error.message}`);
    const bytes = fs.readFileSync(path.join(area, 'stdout'));
    const stdout = bytes.toString('utf8');
    const stderr = fs.readFileSync(path.join(area, 'stderr'), 'utf8');
    return {ok: !result.error && result.status === 0, text: stdout.trim(), raw: stdout, bytes, error: stderr.trim()};
  } finally {
    for (const handle of handles) fs.closeSync(handle);
    fs.rmSync(area, {recursive: true, force: true});
  }
};
const inside = (root, name) => {
  if (typeof name !== 'string' || !name || path.isAbsolute(name) || /^[A-Za-z]:/.test(name)) return null;
  const parts = name.replaceAll('\\', '/').split('/');
  if (parts.includes('..')) return null;
  const result = path.resolve(root, ...parts);
  if (!result.startsWith(root + path.sep)) return null;
  if (fs.existsSync(result) && !fs.realpathSync(result).startsWith(root + path.sep)) return null;
  return result;
};

export function inspectHandoff(cwd = process.cwd(), mainTree) {
  const errors = [], warnings = [];
  const complain = (id, reason) => errors.push({id, reason});
  const top = git(cwd, 'rev-parse', '--show-toplevel');
  const current = top.ok ? fs.realpathSync(top.text) : path.resolve(cwd);
  const listing = top.ok ? git(current, 'worktree', 'list', '--porcelain') : {text: ''};
  const trees = listing.text.split(/\n\n/).filter(Boolean).map(block => {
    const lines = block.split('\n');
    return {path: lines.find(x => x.startsWith('worktree '))?.slice(9), branch: lines.find(x => x.startsWith('branch '))?.slice(7), head: lines.find(x => x.startsWith('HEAD '))?.slice(5)};
  }).filter(x => x.path);
  const mains = trees.filter(x => x.branch === 'refs/heads/main');
  let root = mainTree ? path.resolve(cwd, mainTree) : mains.length === 1 ? mains[0].path : current;
  if (fs.existsSync(root)) root = fs.realpathSync(root);
  if (!top.ok) complain('checkout', 'Git checkout required: git clone --branch main HISTORY_BUNDLE_PATH zil-checkout ; choose the actual companion path from PACKAGE.json companions.history.file');
  if (top.ok && !mainTree && mains.length !== 1) complain('authority', 'Cannot identify one main worktree; pass --main-tree explicitly.');
  if (top.ok && mainTree) {
    const a = git(current, 'rev-parse', '--path-format=absolute', '--git-common-dir');
    const b = git(root, 'rev-parse', '--path-format=absolute', '--git-common-dir');
    if (!a.ok || !b.ok || a.text !== b.text) complain('authority', 'Explicit main tree belongs to another Git repository.');
    if (git(root, 'branch', '--show-current').text !== 'main') complain('authority', 'Explicit authority must be the main branch checkout.');
  }
  const mainHead = git(root, 'rev-parse', '--verify', 'HEAD').text || null;
  const readJson = name => {
    try { return JSON.parse(fs.readFileSync(path.join(root, name), 'utf8')); }
    catch (error) { complain(name, `Cannot read JSON: ${error.message}`); return null; }
  };
  const tracked = (name, bytes, revision = 'HEAD') => {
    const result = git(root, 'show', `${revision}:${name}`);
    return result.ok && result.raw === bytes.toString();
  };
  const checkCommit = (id, field, value) => {
    if (!commitPattern.test(value ?? '') || !git(root, 'cat-file', '-e', `${value}^{commit}`).ok) {
      complain(id, `${field} must identify an existing complete commit.`); return false;
    }
    return true;
  };
  const evidence = (id, refs, revision, requirePassing = false, expectedReview = null) => {
    if (!Array.isArray(refs) || refs.length === 0) { complain(id, 'Evidence references are missing.'); return []; }
    let reviewMatched = false;
    const records = refs.map(ref => {
      const rawName = typeof ref === 'string' ? ref : ref?.result_path;
      const name = typeof rawName === 'string' ? rawName.replaceAll('\\', '/') : rawName;
      const full = inside(root, name);
      if (!full || !name.replaceAll('\\', '/').startsWith('evidence/')) { complain(id, 'Evidence reference must stay under evidence/.'); return {path: name ?? null}; }
      let bytes, location;
      if (fs.existsSync(full) && fs.statSync(full).isFile()) {
        bytes = fs.readFileSync(full);
        if (tracked(name, bytes)) location = 'main';
      }
      if (!location && commitPattern.test(revision ?? '')) {
        const saved = git(root, 'show', `${revision}:${name}`);
        if (saved.ok) { bytes = Buffer.from(saved.raw); location = 'implementation_commit'; }
      }
      if (!location || !bytes?.length) { complain(id, `Evidence is missing, empty, or uncommitted: ${name}`); return {path: name}; }
      let result;
      try { result = JSON.parse(bytes); }
      catch { complain(id, `Evidence must reference a structured JSON result: ${name}`); }
      const outcomes = [];
      if (result && Object.hasOwn(result, 'status')) outcomes.push(['Pass', 'Success'].includes(result.status));
      if (result && Object.hasOwn(result, 'ok')) outcomes.push(result.ok === true);
      if (result && Object.hasOwn(result, 'draft_gate')) outcomes.push(result.draft_gate === 'Pass');
      if (result && Object.hasOwn(result, 'verdict')) outcomes.push(result.verdict === 'Pass');
      for (const field of ['executed', 'assertions']) if (result && Object.hasOwn(result, field)) outcomes.push(Number.isFinite(result[field]) && result[field] > 0);
      const passing = outcomes.length > 0 && outcomes.every(Boolean);
      if (requirePassing && !passing) complain(id, `Evidence has no passing outcome: ${name}`);
      if (expectedReview && passing && result?.reviewer === expectedReview.reviewer && result?.verdict === 'Pass' && result?.implementation_commit === expectedReview.implementation_commit) reviewMatched = true;
      return {path: name, sha256: digest(bytes), location, passing};
    });
    if (expectedReview && !reviewMatched) complain(id, 'No review evidence matches the recorded reviewer and implementation commit.');
    return records;
  };

  const frozen = readJson('data/normative-inputs.json');
  const specPath = path.join(root, '_zil_design.md');
  const specHash = fs.existsSync(specPath) ? digest(fs.readFileSync(specPath)) : null;
  if (!specHash) complain('spec', 'Language specification is missing.');
  if (frozen?.files) {
    const lines = [];
    for (const [name, expected] of Object.entries(frozen.files).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) {
      const full = inside(root, name);
      const actual = full && fs.existsSync(full) ? digest(fs.readFileSync(full)) : null;
      if (actual !== expected) complain('normative', `Changed or missing input: ${name}`);
      lines.push(`${name}\0${actual}\n`);
    }
    if (digest(lines.join('')) !== frozen.normative_hash) complain('normative', 'Normative input digest does not match its registry.');
  } else complain('normative', 'Normative input registry has no file hashes.');

  const historicalSnapshots = new Map();
  const historicalSnapshot = revision => {
    if (historicalSnapshots.has(revision)) return historicalSnapshots.get(revision);
    const errors = [], result = {spec_hash: null, normative_hash: null, errors};
    historicalSnapshots.set(revision, result);
    if (!commitPattern.test(revision ?? '')) { errors.push('Historical specification needs a complete implementation commit.'); return result; }
    const saved = git(root, 'show', `${revision}:data/normative-inputs.json`);
    let registry;
    try { registry = saved.ok ? JSON.parse(saved.raw) : null; } catch { registry = null; }
    if (!registry?.files || typeof registry.files !== 'object' || Array.isArray(registry.files) || !Object.hasOwn(registry.files, '_zil_design.md')) {
      errors.push('Historical normative registry is missing or malformed.'); return result;
    }
    const contract = git(root, 'show', `${revision}:spec/interfaces.json`);
    let required;
    try { required = contract.ok ? JSON.parse(contract.raw).normative_inputs : null; } catch { required = null; }
    const canonicalPath = name => typeof name === 'string' && !name.includes('\\') && path.posix.normalize(name) === name && inside(root, name);
    if (!Array.isArray(required) || !required.length || required.some(name => !canonicalPath(name)) || new Set(required).size !== required.length || !required.includes('_zil_design.md') || !required.includes('spec/interfaces.json')) {
      errors.push('Historical normative contract has a missing or invalid required input set.'); return result;
    }
    const registered = Object.keys(registry.files);
    if (registered.length !== required.length || registered.some(name => !required.includes(name))) errors.push('Historical normative registry differs from the required input set.');
    const lines = [];
    for (const [name, expected] of Object.entries(registry.files).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) {
      if (!inside(root, name) || !/^[0-9a-f]{64}$/.test(expected)) { errors.push(`Historical normative input is invalid: ${name}`); continue; }
      const input = git(root, 'show', `${revision}:${name}`);
      const actual = input.ok ? digest(input.bytes) : null;
      if (actual !== expected) errors.push(`Historical normative input does not match its committed registry: ${name}`);
      lines.push(`${name}\0${actual}\n`);
      if (name === '_zil_design.md') result.spec_hash = actual;
    }
    result.normative_hash = digest(lines.join(''));
    if (result.normative_hash !== registry.normative_hash) errors.push('Historical normative digest does not match its committed registry.');
    return result;
  };

  const tasks = [], slices = [];
  for (const [directory, output, isSlice] of [['.task', tasks, false], ['.task/impl', slices, true]]) {
    const folder = path.join(root, directory);
    if (!fs.existsSync(folder)) { if (!isSlice) complain('tasks', 'Task directory is missing.'); continue; }
    for (const filename of fs.readdirSync(folder).filter(x => x.endsWith('.md') && x !== 'TEMPLATE.md').sort()) {
      const relative = `${directory}/${filename}`, bytes = fs.readFileSync(path.join(root, relative));
      const match = bytes.toString().match(/```json\r?\n([\s\S]*?)\r?\n```/);
      let task;
      try { task = JSON.parse(match?.[1] ?? ''); }
      catch { complain(relative, 'Task has no valid JSON record.'); continue; }
      if (!task || typeof task !== 'object' || Array.isArray(task)) { complain(relative, 'Task record must be an object.'); continue; }
      const id = task.id;
      if (id !== filename.slice(0, -3)) complain(relative, 'Task ID does not match filename.');
      if (!(isSlice ? sliceStates : formalStates).has(task.status)) complain(id, 'Unknown task state.');
      const historical = isSlice && task.status === '已合入';
      const snapshot = historical ? historicalSnapshot(task.implementation_commit) : {spec_hash: specHash, normative_hash: frozen?.normative_hash};
      if (historical) for (const reason of snapshot.errors) complain(id, reason);
      if (task.spec_hash !== snapshot.spec_hash || task.normative_hash !== snapshot.normative_hash) complain(id, historical ? 'Task specification snapshot differs from its implementation commit.' : 'Task specification snapshot is stale.');
      const baseline = isSlice ? task.baseline : task.base_revision;
      if (isSlice || activeStates.has(task.status)) checkCommit(id, 'baseline', baseline);
      const record = {id, path: relative, status: task.status, baseline: baseline ?? null, record_sha256: digest(bytes), committed: tracked(relative, bytes), evidence: []};
      record.specification = {source: historical ? 'implementation_commit' : 'current', revision: historical ? task.implementation_commit : null, spec_hash: snapshot.spec_hash, normative_hash: snapshot.normative_hash};
      if (isSlice) {
        record.parent_task = task.parent_task;
        record.write_set = task.write_set;
        record.implementation_author = task.implementation_author ?? null;
        record.implementation_commit = task.implementation_commit ?? null;
        record.integration_commit = task.integration_commit ?? null;
        if (bytes.toString().split(/\r?\n/)[0] !== task.status) complain(id, 'First-line state differs from JSON state.');
        if (!Array.isArray(task.write_set) || !task.write_set.length || task.write_set.some(x => !inside(root, x))) complain(id, 'Write set is missing or escapes the repository.');
        if (task.authoritative_task && (typeof task.authoritative_task !== 'string' || !task.authoritative_task.replaceAll('\\', '/').endsWith(`/${relative}`) && task.authoritative_task !== relative)) complain(id, 'Saved authority points to a different task.');
        if (['待验', '已合入'].includes(task.status)) {
          if (!nonempty(task.implementation_author)) complain(id, 'Implementation author is missing.');
          if (checkCommit(id, 'implementation_commit', task.implementation_commit) && commitPattern.test(baseline ?? '') && !git(root, 'merge-base', '--is-ancestor', baseline, task.implementation_commit).ok) complain(id, 'Implementation commit does not descend from its baseline.');
          record.evidence = evidence(id, task.evidence, task.implementation_commit, task.status === '已合入');
        }
        if (task.status === '已合入') {
          const review = task.review;
          if (!review || review.verdict !== 'Pass' || !nonempty(review.reviewer) || review.reviewer.trim() === (nonempty(task.implementation_author) ? task.implementation_author.trim() : null) || review.implementation_commit !== task.implementation_commit) complain(id, 'Merged slice needs an independent passing review bound to its implementation commit.');
          record.review = review ? {reviewer: review.reviewer, verdict: review.verdict, implementation_commit: review.implementation_commit, evidence: evidence(id, review.evidence, null, true, review)} : null;
          if (commitPattern.test(task.implementation_commit ?? '') && !git(root, 'merge-base', '--is-ancestor', task.implementation_commit, mainHead).ok) complain(id, 'Implementation commit is not in main history; retain the reviewed commit with merge --no-ff.');
          if (checkCommit(id, 'integration_commit', task.integration_commit) && !git(root, 'merge-base', '--is-ancestor', task.integration_commit, mainHead).ok) complain(id, 'Integration commit is not in main history.');
          if (!record.committed) complain(id, 'Merged task record has not been committed.');
        }
      } else {
        record.blocked_by = Array.isArray(task.blocked_by) ? task.blocked_by : [];
        if (!Array.isArray(task.blocked_by)) complain(id, 'Formal dependency list is missing.');
        if (task.evidence?.length || task.status === 'Done') record.evidence = evidence(id, task.evidence, null, task.status === 'Done');
        if (task.status === 'Done' && !task.acceptance_record) complain(id, 'Done task lacks an integrator acceptance record.');
      }
      if (!record.committed) warnings.push({id, reason: 'Current task record differs from committed main; it is a working instruction, not a completed handoff.'});
      output.push(record);
    }
  }
  if (!tasks.length) complain('tasks', 'No formal task records were found.');
  const byId = new Map(tasks.map(t => [t.id, t]));
  for (const task of tasks) if (activeStates.has(task.status) && task.blocked_by.some(id => byId.get(id)?.status !== 'Done')) complain(task.id, 'Active formal task has an incomplete dependency.');
  for (const slice of slices) if (!byId.has(slice.parent_task)) complain(slice.id, 'Implementation slice has no formal parent task.');
  let measured = null;
  const summary = path.join(root, 'evidence/final-summary.json');
  if (fs.existsSync(summary)) {
    measured = readJson('evidence/final-summary.json');
    if (measured) {
      checkCommit('measured', 'source_commit', measured.source_commit);
      evidence('measured', [measured.result_path], measured.source_commit);
    }
  }
  return {
    schema_version: 1, scope: 'Live handoff record integrity; not language correctness, review quality or full task acceptance.',
    status: errors.length ? 'Fail' : 'Pass', snapshot_only: true,
    authority: {method: mainTree ? 'explicit-main-worktree' : top.ok ? 'git-main-worktree' : 'archive-without-git', task_root: '.task', current_is_main: current === root},
    current_head: git(current, 'rev-parse', '--verify', 'HEAD').text || null, main_head: mainHead,
    spec_hash: specHash, normative_hash: frozen?.normative_hash ?? null,
    worktrees: trees.map(t => ({branch: t.branch ?? null, head: t.head ?? null, is_main: path.resolve(t.path) === root})),
    measured: measured ? {source_commit: measured.source_commit, draft_gate: measured.draft_gate, full_language_gate: measured.full_language_gate, product_gate: measured.product_gate, result_path: measured.result_path} : null,
    tasks, slices,
    next: slices.filter(t => t.status !== '已合入').map(t => ({id: t.id, status: t.status, authoritative_task: t.path})),
    commands: {cold_start: 'node tools/handoff.mjs --check', restore: 'git clone --branch main HISTORY_BUNDLE_PATH zil-checkout', restore_hint: 'Replace HISTORY_BUNDLE_PATH with the actual companion path named by PACKAGE.json companions.history.file', plan: 'python3 tools/check-plan.py --self-test', draft: 'python3 tools/validate-draft.py'},
    warnings, errors,
    runtime: {main_tree: root, current_tree: current},
  };
}

export function runCli(args = process.argv.slice(2), cwd = process.cwd()) {
  let check = false, mainTree;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--check') check = true;
    else if (args[i] === '--main-tree' && args[i + 1]) mainTree = args[++i];
    else throw new Error(`Unknown or incomplete option: ${args[i]}`);
  }
  const report = inspectHandoff(cwd, mainTree);
  const {runtime, ...portable} = report;
  if (!check) {
    if (!report.authority.current_is_main || report.authority.method === 'archive-without-git' || fs.realpathSync(cwd) !== runtime.current_tree) throw new Error('Generate the snapshot at the main Git checkout root; linked worktrees use --check.');
    const folder = path.join(runtime.main_tree, 'data');
    fs.mkdirSync(folder, {recursive: true});
    const temporary = path.join(folder, `.handoff-${process.pid}.tmp`);
    try { fs.writeFileSync(temporary, JSON.stringify(portable, null, 2) + '\n'); fs.renameSync(temporary, path.join(folder, 'handoff.json')); }
    finally { if (fs.existsSync(temporary)) fs.unlinkSync(temporary); }
  }
  console.log(JSON.stringify({...portable, authority: {...portable.authority, main_tree_now: runtime.main_tree}, output: check ? null : 'data/handoff.json'}, null, 2));
  return report.status === 'Pass' ? 0 : 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = runCli(); }
  catch (error) { console.error(JSON.stringify({status: 'Fail', error: error.message})); process.exitCode = 1; }
}
