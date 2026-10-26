import assert from 'node:assert/strict';
import fs from 'node:fs';
import crypto from 'node:crypto';
import {expected, validate} from './domain.mjs';
import {writeReport} from '../report.mjs';

const lit = value => ({kind: 'lit', value: String(value)});
const bin = (op, a, b) => ({kind: 'bin', op, left: lit(a), right: lit(b)});
const program = (expr, width = 32, mode = 'expr') => ({width, mode, expr});
const value = n => ({kind: 'value', value: String(n)});
// Hand-calculated values, not a second call through the model's arithmetic helpers.
const cases = [
    ['i32 maximum', program(lit('2147483647')), value('2147483647')],
    ['i64 minimum', program(lit('-9223372036854775808'), 64), value('-9223372036854775808')],
    ['checked add', program(bin('+', '2147483647', 1)), {kind: 'trap'}],
    ['checked sub', program(bin('-', '-2147483648', 1)), {kind: 'trap'}],
    ['checked multiply', program(bin('*', '2147483647', 2)), {kind: 'trap'}],
    ['signed division truncates toward zero', program(bin('/', -17, 5)), value(-3)],
    ['signed remainder follows dividend', program(bin('%', -17, 5)), value(-2)],
    ['division by zero', program(bin('/', 1, 0)), {kind: 'trap'}],
    ['division minimum by minus one', program(bin('/', '-9223372036854775808', -1), 64), {kind: 'trap'}],
    ['remainder minimum by minus one', program(bin('%', '-2147483648', -1)), {kind: 'trap'}],
    ['wrapping i32 add', program(bin('+%', '2147483647', 1)), value('-2147483648')],
    ['wrapping i64 sub', program(bin('-%', '-9223372036854775808', 1), 64), value('9223372036854775807')],
    ['wrapping i64 multiply', program(bin('*%', '9223372036854775807', 3), 64), value('9223372036854775805')],
    ['compile time arithmetic is rejection', program(bin('/', 1, 0), 32, 'ct'), {kind: 'ct-reject'}],
    ['false && skips trap', {...program(bin('/', 1, 0), 32, 'short'), left: false, operator: '&&'}, value(0)],
    ['true || skips trap', {...program(bin('/', 1, 0), 64, 'short'), left: true, operator: '||'}, value(1)],
    ['true && evaluates trap', {...program(bin('/', 1, 0), 32, 'short'), left: true, operator: '&&'}, {kind: 'trap'}],
    ['zero loop skips body', {...program(lit(8), 32, 'loop'), other: bin('/', 1, 0), count: 0, operator: '+'}, value(8)],
    ['loop repeats', {...program(lit(-3), 32, 'loop'), other: lit(2), count: 4, operator: '+'}, value(5)],
    ['untaken branch skips trap', {...program(lit(1), 32, 'branch'), other: bin('/', 1, 0)}, value(7)],
];
const results = [];
for (const [name, input, answer] of cases) {
    try { assert.deepEqual(expected(input), answer); results.push({name, status: 'Pass'}); }
    catch (error) { results.push({name, status: 'Fail', message: error.message}); }
}
for (const [name, input] of [
    ['reject out of range', program(lit('2147483648'))],
    ['reject unsafe JS numeric literal', program({kind: 'lit', value: 9007199254740993})],
    ['reject unbounded loop', {...program(lit(0), 32, 'loop'), other: lit(1), count: 1000000, operator: '+'}],
]) {
    try { assert.equal(validate(input), false); assert.throws(() => expected(input)); results.push({name, status: 'Pass'}); }
    catch (error) { results.push({name, status: 'Fail', message: error.message}); }
}
const report = {schema_version: 1, scope: 'Independent model anchors and invalid-domain boundaries; not native compiler execution', status: results.every(x => x.status === 'Pass') ? 'Pass' : 'Fail', executed: results.length, assertions: cases.length + 6, node: process.version, model_sha256: crypto.createHash('sha256').update(fs.readFileSync(new URL('./domain.mjs', import.meta.url))).digest('hex'), results};
const file = writeReport('model-check.json', report);
console.log(JSON.stringify({status: report.status, executed: report.executed, assertions: report.assertions, report_path: file}));
process.exitCode = report.status === 'Pass' ? 0 : 1;
