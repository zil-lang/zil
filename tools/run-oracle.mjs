import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const result = spawnSync(process.execPath, ['--experimental-transform-types', '--import', fileURLToPath(new URL('./roam/register.mjs', import.meta.url)), fileURLToPath(new URL('../tests/oracle/run.mjs', import.meta.url)), ...process.argv.slice(2)], {cwd: root, stdio: 'inherit'});
if (result.error) console.error(result.error.message);
process.exitCode = result.error ? 1 : (result.status ?? 1);
