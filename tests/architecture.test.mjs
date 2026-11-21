import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import {checkMetadata,validatePolicy} from '../tools/check-architecture.mjs';
import {writeReport} from './report.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const policy=JSON.parse(fs.readFileSync(path.join(root,'policy/dependencies.json'),'utf8'));
const clone=x=>JSON.parse(JSON.stringify(x));
const mk=(entries)=>({workspace_members:entries.map(([name])=>name+'#0'), packages:entries.map(([name,deps])=>({id:name+'#0',name,manifest_path:path.join(root,'crates',name,'Cargo.toml'),dependencies:deps.map(d=>typeof d==='string'?{name:d,kind:null}:d)}))});
const results=[];
function test(name, fn){ fn(); results.push({name,passed:true}); }
test('policy graph is acyclic',()=>assert.deepEqual(validatePolicy(policy),[]));
test('approved real-shaped metadata passes',()=>assert.equal(checkMetadata(mk([['zil-base',[]],['zil-ir',['zil-base']],['zil-semantics',['zil-ir','zil-base']],['zil-check',['zil-ir','zil-base','zil-semantics']]]),policy).ok,true));
test('checker cannot import solver',()=>assert.equal(checkMetadata(mk([['zil-solve',[]],['zil-check',['zil-solve']]]),policy).ok,false));
test('renaming solver cannot bypass',()=>assert.equal(checkMetadata(mk([['zil-solve',[]],['zil-check',[{name:'zil-solve',rename:'shared_rules'}]]]),policy).ok,false));
test('dev dependency still checked',()=>assert.equal(checkMetadata(mk([['zil-typeck',[]],['zil-check',[{name:'zil-typeck',kind:'dev'}]]]),policy).ok,false));
test('unknown workspace crate rejected',()=>assert.equal(checkMetadata(mk([['zil-magic',[]]]),policy).ok,false));
test('unknown internal dependency rejected',()=>assert.equal(checkMetadata(mk([['zil-ir',['zil-private-checker']]]),policy).ok,false));
test('external crates allowed, hidden path packages rejected',()=>{
 assert.equal(checkMetadata(mk([['zil-base',[{name:'smallvec',source:'registry+example'}]]]),policy).ok,true);
 assert.equal(checkMetadata(mk([['zil-base',[{name:'hidden',path:'../hidden'}]]]),policy).ok,false);
});
test('empty or malformed result fails closed',()=>{
 assert.equal(checkMetadata(mk([]),policy).ok,false);
 assert.equal(checkMetadata({},policy).ok,false);
});
test('cycle rejected even when permitted by forged policy',()=>{
 const p=clone(policy);p.crates['zil-base']=['zil-ir'];
 assert.notEqual(validatePolicy(p).length,0);
});
test('solver cannot import checker; checker cannot import solver',()=>{
  assert.equal(checkMetadata(mk([['zil-check',[]],['zil-solve',['zil-check']]]),policy).ok,false);
  assert.equal(checkMetadata(mk([['zil-solve',[]],['zil-check',['zil-solve']]]),policy).ok,false);
  assert.equal(checkMetadata(mk([['zil-typeck',[]],['zil-check',['zil-typeck']]]),policy).ok,false);
});
test('semantics kernel depends only on base and ir',()=>assert.equal(checkMetadata(mk([['zil-check',[]],['zil-semantics',['zil-check']]]),policy).ok,false));
test('module-scoped syntax use outside allowed modules rejected',()=>{
  const dir=fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()),'zil-arch-'));
  fs.mkdirSync(path.join(dir,'src/fidelity'),{recursive:true});
  fs.writeFileSync(path.join(dir,'src/fidelity/fields.rs'),'use zil_syntax::Ast;\n');
  fs.writeFileSync(path.join(dir,'src/lib.rs'),'mod fidelity;\n');
  const meta=mk([['zil-check',['zil-syntax']],['zil-syntax',[]]]);
  meta.packages[0].manifest_path=path.join(dir,'Cargo.toml');
  assert.equal(checkMetadata(meta,policy).ok,true);
  fs.writeFileSync(path.join(dir,'src/certificate.rs'),'fn f(_: zil_syntax::Ast) {}\n');
  assert.equal(checkMetadata(meta,policy).ok,false);
  delete meta.packages[0].manifest_path;
  assert.equal(checkMetadata(meta,policy).ok,false);
  fs.rmSync(dir,{recursive:true});
});
test('missing member manifest rejected',()=>assert.equal(checkMetadata({packages:[],workspace_members:['missing']},policy).ok,false));
const report={scope:'synthetic Cargo-metadata fixture tests only; not Rust code or ZIL semantic validation',node:process.version,total:results.length,passed:results.length,results};
writeReport('architecture-tests.json',report);
console.log(JSON.stringify(report,null,2));
