import {defineConfig, devices} from '@playwright/test';
export default defineConfig({
    testDir: '../../..', testMatch: ['ed/roam.integration.pw.ts'],
    outputDir: '../../../test-results/roam-editor', retries: 0, workers: 1,
    timeout: 30_000, reporter: 'list',
    use: {...devices['Desktop Chrome'], trace: 'off', ...(process.env.ROAM_PW_CHANNEL ? {channel: process.env.ROAM_PW_CHANNEL} : {})},
    // An in-memory immutable route supplies all fixture assets. No server/HMR process.
});
