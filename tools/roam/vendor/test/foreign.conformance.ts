/** Explicit optional-tool gate: bun roam/test/foreign.conformance.ts [python executable]. */
import {fileURLToPath} from 'node:url';
import {readFileSync} from 'node:fs';
import {supervise} from '../worker';
import {decodeCase, execute, property, type RoamSpec, type RoamDriver} from '../core';

const trace=decodeCase(readFileSync(new URL('../corpus/counter.json',import.meta.url),'utf8'));
const spec: RoamSpec<number>={
    contract: trace.contract, init: initial => initial as number,
    step: (m,a) => a.op==='add'? m+(a.value as number):m,
    check: (m,o,c) => {property('counter.value',m===o); let a=c.case.steps[c.step]; if(a?.op==='check') property('counter.value',o===a.expected);},
};
const driver: RoamDriver<{value: number}>={
    init: n => ({value: n as number}),step: (r,a) => {if(a.op==='add') r.value+=a.value as number;},observe: r => r.value,
};
for(const input of [trace,{...trace,steps: [...trace.steps,{op: 'check',expected: 99}]}]) {
    const native=await execute(spec,driver,input);
    const foreign=await supervise({command: process.argv[2]??'python',args: [fileURLToPath(new URL('./fixtures/counter.py',import.meta.url))],trace: input,timeoutMs: 5000});
    if(native.status!==foreign.result.status||native.failure?.propertyId!==foreign.result.failure?.propertyId) throw new Error(JSON.stringify({native,foreign}));
    console.log(JSON.stringify({contract: trace.contract,status: native.status,property: native.failure?.propertyId,foreign: 'python-jsonl'}));
}
