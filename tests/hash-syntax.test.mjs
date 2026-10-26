import assert from 'node:assert/strict';
import {writeReport} from './report.mjs';
import {dispatch,parseAttribute,attributePlan} from '../tools/hash-syntax.mjs';
const results=[];
const test=(name,fn)=>{try{fn();results.push({name,pass:true});}catch(e){results.push({name,pass:false,error:String(e.stack)});}};
const error=(fn,code)=>assert.throws(fn,e=>e.code===code);
for(const [text,ctx,kind] of [
 ['#(square(12))','expression','evaluate'],['#([1,2])','expression','evaluate'],['#{ let n = 3; yield n }','expression','evaluate-block'],
 ['#[repr(C)]','item','attribute'],['#if (ok) {}','item','select'],['#for (x in list) {}','statement','repeat'],['#emit derive(T)','item','emit'],['#N: usize','generic-parameter','comptime-parameter'],
 ['# /* comment */ (1)','expression','evaluate'],['#[doc="[a] ]"]','item','attribute'],['#(f("\\\"", [1,2]))','expression','evaluate']
])test('prefix '+text,()=>assert.equal(dispatch(text,ctx).kind,kind));
for(const [text,ctx,code] of [
 ['#[1,2]','expression','E_ATTR_POSITION'],['#[]','item','E_ATTR_EMPTY'],['#call()','expression','E_HASH_FORM'],['#![cfg(x)]','item','E_INNER_ATTRIBUTE'],['#\n(1)','expression','E_HASH_NEWLINE'],['#(x]','expression','E_DELIMITER'],['#()','expression','E_CT_EMPTY'],['[repr(C)]','item','E_HASH_REQUIRED'],['#if(x){}','expression','E_CT_POSITION']
])test('reject '+text,()=>error(()=>dispatch(text,ctx),code));
for(const [a,target]of [['#[repr(C)]','struct'],['#[align(16)]','struct'],['#[derive(Debug, Eq)]','struct'],['#[inline]','fn'],['#[inline(never)]','fn'],['#[cfg(target.os == "windows")]','fn'],['#[doc = "a"]','field'],['#[test]','fn']])test('attribute '+a,()=>assert.ok(parseAttribute(a,target)));
for(const [a,target,code]of [['#[unknown]','fn','E_ATTR_UNKNOWN'],['#[repr(C)]','fn','E_ATTR_TARGET'],['#[test(1)]','fn','E_ATTR_ARGS'],['#[align(3)]','struct','E_ATTR_ARGS'],['#[derive(Eq, Eq)]','struct','E_ATTR_DUPLICATE'],['#[inline = "always"]','fn','E_ATTR_FORMAT'],['#[doc = 3]','fn','E_ATTR_ARGS']])test('bad attribute '+a,()=>error(()=>parseAttribute(a,target),code));
test('phase order independent of source order',()=>assert.deepEqual(attributePlan(['#[align(16)]','#[derive(Debug)]','#[cfg(true)]'],'struct').map(a=>a.name),['cfg','derive','align']));
test('duplicate inline rejected',()=>error(()=>attributePlan(['#[inline]','#[inline(never)]']),'E_ATTR_DUPLICATE'));
test('repeated cfg preserved',()=>assert.equal(attributePlan(['#[cfg(true)]','#[cfg(false)]']).length,2));
test('qualified extension is explicit',()=>assert.equal(parseAttribute('#[ui.component]','fn',[{name:'ui.component',kind:'syntax',phase:1,targets:['fn'],repeat:'error',args:'none'}]).kind,'syntax'));
test('character bracket does not close meta delimiter',()=>assert.equal(dispatch("#(']')").kind,'evaluate'));
test('character opener does not open meta delimiter',()=>assert.equal(dispatch("#('[')").kind,'evaluate'));
test('nested block comments are balanced',()=>assert.equal(dispatch('#(1 /* [ /* } */ ] */ + 2)').kind,'evaluate'));
test('generic marker unavailable in expression',()=>error(()=>dispatch('#N','expression'),'E_HASH_FORM'));
test('numeric attribute cannot masquerade as array',()=>error(()=>parseAttribute('#[1,2]','fn'),'E_ATTR_NAME'));
test('attributes cannot grant noalias',()=>error(()=>parseAttribute('#[noalias]','fn'),'E_ATTR_UNKNOWN'));
test('duplicate derives across brackets rejected',()=>error(()=>attributePlan(['#[derive(Eq)]','#[derive(Debug, Eq)]'],'struct'),'E_ATTR_DUPLICATE'));
test('distinct derives across brackets preserved',()=>assert.equal(attributePlan(['#[derive(Eq)]','#[derive(Debug)]'],'struct').length,2));
const report={scope:'Only hash dispatch, balanced input, attribute schema/targets/phase-order; NOT full parser/evaluator/type safety',total:results.length,passed:results.filter(r=>r.pass).length,results};
writeReport('hash-syntax.json',report);
console.log(JSON.stringify(report,null,2));if(report.passed!==report.total)process.exitCode=1;
