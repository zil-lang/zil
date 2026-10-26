export * from './data';

/** Portable replay protocol. Numbers are finite IEEE-754 values; -0 is rejected. */
export type Json=null|boolean|number|string|Json[]|{[key: string]: Json;};
export type Action={op: string;[key: string]: Json;};
export interface RoamCase {
    format: 'roam/1';
    /** Stable specification identity including its semantic version. */
    contract: string;
    initial?: Json;
    steps: Action[];
    requires?: string[];
    schedule?: Json[];
    seed?: number;
    meta?: {[key: string]: Json;};
}
export interface PropertyIssue {propertyId: string; message: string; expected?: Json; actual?: Json;}
export interface RunIssue {phase: string; step?: number; name: string; message: string;}
export interface HistoryEntry {phase: string; step?: number; action?: Action; value?: Json;}
export interface RunResult {
    status: 'pass'|'fail'|'invalid'|'error'|'unsupported'|'incomplete';
    steps: number;
    failure?: PropertyIssue;
    issues: RunIssue[];
    history?: HistoryEntry[];
    reason?: string;
}

/** Reject lossy JSON conversion, cycles, accessors, symbols and custom instances. */
export function assertJson(value: unknown): asserts value is Json {
    const active=new Set<object>();
    const visit=(v: unknown,path: string): void => {
        if(v===null||typeof v==='string'||typeof v==='boolean') return;
        if(typeof v==='number'&&Number.isFinite(v)&&!Object.is(v,-0)) return;
        if(typeof v!=='object'||v===null) throw new TypeError(`Non-JSON value at ${path}`);
        if(active.has(v)) throw new TypeError(`JSON cycle at ${path}`);
        const arr=Array.isArray(v);
        if(!arr&&Object.getPrototypeOf(v)!==Object.prototype&&Object.getPrototypeOf(v)!==null)
            throw new TypeError(`Non-plain object at ${path}`);
        active.add(v);
        const keys=Reflect.ownKeys(v);
        for(const key of keys) {
            if(arr&&key==='length') continue;
            if(typeof key!=='string') throw new TypeError(`Symbol key at ${path}`);
            const d=Object.getOwnPropertyDescriptor(v,key)!;
            if(!d.enumerable||!('value' in d)) throw new TypeError(`Hidden/accessor property at ${path}.${key}`);
            if(arr&&(!/^(0|[1-9]\d*)$/.test(key)||Number(key)>=(v as unknown[]).length))
                throw new TypeError(`Extra array property at ${path}.${key}`);
            visit(d.value,`${path}.${key}`);
        }
        if(arr&&keys.length-1!==(v as unknown[]).length) throw new TypeError(`Sparse array at ${path}`);
        active.delete(v);
    };
    visit(value,'$');
}

export function validateCase(value: unknown): asserts value is RoamCase {
    assertJson(value);
    if(!value||Array.isArray(value)||typeof value!=='object') throw new TypeError('Expected roam case');
    const c=value as {[key: string]: Json;};
    if(c.format!=='roam/1'||typeof c.contract!=='string'||!c.contract)
        throw new TypeError('Unsupported case format or missing contract');
    if(!Array.isArray(c.steps)||c.steps.some(a => !a||Array.isArray(a)||typeof a!=='object'||typeof a.op!=='string'||!a.op))
        throw new TypeError('Expected action objects with non-empty op');
    if(c.requires!==undefined&&(!Array.isArray(c.requires)||c.requires.some(x => typeof x!=='string')))
        throw new TypeError('Expected capability names');
    if(c.schedule!==undefined&&!Array.isArray(c.schedule)) throw new TypeError('Expected schedule array');
    if(c.seed!==undefined&&(typeof c.seed!=='number'||!Number.isSafeInteger(c.seed))) throw new TypeError('Expected integer seed');
    if(c.meta!==undefined&&(!c.meta||Array.isArray(c.meta)||typeof c.meta!=='object')) throw new TypeError('Expected metadata object');
}
export function encodeCase(value: RoamCase): string {validateCase(value); return JSON.stringify(value);}
export function decodeCase(text: string): RoamCase {const value: unknown=JSON.parse(text); validateCase(value); return value;}
export function cloneCase(value: RoamCase): RoamCase {return decodeCase(encodeCase(value));}


export class PropertyFailure extends Error {
    constructor(public propertyId: string,message: string,public expected?: Json,public actual?: Json) {super(message); this.name='PropertyFailure';}
}
export class InvalidCase extends Error {constructor(message: string) {super(message); this.name='InvalidCase';} }
export class Unsupported extends Error {constructor(message: string) {super(message); this.name='Unsupported';} }
export function property(id: string,condition: unknown,message=id): asserts condition {
    if(!id) throw new TypeError('propertyId must not be empty');
    if(!condition) throw new PropertyFailure(id,message);
}
type Awaitable<T>=T|Promise<T>;
export interface RunContext {
    readonly case: RoamCase;
    readonly step: number;
    /** Register partial-init resources immediately; all callbacks run in reverse order. */
    defer(cleanup: () => Awaitable<void>): void;
    record(value: Json): void;
}
export interface RoamSpec<M=unknown> {
    contract: string;
    init(initial: Json|undefined,context: RunContext): Awaitable<M>;
    valid?(model: M,action: Action,context: RunContext): Awaitable<boolean>;
    resources?(model: M): Iterable<string>;
    dependencies?(action: Action): {requires?: string[]; provides?: string[]; releases?: string[];};
    step(model: M,action: Action,context: RunContext): Awaitable<M>;
    check?(model: M,observed: Json|undefined,context: RunContext): Awaitable<void>;
    done?(model: M,observed: Json|undefined,context: RunContext): Awaitable<void>;
}
export interface RoamDriver<R=unknown> {
    capabilities?: readonly string[];
    init(initial: Json|undefined,context: RunContext): Awaitable<R>;
    step(real: R,action: Action,context: RunContext): Awaitable<void>;
    observe?(real: R,context: RunContext): Awaitable<Json>;
    done?(real: R,context: RunContext): Awaitable<void>;
    dispose?(real: R|undefined,context: RunContext): Awaitable<void>;
}
export interface ExecuteOptions {
    maxSteps?: number;
    budgetMs?: number;
    signal?: {readonly aborted: boolean;};
    now?: () => number;
    history?: boolean;
    beforeStep?(step: number,action: Action): Awaitable<void>;
}
const issue=(e: unknown,phase: string,step: number): RunIssue => ({phase,step,name: e instanceof Error? e.name:'ThrownValue',message: e instanceof Error? e.message:String(e)});

/** Strict replay: consumes only the supplied actions; no generator or implicit retry. */
export async function execute<M,R>(spec: RoamSpec<M>,driver: RoamDriver<R>,input: RoamCase,options: ExecuteOptions={}): Promise<RunResult> {
    const result: RunResult={status: 'pass',steps: 0,issues: []};
    if(options.history) result.history=[];
    let testCase: RoamCase;
    try {
        testCase=cloneCase(input);
        const freeze=(value: unknown): void => {if(value&&typeof value==='object') {for(const v of Object.values(value)) freeze(v); Object.freeze(value);} };
        freeze(testCase);
    } catch(e) {result.status='invalid'; result.issues.push(issue(e,'case',-1)); return result;}
    let real: R|undefined,model: M,index=-1,phase='init';
    const cleanups: (() => Awaitable<void>)[]=[];
    const context: RunContext={
        case: testCase,get step() {return index;},
        defer(fn) {cleanups.push(fn);},
        record(value) {assertJson(value); result.history?.push({phase: 'record',step: index,value: JSON.parse(JSON.stringify(value))});},
    };
    const now=options.now??Date.now,start=now();
    const checkBudget=() => {
        const reason=options.signal?.aborted? 'aborted':options.budgetMs!==undefined&&now()-start>=options.budgetMs? 'time-budget':undefined;
        if(reason) {result.status='incomplete'; result.reason=reason; return false;} return true;
    };
    const observe=async () => {
        if(!driver.observe) return undefined;
        phase='observe'; const value=await driver.observe(real as R,context); assertJson(value);
        context.record(value); return value;
    };
    try {
        if(spec.contract!==testCase.contract) throw new Unsupported(`Contract ${testCase.contract}`);
        for(const cap of testCase.requires??[]) if(!driver.capabilities?.includes(cap)) throw new Unsupported(`Capability ${cap}`);
        // An opaque schedule cannot be replayed merely by declaring a capability.
        // A future scheduler adapter must define explicit consumption/acknowledgement.
        if(testCase.schedule?.length) throw new Unsupported('Explicit schedule replay is not implemented');
        if(!checkBudget()) return result;
        phase='model-init'; model=await spec.init(testCase.initial,context);
        if(!checkBudget()) return result;
        phase='driver-init'; real=await driver.init(testCase.initial,context);
        if(!checkBudget()) return result;
        const resources=new Set(spec.resources?.(model)??[]);
        let observed=await observe(); phase='check'; await spec.check?.(model,observed,context);
        for(index=0;index<testCase.steps.length;index++) {
            if(!checkBudget()) break;
            if(options.maxSteps!==undefined&&result.steps>=options.maxSteps) {result.status='incomplete'; result.reason='step-budget'; break;}
            const action=testCase.steps[index]; phase='validate';
            const deps=spec.dependencies?.(action);
            for(const id of [...deps?.requires??[],...deps?.releases??[]]) if(!resources.has(id)) throw new InvalidCase(`Missing resource ${id}`);
            for(const id of deps?.provides??[]) if(resources.has(id)) throw new InvalidCase(`Duplicate resource ${id}`);
            if(spec.valid&&!await spec.valid(model,action,context)) throw new InvalidCase(`Precondition at step ${index}`);
            if(!checkBudget()) break;
            phase='before-step'; await options.beforeStep?.(index,action);
            if(!checkBudget()) break;
            result.history?.push({phase: 'call',step: index,action: JSON.parse(JSON.stringify(action))});
            phase='driver-step'; await driver.step(real,action,context);
            result.steps++;
            result.history?.push({phase: 'complete',step: index});
            phase='model-step'; model=await spec.step(model,action,context);
            for(const id of deps?.releases??[]) resources.delete(id);
            for(const id of deps?.provides??[]) resources.add(id);
            observed=await observe(); phase='check'; await spec.check?.(model,observed,context);
        }
        if(result.status==='pass'&&checkBudget()) {
            phase='driver-done'; await driver.done?.(real,context);
            observed=await observe(); phase='done'; await spec.done?.(model,observed,context);
            checkBudget();
        }
    } catch(e) {
        result.issues.push(issue(e,phase,index));
        if(e instanceof PropertyFailure&&e.propertyId) {
            result.status='fail';
            const failure: PropertyIssue={propertyId: e.propertyId,message: e.message};
            if(e.expected!==undefined) failure.expected=e.expected;
            if(e.actual!==undefined) failure.actual=e.actual;
            try {assertJson(failure); result.failure=JSON.parse(JSON.stringify(failure));}
            catch(encodingError) {result.status='error'; result.issues.push(issue(encodingError,'failure-encoding',index));}
        } else result.status=e instanceof InvalidCase? 'invalid':e instanceof Unsupported? 'unsupported':'error';
    } finally {
        const cleanup=async (fn: () => Awaitable<void>,name: string) => {
            try {await fn();} catch(e) {result.issues.push(issue(e,name,index)); result.status='error';}
        };
        await cleanup(async () => {await driver.dispose?.(real,context);},'dispose');
        for(const fn of cleanups.reverse()) await cleanup(fn,'deferred-dispose');
        if(result.status==='pass') checkBudget();
    }
    return result;
}
