import {defineConfig, devices} from '@playwright/test';
export default defineConfig({
    testDir: '../../..', testMatch: ['ui/roam.pw.ts'],
    outputDir: '../../../test-results/roam-pw', retries: 0, workers: 1,
    timeout: 30_000, reporter: 'list',
    use: {...devices['Desktop Chrome'], trace: 'off', ...(process.env.ROAM_PW_CHANNEL ? {channel: process.env.ROAM_PW_CHANNEL} : {})},
    // Fixture assets are immutable and served from Page.route; no live server is needed.
});
