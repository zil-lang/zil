import {InvalidCase, property} from '../../tools/roam/vendor/core.ts';

export const contract = 'zil-scalar-native/1';
const operators = ['+', '-', '*', '/', '%', '+%', '-%', '*%'];
const bounds = width => ({min: -(1n << BigInt(width - 1)), max: (1n << BigInt(width - 1)) - 1n});
class Trap extends Error {}

export function validate(program) {
    if (!program || ![32, 64].includes(program.width) || !['expr', 'ct', 'locals', 'loop', 'short', 'branch'].includes(program.mode)) return false;
    let nodes = 0;
    function expr(value, depth = 0) {
        if (++nodes > 256 || depth > 8 || !value || typeof value !== 'object') return false;
        if (value.kind === 'lit') {
            if (typeof value.value !== 'string' || !/^-?(0|[1-9][0-9]{0,18})$/.test(value.value)) return false;
            const n = BigInt(value.value), b = bounds(program.width);
            return n >= b.min && n <= b.max;
        }
        if (value.kind === 'neg') return expr(value.value, depth + 1);
        return value.kind === 'bin' && operators.includes(value.op) && expr(value.left, depth + 1) && expr(value.right, depth + 1);
    }
    if (!expr(program.expr)) return false;
    if (['locals', 'loop', 'branch'].includes(program.mode) && !expr(program.other)) return false;
    if (program.mode === 'locals' && !operators.includes(program.operator)) return false;
    if (program.mode === 'loop' && (!Number.isInteger(program.count) || program.count < 0 || program.count > 12 || !['+', '-', '+%', '-%'].includes(program.operator))) return false;
    if (program.mode === 'short' && (typeof program.left !== 'boolean' || !['&&', '||'].includes(program.operator))) return false;
    return true;
}

function operation(op, left, right, width) {
    const b = bounds(width);
    if (op.endsWith('%') && op.length === 2) {
        const n = op === '+%' ? left + right : op === '-%' ? left - right : left * right;
        const modulus = 1n << BigInt(width);
        return ((n - b.min) % modulus + modulus) % modulus + b.min;
    }
    if ((op === '/' || op === '%') && (right === 0n || (left === b.min && right === -1n))) throw new Trap('division');
    const n = op === '+' ? left + right : op === '-' ? left - right : op === '*' ? left * right : op === '/' ? left / right : left % right;
    if (n < b.min || n > b.max) throw new Trap('overflow');
    return n;
}

function evaluateExpr(expr, width) {
    if (expr.kind === 'lit') return BigInt(expr.value);
    if (expr.kind === 'neg') return operation('-', 0n, evaluateExpr(expr.value, width), width);
    const left = evaluateExpr(expr.left, width);
    return operation(expr.op, left, evaluateExpr(expr.right, width), width);
}

export function expected(program) {
    if (!validate(program)) throw new InvalidCase('Invalid bounded scalar program');
    try {
        const {width, mode} = program;
        let result;
        if (mode === 'short') {
            const right = () => evaluateExpr(program.expr, width) === 0n;
            result = BigInt(program.operator === '&&' ? program.left && right() : program.left || right());
        } else if (mode === 'branch') {
            result = evaluateExpr(program.expr, width) < 0n ? evaluateExpr(program.other, width) : 7n;
        } else {
            result = evaluateExpr(program.expr, width);
            if (mode === 'locals') result = operation(program.operator, result, evaluateExpr(program.other, width), width);
            if (mode === 'loop') for (let i = 0; i < program.count; i++) result = operation(program.operator, result, evaluateExpr(program.other, width), width);
        }
        return {kind: 'value', value: result.toString()};
    } catch (error) {
        if (!(error instanceof Trap)) throw error;
        return {kind: program.mode === 'ct' ? 'ct-reject' : 'trap'};
    }
}

export function source(program, mutation = false) {
    if (!validate(program)) throw new InvalidCase('Invalid bounded scalar program');
    const ty = `i${program.width}`;
    function print(expr) {
        if (expr.kind === 'lit') return expr.value.startsWith('-') ? `(${expr.value})` : expr.value;
        if (expr.kind === 'neg') return `(-${print(expr.value)})`;
        const op = mutation && expr.op === '-' ? '+' : expr.op;
        return `(${print(expr.left)} ${op} ${print(expr.right)})`;
    }
    const first = print(program.expr);
    let body;
    switch (program.mode) {
        case 'ct': body = `return #(${first})`; break;
        case 'locals': body = `let a: ${ty} = ${first}\n    let b: ${ty} = ${print(program.other)}\n    return a ${program.operator} b`; break;
        case 'loop': body = `var acc: ${ty} = ${first}\n    var index: i32 = 0\n    while index < ${program.count} {\n        acc = acc ${program.operator} ${print(program.other)}\n        index = index + 1\n    }\n    return acc`; break;
        case 'short': body = `if ${program.left} ${program.operator} zero(${first}) { return 1 } else { return 0 }`; break;
        case 'branch': body = `if negative(${first}) { return ${print(program.other)} } else { return 7 }`; break;
        default: body = `return ${first}`;
    }
    return `fn zero(x: ${ty}) -> bool => x == 0\nfn negative(x: ${ty}) -> bool => x < 0\nfn value() -> ${ty} {\n    ${body}\n}\n`;
}

export const spec = {
    contract,
    init: () => null,
    valid: (_model, action) => action.op === 'run' && validate(action.program),
    step: (_model, action, context) => {
        const value = expected(action.program);
        context.record({oracle: value});
        return value;
    },
    check: (model, observed) => {
        if (model === null) return;
        property('scalar.native-equals-model', JSON.stringify(model) === JSON.stringify(observed), `Independent BigInt result differs: expected ${JSON.stringify(model)}, actual ${JSON.stringify(observed)}`);
    },
};

const lit = value => ({kind: 'lit', value: String(value)});
const bin = (op, left, right) => ({kind: 'bin', op, left: typeof left === 'object' ? left : lit(left), right: typeof right === 'object' ? right : lit(right)});
export function pins() {
    const programs = [];
    for (const width of [32, 64]) {
        const {min, max} = bounds(width);
        for (const expr of [lit(min), lit(max), bin('+', max, 1), bin('-', min, 1), bin('*', max, 2), bin('/', 7, 0), bin('/', min, -1), bin('%', min, -1), bin('+%', max, 1), bin('-%', min, 1), bin('*%', max, 3), {kind: 'neg', value: lit(min)}, bin('/', -17, 5), bin('%', -17, 5)]) {
            programs.push({width, mode: 'expr', expr}, {width, mode: 'ct', expr});
        }
        for (const operator of ['&&', '||']) for (const left of [false, true]) programs.push({width, mode: 'short', operator, left, expr: bin('/', 1, 0)});
        programs.push({width, mode: 'loop', expr: lit(max), other: lit(1), operator: '+%', count: 3});
        programs.push({width, mode: 'loop', expr: lit(8), other: bin('/', 1, 0), operator: '+', count: 0});
        programs.push({width, mode: 'branch', expr: lit(1), other: bin('/', 1, 0)});
        programs.push({width, mode: 'branch', expr: lit(-1), other: lit(min)});
    }
    return programs;
}

export function generate(seed, count) {
    let state = (seed >>> 0) || 0x9e3779b9;
    function next(n) { state ^= state << 13; state ^= state >>> 17; state ^= state << 5; return (state >>> 0) % n; }
    const modes = ['expr', 'ct', 'locals', 'loop', 'short', 'branch'];
    const programs = [];
    for (let i = 0; i < count; i++) {
        const width = next(2) ? 64 : 32, b = bounds(width);
        function expr(depth = 0) {
            if (depth >= 3 || next(3) === 0) return lit(next(8) === 0 ? [b.min, b.max][next(2)] : next(33) - 16);
            if (next(6) === 0) return {kind: 'neg', value: expr(depth + 1)};
            return bin(operators[next(operators.length)], expr(depth + 1), expr(depth + 1));
        }
        const mode = modes[next(modes.length)], program = {width, mode, expr: expr()};
        if (['locals', 'loop', 'branch'].includes(mode)) program.other = expr();
        if (mode === 'locals') program.operator = operators[next(operators.length)];
        if (mode === 'loop') { program.count = next(7); program.operator = ['+', '-', '+%', '-%'][next(4)]; }
        if (mode === 'short') { program.left = Boolean(next(2)); program.operator = ['&&', '||'][next(2)]; }
        programs.push(program);
    }
    return programs;
}

export const makeCase = (programs, seed) => ({format: 'roam/1', contract, ...(seed === undefined ? {} : {seed}), steps: programs.map(program => ({op: 'run', program}))});
export const mutationCase = () => makeCase([{width: 32, mode: 'expr', expr: lit(2)}, {width: 64, mode: 'expr', expr: bin('-', 9, 4)}, {width: 32, mode: 'expr', expr: lit(8)}], 4);
