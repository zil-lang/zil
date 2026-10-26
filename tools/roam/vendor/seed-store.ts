let _req=(import.meta as any).require;
if(!_req&&!(globalThis as any).document&&(globalThis as any).process?.versions?.node) {
    _req=(await import('node:module')).createRequire(import.meta.url);
}

let fs: any=(globalThis as any).document||!_req? undefined:_req('node:fs');
/** Legacy seed archives: absent is empty; corruption and I/O failures are explicit. */
export function seedLoad(file: string): any[] {
    if(!fs) throw new Error('Seed archives require the Node/Bun storage host');
    let text: string;
    try {text=fs.readFileSync(file,'utf8');} catch(e: any) {if(e.code==='ENOENT') return []; throw e;}
    return text.split('\n').filter(x => x.trim()).map((row,i) => {
        try {let value=JSON.parse(row); if(!value||typeof value!=='object'||Array.isArray(value)) throw new Error('expected archive record'); return value;}
        catch(e) {throw new Error(`Invalid seed archive ${file}:${i+1}`,{cause: e});}
    });
}
export function seedSave(file: string,rows: object[]) {
    if(!rows.length) return;
    if(!fs) throw new Error('Seed archives require the Node/Bun storage host');
    fs.appendFileSync(file,rows.map(r => JSON.stringify(r)).join('\n')+'\n');
}
