import {execute} from '../../tools/roam/vendor/core.ts';
import {serveWorker} from '../../tools/roam/vendor/worker.ts';
import {driver, spec} from './session.mjs';

const [, , binary, fault = 'none'] = process.argv;
if (!binary) throw new Error('missing frozen compiler session bridge');
const actual = driver(binary, fault);
await serveWorker((trace, beforeStep, afterStep) => execute(spec, {
    ...actual,
    async step(real, action, context) {
        await actual.step(real, action, context);
        afterStep(context.step);
    },
}, trace, {history: true, beforeStep, maxSteps: 256, budgetMs: 10_000}));
