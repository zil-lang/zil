import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {checkMetadata,validatePolicy} from '../tools/check-architecture.mjs';
import {writeReport} from './report.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const policy=JSON.parse(fs.readFileSync(path.join(root,'policy/dependencies.json'),'utf8'));
const clone=x=>JSON.parse(JSON.stringify(x));
const mk=(entries)=>({workspace_members:entries.map(([name])=>name+'#0'), packages:entries.map(([name,deps])=>({id:name+'#0',name,dependencies:deps.map(d=>typeof d==='string'?{name:d,kind:null}:d)}))});
const results=[];
function test(name, fn){ fn(); results.push({name,passed:true}); }
test('policy graph is acyclic',()=>assert.deepEqual(validatePolicy(policy),[]));
test('approved real-shaped metadata passes',()=>assert.equal(checkMetadata(mk([['zil-base',[]],['zil-ir',['zil-base']],['zil-verify',['zil-ir','zil-base']]]),policy).ok,true));
test('verifier cannot import analyzer',()=>assert.equal(checkMetadata(mk([['zil-analyze',[]],['zil-verify',['zil-analyze']]]),policy).ok,false));
test('renaming analyzer cannot bypass',()=>assert.equal(checkMetadata(mk([['zil-analyze',[]],['zil-verify',[{name:'zil-analyze',rename:'shared_rules'}]]]),policy).ok,false));
test('dev dependency still checked',()=>assert.equal(checkMetadata(mk([['zil-types',[]],['zil-verify',[{name:'zil-types',kind:'dev'}]]]),policy).ok,false));
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
test('missing member manifest rejected',()=>assert.equal(checkMetadata({packages:[],workspace_members:['missing']},policy).ok,false));
const report={scope:'synthetic Cargo-metadata fixture tests only; not Rust code or ZIL semantic validation',node:process.version,total:results.length,passed:results.length,results};
writeReport('architecture-tests.json',report);
console.log(JSON.stringify(report,null,2));
