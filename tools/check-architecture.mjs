#!/usr/bin/env node
// Checks Cargo workspace dependency declarations and module-scoped Rust path references
// (policy.module_scoped_dependencies); it does not check semantics.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const load = p => JSON.parse(fs.readFileSync(p, 'utf8').replace(/^\uFEFF/, ''));

function graphCycles(graph) {
  const mark = new Map(), stack = [], errors = [];
  function visit(n) {
    if (mark.get(n) === 2) return;
    if (mark.get(n) === 1) {
      errors.push(`dependency cycle: ${stack.slice(stack.indexOf(n)).concat(n).join(' -> ')}`);
      return;
    }
    mark.set(n, 1); stack.push(n);
    for (const to of graph[n] ?? []) if (Object.hasOwn(graph, to)) visit(to);
    stack.pop(); mark.set(n, 2);
  }
  for (const n of Object.keys(graph)) visit(n);
  return errors;
}

export function validatePolicy(policy) {
  const errors = [];
  if (!policy || policy.schema_version !== 1 || !policy.crates || Array.isArray(policy.crates))
    return ['invalid or missing policy schema'];
  const names = Object.keys(policy.crates);
  if (!names.length) errors.push('empty crate policy');
  for (const [name, deps] of Object.entries(policy.crates)) {
    if (!Array.isArray(deps) || deps.some(x => typeof x !== 'string')) {
      errors.push(`invalid dependency list for ${name}`); continue;
    }
    if (new Set(deps).size !== deps.length) errors.push(`duplicate policy edges for ${name}`);
    for (const d of deps) if (!Object.hasOwn(policy.crates, d)) errors.push(`unknown policy target ${name} -> ${d}`);
  }
  if (!errors.length) errors.push(...graphCycles(policy.crates));
  return errors;
}

export function checkMetadata(meta, policy) {
  const errors = validatePolicy(policy);
  if (errors.length) return { ok: false, errors, checkedCrates: 0, checkedEdges: 0 };
  if (!meta || !Array.isArray(meta.packages) || !Array.isArray(meta.workspace_members))
    return { ok:false, errors:['invalid cargo metadata: packages/workspace_members required'], checkedCrates:0, checkedEdges:0 };
  const byId = new Map(meta.packages.map(p => [p.id, p]));
  const members = [];
  for (const id of meta.workspace_members) {
    const p = byId.get(id);
    if (!p) errors.push(`workspace member missing package record: ${id}`);
    else members.push(p);
  }
  if (!members.length) errors.push('empty workspace is not a passing architecture check');
  const names = new Set(members.map(p => p.name));
  if (names.size !== members.length) errors.push('duplicate workspace package names');
  const graph = {};
  let checkedEdges = 0;
  for (const p of members) {
    graph[p.name] = [];
    if (!Object.hasOwn(policy.crates, p.name)) errors.push(`workspace crate not approved: ${p.name}`);
    if (!Array.isArray(p.dependencies)) { errors.push(`dependencies missing for ${p.name}`); continue; }
    for (const d of p.dependencies) {
      if (typeof d.name !== 'string') { errors.push(`dependency missing package name in ${p.name}`); continue; }
      // Cargo's name identifies the actual package; rename is only a local alias.
      if (names.has(d.name)) {
        checkedEdges++; graph[p.name].push(d.name);
        if (!(policy.crates[p.name] ?? []).includes(d.name))
          errors.push(`forbidden internal dependency: ${p.name} -> ${d.name} (${d.kind ?? 'normal'})`);
      } else if (d.name.startsWith('zil-')) {
        errors.push(`untracked internal dependency: ${p.name} -> ${d.name}`);
      } else if (d.path && !(policy.external_path_allowlist ?? []).includes(d.name)) {
        errors.push(`unapproved external path dependency: ${p.name} -> ${d.name}`);
      }
    }
  }
  errors.push(...graphCycles(graph));
  errors.push(...checkScopedUses(members, policy));
  return { ok:errors.length===0, errors, checkedCrates:members.length, checkedEdges };
}

function rustFiles(dir) {
  const out = [];
  if (!fs.existsSync(dir)) return out;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) out.push(...rustFiles(p));
    else if (e.name.endsWith('.rs')) out.push(p);
  }
  return out;
}

export function checkScopedUses(members, policy) {
  const errors = [];
  const scoped = policy.module_scoped_dependencies ?? {};
  for (const [crate, deps] of Object.entries(scoped)) {
    if (crate === 'description') continue;
    if (!Object.hasOwn(policy.crates, crate)) { errors.push(`module scope for unknown crate ${crate}`); continue; }
    const pkg = members.find(p => p.name === crate);
    if (!pkg) continue;
    if (typeof pkg.manifest_path !== 'string') { errors.push(`manifest_path missing for ${crate}`); continue; }
    const root = path.dirname(pkg.manifest_path);
    for (const [dep, allowed] of Object.entries(deps)) {
      if (!(policy.crates[crate] ?? []).includes(dep)) errors.push(`module scope for undeclared edge ${crate} -> ${dep}`);
      const ident = new RegExp(`\\b${dep.replace(/-/g, '_')}\\b`);
      for (const f of ['src', 'tests', 'benches', 'examples', 'build.rs'].flatMap(d => {
        const p = path.join(root, d);
        return d.endsWith('.rs') ? (fs.existsSync(p) ? [p] : []) : rustFiles(p);
      })) {
        const rel = path.relative(root, f).split(path.sep).join('/');
        const ok = allowed.some(a => rel === a || rel === `${a}.rs` || rel.startsWith(`${a}/`));
        if (!ok && ident.test(fs.readFileSync(f, 'utf8')))
          errors.push(`module-scoped dependency ${crate} -> ${dep} referenced outside ${allowed.join(', ')}: ${rel}`);
      }
    }
  }
  return errors;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    const option = name => { const i=args.indexOf(name); return i<0?null:args[i+1]; };
    const policy = load(option('--policy') ?? path.join(here,'../policy/dependencies.json'));
    let result;
    if (args.includes('--policy-only')) {
      const errors=validatePolicy(policy);
      result={ok:errors.length===0, scope:'policy-only; not an actual Rust workspace', errors, declaredCrates:Object.keys(policy.crates??{}).length};
    } else {
      const input=option('--metadata');
      if (!input) throw new Error('use --metadata <cargo-metadata.json> or --policy-only');
      result=checkMetadata(load(input),policy);
    }
    console.log(JSON.stringify(result,null,2));
    process.exitCode=result.ok?0:1;
  } catch(e) {
    console.error(JSON.stringify({ok:false, errors:[String(e.message??e)]}));
    process.exitCode=2;
  }
}
