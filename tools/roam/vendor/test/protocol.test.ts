import {test, expect} from 'bun:test';
import {assertJson, decodeCase, encodeCase, type RoamCase} from '../core';
const sample: RoamCase = {format: 'roam/1', contract: 'counter/1', steps: [{op: 'add', value: 1}]};
test('roam-protocol-01 roundtrip preserves explicit trace and schedule', () => {
    const c = {...sample, schedule: [{deliver: 'task-1'}], initial: {value: 0}};
    expect(decodeCase(encodeCase(c))).toEqual(c);
});
test('roam-protocol-02 rejects JSON silent-loss values', () => {
    const sparse = Array(1), cyclic: any = {}; cyclic.self = cyclic;
    const symbol = {[Symbol('hidden')]: 1};
    for (const value of [undefined, NaN, Infinity, -0, 1n, ()=>0, sparse, cyclic, symbol, new Date(), {x: undefined}, Object.defineProperty({}, 'x', {get() {throw Error('must not invoke');}, enumerable: true})])
        expect(() => assertJson(value)).toThrow();
});
test('roam-protocol-03 rejects incompatible or malformed cases', () => {
    for (const value of [{...sample, format: 'roam/2'}, {...sample, steps: [{}]}, {...sample, requires: [3]}])
        expect(() => decodeCase(JSON.stringify(value))).toThrow();
});
