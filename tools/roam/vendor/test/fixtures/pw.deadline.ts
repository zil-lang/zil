/** Deliberately unresolved real locator, to exercise PW deadline cleanup in a child. */
import type {Page} from '@playwright/test';
export const spec = {contract: 'pw-deadline/1', init: () => 0, step: (model: number) => model};
export const generate = () => ({format: 'roam/1', contract: spec.contract, steps: [{op: 'hang'}]});
export const createDriver = (page: Page, url: string) => ({capabilities: ['pw-real-input'],
    init: async () => {await page.goto(url); return page;},
    step: async () => {await page.locator('#never-created').waitFor({timeout: 0});},
});
