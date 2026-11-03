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
const migrationPath = 'docs/validation/handoff-migration-326.json';
const pointer = (value, selector = '') => {
  if (typeof selector !== 'string' || selector !== '' && !selector.startsWith('/')) return undefined;
  for (const part of selector === '' ? [] : selector.slice(1).split('/')) {
    if (/~(?![01])/u.test(part)) return undefined;
    const key = part.replaceAll('~1', '/').replaceAll('~0', '~');
    if (!value || typeof value !== 'object' || !Object.hasOwn(value, key)) return undefined;
    value = value[key];
  }
  return value;
};
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

export function inspectHandoff(cwd = process.cwd(), mainTree, {legacyAudit = false} = {}) {
  const errors = [], warnings = [];
  const complain = (id, reason) => errors.push({id, reason});
  const gitCache = new Map();
  const cachedGit = (...args) => {
    const key = JSON.stringify(args);
    if (!gitCache.has(key)) gitCache.set(key, git(root, ...args));
    return gitCache.get(key);
  };
  const top = git(cwd, 'rev-parse', '--show-toplevel');
  const current = top.ok ? fs.realpathSync(top.text) : path.resolve(cwd);
  const listing = top.ok ? git(current, 'worktree', 'list', '--porcelain') : {text: ''};
  const trees = listing.text.split(/\n\n/).filter(Boolean).map(block => {
    const lines = block.split('\n');
    return {path: lines.find(x => x.startsWith('worktree '))?.slice(9), branch: lines.find(x => x.startsWith('branch '))?.slice(7), head: lines.find(x => x.startsWith('HEAD '))?.slice(5)};
  }).filter(x => x.path);
  const mains = trees.filter(x => x.branch === 'refs/heads/main');
  const common = top.ok ? git(current, 'rev-parse', '--path-format=absolute', '--git-common-dir') : {ok: false};
  const primary = common.ok && path.basename(common.text) === '.git' ? path.dirname(common.text) : null;
  const primaryWork = trees.find(tree => primary && path.resolve(tree.path) === path.resolve(primary) && tree.branch === 'refs/heads/work');
  let root = mainTree ? path.resolve(cwd, mainTree) : mains.length === 1 ? mains[0].path : mains.length === 0 && primaryWork ? primaryWork.path : current;
  if (fs.existsSync(root)) root = fs.realpathSync(root);
  let authorityVerified = top.ok;
  if (!top.ok) complain('checkout', 'Git checkout required: git clone --branch main HISTORY_BUNDLE_PATH zil-checkout ; choose the actual companion path from PACKAGE.json companions.history.file');
  if (top.ok && !mainTree && mains.length !== 1 && !(mains.length === 0 && primaryWork)) {
    complain('authority', 'Cannot identify one main worktree; pass --main-tree explicitly.'); authorityVerified = false;
  }
  if (top.ok && mainTree) {
    const a = git(current, 'rev-parse', '--path-format=absolute', '--git-common-dir');
    const b = git(root, 'rev-parse', '--path-format=absolute', '--git-common-dir');
    if (!a.ok || !b.ok || a.text !== b.text) { complain('authority', 'Explicit main tree belongs to another Git repository.'); authorityVerified = false; }
    const branch = git(root, 'branch', '--show-current').text;
    const listed = trees.some(tree => path.resolve(tree.path) === root);
    if (!listed || branch !== 'main' && !(branch === 'work' && primary && path.resolve(primary) === root)) {
      complain('authority', 'Explicit authority must be a main checkout or the primary work checkout.'); authorityVerified = false;
    }
  }
  const authorityBranch = git(root, 'branch', '--show-current').text || null;
  const mainHead = git(root, 'rev-parse', '--verify', 'HEAD').text || null;
  let migration = null;
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
  const evidence = (id, refs, revision, requirePassing = false, expectedReview = null, limitedScope = false, taskRecord = null) => {
    if (!Array.isArray(refs) || refs.length === 0) { complain(id, 'Evidence references are missing.'); return []; }
    let reviewMatched = false;
    const records = refs.map(ref => {
      const rawName = typeof ref === 'string' ? ref : ref?.result_path;
      const name = typeof rawName === 'string' ? rawName.replaceAll('\\', '/') : rawName;
      const full = inside(root, name);
      if (!full || !['evidence/', 'docs/validation/'].some(prefix => name.startsWith(prefix))) { complain(id, 'Evidence reference must stay under evidence/ or docs/validation/.'); return {path: name ?? null}; }
      let bytes, location;
      const reviewRevision = expectedReview?.review_commit;
      if (reviewRevision) {
        if (checkCommit(id, 'review_commit', reviewRevision)) {
          const saved = cachedGit('show', `${reviewRevision}:${name}`);
          if (saved.ok) { bytes = saved.bytes; location = 'review_commit'; }
        }
      } else if (fs.existsSync(full) && fs.statSync(full).isFile()) {
        bytes = fs.readFileSync(full);
        if (tracked(name, bytes)) location = 'main';
      }
      if (!reviewRevision && !location && commitPattern.test(revision ?? '')) {
        const saved = git(root, 'show', `${revision}:${name}`);
        if (saved.ok) { bytes = Buffer.from(saved.raw); location = 'implementation_commit'; }
      }
      if (!location || !bytes?.length) { complain(id, `Evidence is missing, empty, or uncommitted: ${name}`); return {path: name}; }
      let result, selected;
      try { result = JSON.parse(bytes); }
      catch { complain(id, `Evidence must reference a structured JSON result: ${name}`); }
      const adapter = migration?.review_adapters.find(value => value.task_id === id && value.path === name);
      const selector = typeof ref === 'object' && ref !== null && Object.hasOwn(ref, 'selector') ? ref.selector : adapter?.selector ?? '';
      selected = pointer(result, selector);
      if (!selected || typeof selected !== 'object' || Array.isArray(selected)) {
        complain(id, `Evidence selector does not name a structured record: ${name}`); return {path: name, selector};
      }
      const outcomes = [];
      if (Object.hasOwn(selected, 'status')) {
        if (selected.status === 'LocalLimitedAcceptance') {
          const bound = taskRecord && selected.implementation_commits?.[id] === taskRecord.implementation_commit && selected.tested_commit === taskRecord.accepted_candidate;
          const gates = Array.isArray(selected.gates) && selected.gates.some(gate => Number.isFinite(gate.passed) && gate.passed > 0) && selected.gates.every(gate => gate.status === 'Pass' && gate.exit_code === 0 && (!Object.hasOwn(gate, 'failed') || gate.failed === 0) && (!Object.hasOwn(gate, 'passed') || gate.passed > 0));
          outcomes.push(limitedScope && Boolean(bound) && gates && availability(selected.tested_commit) === 'Present' && cachedGit('merge-base', '--is-ancestor', taskRecord.implementation_commit, selected.tested_commit).ok);
        } else outcomes.push(['Pass', 'Success'].includes(selected.status));
      }
      if (Object.hasOwn(selected, 'ok')) outcomes.push(selected.ok === true);
      if (Object.hasOwn(selected, 'draft_gate')) outcomes.push(selected.draft_gate === 'Pass');
      if (Object.hasOwn(selected, 'verdict')) outcomes.push(selected.verdict === 'Pass' || limitedScope && selected.verdict === 'PassLimited');
      for (const field of ['executed', 'assertions']) if (Object.hasOwn(selected, field)) outcomes.push(Number.isFinite(selected[field]) && selected[field] > 0);
      const passing = outcomes.length > 0 && outcomes.every(Boolean);
      if (requirePassing && !passing) complain(id, `Evidence has no passing outcome: ${name}`);
      let binding = 'Exact';
      if (expectedReview && passing) {
        const commitField = typeof ref === 'object' && ref !== null && ref.commit_field ? ref.commit_field : adapter?.commit_field ?? 'implementation_commit';
        const reviewedCommit = selected[commitField];
        let commitMatches = reviewedCommit === expectedReview.implementation_commit;
        const proof = adapter?.metadata_only;
        if (!commitMatches && proof && proof.reviewed_commit === reviewedCommit && proof.delivery_commit === expectedReview.implementation_commit) {
          const allowed = proof.allowed_paths;
          const validPaths = Array.isArray(allowed) && allowed.length > 0 && allowed.every(value => typeof value === 'string' && /^changes\/[^/]+\.md$/.test(value) && inside(root, value));
          const diff = validPaths && availability(reviewedCommit) === 'Present' && availability(proof.delivery_commit) === 'Present' ? cachedGit('diff', '--name-only', reviewedCommit, proof.delivery_commit) : null;
          commitMatches = Boolean(diff?.ok && cachedGit('merge-base', '--is-ancestor', reviewedCommit, proof.delivery_commit).ok && diff.text.split('\n').filter(Boolean).every(value => allowed.includes(value)));
          if (commitMatches) binding = 'MetadataOnlyEquivalent';
          else complain(id, 'Metadata-only review binding changes source/tests or lacks its exact whitelist proof.');
        }
        if ((selected.reviewer ?? result.reviewer) === expectedReview.reviewer && selected.verdict === expectedReview.verdict && commitMatches) reviewMatched = true;
      }
      return {path: name, sha256: digest(bytes), location, passing, ...(selector ? {selector} : {}), ...(expectedReview ? {binding, scope: selected.verdict === 'PassLimited' ? 'Limited' : 'Declared'} : {})};
    });
    if (expectedReview && !reviewMatched) complain(id, 'No review evidence matches the recorded reviewer and implementation commit.');
    return records;
  };

  const frozen = readJson('data/normative-inputs.json');
  const specPath = path.join(root, '_zil_design.md');
  const specHash = fs.existsSync(specPath) ? digest(fs.readFileSync(specPath)) : null;
  if (!specHash) complain('spec', 'Language specification is missing.');
  if (frozen?.files) {
    const required = readJson('spec/interfaces.json')?.normative_inputs;
    const registered = Object.keys(frozen.files);
    const canonical = name => typeof name === 'string' && !name.includes('\\') && path.posix.normalize(name) === name && inside(root, name);
    if (!Array.isArray(required) || required.length === 0 || new Set(required).size !== required.length || !required.includes('_zil_design.md') || !required.includes('spec/interfaces.json') || required.some(name => !canonical(name)) || required.length !== registered.length || registered.some(name => !required.includes(name))) complain('normative', 'Current normative registry differs from the complete required input set.');
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

  const availability = value => !commitPattern.test(value ?? '') ? 'Invalid' : cachedGit('cat-file', '-e', `${value}^{commit}`).ok ? 'Present' : 'Missing';
  const references = task => {
    const result = [];
    if (!task) return result;
    for (const field of ['baseline', 'base_revision', 'implementation_commit', 'integration_commit']) {
      if (task[field]) result.push({field, commit: task[field], availability: availability(task[field])});
    }
    if (task.review && typeof task.review === 'object') for (const field of ['implementation_commit', 'review_commit']) {
      if (task.review[field]) result.push({field: `review.${field}`, commit: task.review[field], availability: availability(task.review[field])});
    }
    return result;
  };
  const historicalRecords = new Map();
  if (fs.existsSync(path.join(root, migrationPath))) {
    const bytes = fs.readFileSync(path.join(root, migrationPath));
    const policy = readJson(migrationPath);
    if (!tracked(migrationPath, bytes)) complain('migration', 'Migration policy must be committed; working or untracked classifications cannot hide current errors.');
    else if (!policy || policy.schema_version !== 1 || policy.kind !== 'HandoffHistoricalMigration' || policy.approved_by !== '/root' || policy.claims_revalidated !== false || !Array.isArray(policy.records) || !Array.isArray(policy.review_adapters)) {
      complain('migration', 'Migration policy is malformed or claims historical revalidation.');
    } else if (checkCommit('migration', 'source_commit', policy.source_commit)) {
      migration = policy;
      const seen = new Set();
      for (const entry of policy.records) {
        const name = entry?.path;
        if (!/^\.task\/(?:impl\/)?[^/]+\.md$/.test(name ?? '') || !inside(root, name) || seen.has(name) || !['Current', 'HistoricalRecord'].includes(entry.classification)) {
          complain('migration', 'Migration task path or classification is invalid or duplicated.'); continue;
        }
        seen.add(name);
        const original = cachedGit('show', `${policy.source_commit}:${name}`);
        if (!original.ok || digest(original.bytes) !== entry.sha256) {
          complain('migration', `Frozen task bytes do not match the source commit: ${name}`); continue;
        }
        let task = null;
        try { task = JSON.parse(original.raw.match(/```json\r?\n([\s\S]*?)\r?\n```/)?.[1] ?? ''); } catch { /* An explicitly frozen note is not invented JSON. */ }
        const state = task?.status ?? original.raw.split(/\r?\n/)[0];
        if (state !== entry.recorded_state || JSON.stringify(references(task)) !== JSON.stringify(entry.object_references)) {
          complain('migration', `Frozen state or object availability differs: ${name}`); continue;
        }
        const expectedEvidence = [];
        for (const [field, refs] of [['evidence', task?.evidence], ['review.evidence', task?.review?.evidence]]) {
          for (const ref of Array.isArray(refs) ? refs : []) {
            const evidencePath = typeof ref === 'string' ? ref : ref?.result_path;
            if (typeof evidencePath !== 'string') continue;
            const saved = cachedGit('show', `${policy.source_commit}:${evidencePath}`);
            expectedEvidence.push({field, path: evidencePath, availability: saved.ok && saved.bytes.length ? 'PresentAtSnapshot' : 'MissingAtSnapshot', sha256: saved.ok && saved.bytes.length ? digest(saved.bytes) : null});
          }
        }
        if (JSON.stringify(expectedEvidence) !== JSON.stringify(entry.evidence_references)) {
          complain('migration', `Frozen evidence inventory differs: ${name}`); continue;
        }
        if (entry.classification === 'HistoricalRecord') {
          const full = inside(root, name);
          if (!full || !fs.existsSync(full) || digest(fs.readFileSync(full)) !== entry.sha256 || !tracked(name, fs.readFileSync(full))) {
            complain('migration', `Historical record was removed, changed or uncommitted: ${name}`); continue;
          }
          const unavailable = !task || entry.object_references.some(ref => ref.availability !== 'Present') || entry.evidence_references.some(ref => ref.availability !== 'PresentAtSnapshot');
          historicalRecords.set(name, {...entry, parent_task: task?.parent_task ?? null, availability: unavailable ? 'Unavailable' : 'Unreviewed', claims_revalidated: false});
        }
      }
      for (const adapter of policy.review_adapters) {
        if (!nonempty(adapter.task_id) || !nonempty(adapter.path) || !adapter.path.startsWith('docs/validation/') || !inside(root, adapter.path) || typeof adapter.selector !== 'string' || !adapter.selector.startsWith('/') || !['implementation_commit', 'delivery_commit', 'commit'].includes(adapter.commit_field)) {
          complain('migration', 'Compact review adapter is invalid.');
        }
      }
    }
  }

  const tasks = [], slices = [], legacyErrors = [];
  for (const [directory, output, isSlice] of [['.task', tasks, false], ['.task/impl', slices, true]]) {
    const folder = path.join(root, directory);
    if (!fs.existsSync(folder)) { if (!isSlice) complain('tasks', 'Task directory is missing.'); continue; }
    for (const filename of fs.readdirSync(folder).filter(x => x.endsWith('.md') && x !== 'TEMPLATE.md').sort()) {
      const relative = `${directory}/${filename}`, bytes = fs.readFileSync(path.join(root, relative));
      const archived = historicalRecords.get(relative);
      if (archived && !legacyAudit) {
        output.push({id: filename.slice(0, -3), path: relative, status: archived.recorded_state, baseline: archived.object_references.find(ref => ['baseline', 'base_revision'].includes(ref.field))?.commit ?? null, record_sha256: digest(bytes), committed: true, validation: {classification: 'HistoricalRecord', availability: archived.availability, claims_revalidated: false}, parent_task: archived.parent_task});
        continue;
      }
      const errorStart = errors.length;
      const match = bytes.toString().match(/```json\r?\n([\s\S]*?)\r?\n```/);
      let task;
      try { task = JSON.parse(match?.[1] ?? ''); }
      catch {
        complain(relative, 'Task has no valid JSON record.');
        if (archived) legacyErrors.push(...errors.splice(errorStart));
        continue;
      }
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
          record.evidence = evidence(id, task.evidence, task.implementation_commit, task.status === '已合入', null, nonempty(task.scope_contract), task);
        }
        if (task.status === '已合入' || task.review && ['Pass', 'PassLimited'].includes(task.review.verdict)) {
          const review = task.review;
          const allowedVerdict = review?.verdict === 'Pass' || migration && nonempty(task.scope_contract) && review?.verdict === 'PassLimited';
          if (!review || !allowedVerdict || !nonempty(review.reviewer) || review.reviewer.trim() === (nonempty(task.implementation_author) ? task.implementation_author.trim() : null) || review.implementation_commit !== task.implementation_commit) complain(id, 'Merged slice needs an independent passing review bound to its implementation commit.');
          record.review = review ? {reviewer: review.reviewer, verdict: review.verdict, implementation_commit: review.implementation_commit, evidence: evidence(id, review.evidence, null, true, review, Boolean(allowedVerdict && review.verdict === 'PassLimited'))} : null;
        }
        if (task.status === '已合入') {
          if (task.review?.review_commit && !cachedGit('merge-base', '--is-ancestor', task.review.review_commit, mainHead).ok) complain(id, 'Review commit is not in main history.');
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
      if (archived) {
        record.validation = {classification: 'HistoricalRecord', availability: archived.availability, claims_revalidated: false};
        legacyErrors.push(...errors.splice(errorStart));
      }
    }
  }
  if (!tasks.length) complain('tasks', 'No formal task records were found.');
  const byId = new Map(tasks.map(t => [t.id, t]));
  for (const task of tasks) if (activeStates.has(task.status) && task.blocked_by.some(id => byId.get(id)?.status !== 'Done')) complain(task.id, 'Active formal task has an incomplete dependency.');
  for (const slice of slices) if (slice.validation?.classification !== 'HistoricalRecord' && !byId.has(slice.parent_task)) complain(slice.id, 'Implementation slice has no formal parent task.');
  let measured = null;
  const summary = path.join(root, 'evidence/final-summary.json');
  if (fs.existsSync(summary)) {
    measured = readJson('evidence/final-summary.json');
    if (measured) {
      checkCommit('measured', 'source_commit', measured.source_commit);
      evidence('measured', [measured.result_path], measured.source_commit);
    }
  } else if (migration?.current_acceptance) {
    const name = migration.current_acceptance, full = inside(root, name);
    if (!full || !name.startsWith('docs/validation/') || !fs.existsSync(full) || !tracked(name, fs.readFileSync(full))) complain('measured', 'Current compact acceptance must be a committed docs/validation JSON record.');
    else {
      const compact = readJson(name);
      if (compact && checkCommit('measured', 'tested_commit', compact.tested_commit)) measured = {
        source_commit: compact.tested_commit, receipt_status: compact.status, result_path: name,
        known_incomplete: compact.known_incomplete,
        gates: Array.isArray(compact.gates) ? compact.gates.map(({gate, status, passed, failed, ignored, filtered_out}) => ({gate, status, passed, failed, ignored, filtered_out})) : [],
      };
    }
  }
  const currentGate = {status: errors.length ? 'CurrentFail' : 'CurrentPass', error_count: errors.length};
  const unavailable = [...historicalRecords.values()].filter(record => record.availability === 'Unavailable').length;
  const legacyFailed = errors.length + legacyErrors.length > 0;
  return {
    schema_version: migration ? 2 : 1, scope: 'Current handoff record integrity; historical availability is separate. Not language correctness, review quality or full task acceptance.',
    status: migration ? legacyAudit ? legacyFailed ? 'LegacyAuditFail' : 'LegacyAuditPass' : currentGate.status : errors.length ? 'Fail' : 'Pass', snapshot_only: true,
    current_gate: currentGate,
    historical_status: {status: unavailable ? 'Unavailable' : historicalRecords.size ? 'Unreviewed' : 'NotClassified', archived: historicalRecords.size, unavailable, unreviewed: historicalRecords.size - unavailable, claims_revalidated: false, source_commit: migration?.source_commit ?? null},
    legacy_audit: {status: legacyAudit ? legacyFailed ? 'Fail' : 'Pass' : 'NotRun', errors: legacyAudit ? [...errors, ...legacyErrors] : []},
    authority: {method: mainTree ? 'explicit-main-worktree' : top.ok ? authorityBranch === 'work' ? 'git-primary-worktree' : 'git-main-worktree' : 'archive-without-git', task_root: '.task', verified: authorityVerified, branch: authorityBranch, current_is_main: authorityVerified && current === root, origin_main_tracking_head: top.ok ? cachedGit('rev-parse', '--verify', 'refs/remotes/origin/main').text || null : null, remote_observation: 'Local tracking ref only; no network or remote merge claim.'},
    current_head: git(current, 'rev-parse', '--verify', 'HEAD').text || null, main_head: mainHead,
    spec_hash: specHash, normative_hash: frozen?.normative_hash ?? null,
    worktrees: trees.map(t => ({branch: t.branch ?? null, head: t.head ?? null, is_main: path.resolve(t.path) === root})),
    measured: measured ? {source_commit: measured.source_commit, draft_gate: measured.draft_gate, full_language_gate: measured.full_language_gate, product_gate: measured.product_gate, result_path: measured.result_path, receipt_status: measured.receipt_status, gates: measured.gates, known_incomplete: measured.known_incomplete} : null,
    tasks, slices,
    next: slices.filter(t => t.validation?.classification !== 'HistoricalRecord' && t.status !== '已合入').map(t => ({id: t.id, status: t.status, authoritative_task: t.path})),
    commands: {cold_start: 'node tools/handoff.mjs --check', legacy_audit: 'node tools/handoff.mjs --check --legacy-audit', restore: 'git clone --branch main HISTORY_BUNDLE_PATH zil-checkout', restore_hint: 'Use the actual PACKAGE.json companion path. Cleanup-deleted objects/evidence remain unavailable.', plan: 'python3 tools/check-plan.py --self-test', draft: 'python3 tools/validate-draft.py'},
    warnings, errors,
    runtime: {main_tree: root, current_tree: current},
  };
}

export function runCli(args = process.argv.slice(2), cwd = process.cwd()) {
  let check = false, mainTree, legacyAudit = false;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--check') check = true;
    else if (args[i] === '--legacy-audit') { legacyAudit = true; check = true; }
    else if (args[i] === '--main-tree' && args[i + 1]) mainTree = args[++i];
    else throw new Error(`Unknown or incomplete option: ${args[i]}`);
  }
  const report = inspectHandoff(cwd, mainTree, {legacyAudit});
  const {runtime, ...portable} = report;
  if (!check) {
    if (!report.authority.verified || !report.authority.current_is_main || report.authority.method === 'archive-without-git' || fs.realpathSync(cwd) !== runtime.current_tree) throw new Error('Generate the snapshot at the main Git checkout root; linked worktrees use --check.');
    const folder = path.join(runtime.main_tree, 'data');
    fs.mkdirSync(folder, {recursive: true});
    const temporary = path.join(folder, `.handoff-${process.pid}.tmp`);
    try { fs.writeFileSync(temporary, JSON.stringify(portable, null, 2) + '\n'); fs.renameSync(temporary, path.join(folder, 'handoff.json')); }
    finally { if (fs.existsSync(temporary)) fs.unlinkSync(temporary); }
  }
  console.log(JSON.stringify({...portable, authority: {...portable.authority, main_tree_now: runtime.main_tree}, output: check ? null : 'data/handoff.json'}, null, 2));
  return report.current_gate.status === 'CurrentPass' && (!legacyAudit || report.legacy_audit.status === 'Pass') ? 0 : 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = runCli(); }
  catch (error) { console.error(JSON.stringify({status: 'Fail', error: error.message})); process.exitCode = 1; }
}
