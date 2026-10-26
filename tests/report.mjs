import fs from 'node:fs';
import path from 'node:path';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));

// Placement only: report values and test verdicts belong to each producer.
export function writeReport(name, report, {file} = {}) {
    if (typeof name !== 'string' || path.basename(name) !== name || !name.endsWith('.json')) throw new Error('Report name must be a JSON filename');
    const directory = process.env.ZIL_TEST_REPORT_DIR ?? path.join(root, 'out/runs/tool-reports', `${Date.now()}-${randomUUID()}`);
    const output = file === undefined ? path.resolve(directory, name) : path.resolve(file);
    fs.mkdirSync(path.dirname(output), {recursive: true});
    fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n', {flag: 'wx'});
    return output;
}
