import {readFileSync, mkdirSync, writeFileSync, unlinkSync, existsSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
/** Bundle every application/package dependency; only runtime builtins remain external. */
export async function freezeDriver(driver: string, root: string): Promise<string> {
    const runtime = (globalThis as any).Bun;
    if (!runtime?.build) throw new Error('snapshot compilation requires Bun; prebuilt external workers can use supervise directly');
    const snapshots = resolve(root, '.snapshots'); mkdirSync(snapshots, { recursive: true });
    const temporary = resolve(snapshots, `entry-${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}.ts`);
    const modulePath = (path: string) => JSON.stringify(path.replaceAll('\\', '/'));
    const wrapper = `import * as domain from ${modulePath(driver)};
import { execute } from ${modulePath(fileURLToPath(new URL('./core.ts', import.meta.url)))};
import { serveWorker, serveGenerator } from ${modulePath(fileURLToPath(new URL('./worker.ts', import.meta.url)))};
export const spec=domain.spec, driver=domain.driver, hasGenerate=typeof domain.generate==='function';
export const generate=(...args)=>domain.generate(...args);
export const worker=()=>serveWorker((trace,beforeStep,afterStep)=>execute(spec,{capabilities:driver.capabilities,init:driver.init.bind(driver),observe:driver.observe?.bind(driver),done:driver.done?.bind(driver),dispose:driver.dispose?.bind(driver),step:async(real,action,context)=>{await driver.step(real,action,context);afterStep(context.step);}},trace,{beforeStep,history:true}),driver.capabilities??[]);
if(import.meta.main) { if(process.argv[2]==='generate') await serveGenerator((seed,steps)=>{if(!spec||!driver)throw new Error('driver module must export spec and driver');if(!hasGenerate)throw new Error('driver module must export generate');return domain.generate(seed,steps);}); else await worker(); }`;
    writeFileSync(temporary, wrapper);
    try {
        const build = await runtime.build({ entrypoints: [temporary], target: 'bun', format: 'esm', packages: 'bundle', minify: false, write: false });
        if (!build.success) throw new Error(`snapshot build failed: ${build.logs.map(String).join('\n')}`);
        if (build.outputs.length !== 1) throw new Error('snapshot requires a single bundled entry');
        const source = (await build.outputs[0].text()).replace(/entry-\d+-\d+-[a-f0-9]+\.ts/g, 'roam-entry.ts');
        if (/\bimport\s*\((?!\s*['"])/.test(source) || /(?<![\w$])(?:__require[0-9]*|require)\s*\((?!\s*['"])/.test(source)) throw new Error('unresolved dynamic module loading is unsupported in a frozen driver');
        const hash = createHash('sha256').update(source).digest('hex'), directory = resolve(snapshots, hash);
        mkdirSync(directory, { recursive: true }); const output = resolve(directory, 'driver.mjs');
        if (!existsSync(output)) writeFileSync(output, source, { flag: 'wx' });
        else if (createHash('sha256').update(readFileSync(output)).digest('hex') !== hash) throw new Error('frozen snapshot corrupted');
        return output;
    } finally { unlinkSync(temporary); }
}
