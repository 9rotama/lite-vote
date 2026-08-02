import { defineConfig, devices } from '@playwright/test';

const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? 'http://127.0.0.1:3000';
const usesLocalHttp = baseURL.startsWith('http://');

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: 'list',
  use: {
    baseURL,
    trace: 'retain-on-failure',
  },
  webServer: process.env.PLAYWRIGHT_BASE_URL
    ? undefined
    : {
        command: 'just e2e-server',
        url: `${baseURL}/readyz`,
        reuseExistingServer: false,
        timeout: 120_000,
      },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'firefox',
      use: { ...devices['Desktop Firefox'] },
    },
    {
      name: 'webkit',
      // WebKit does not retain Secure cookies from the local HTTP test server.
      // Run the authenticated flows against Chromium/Firefox locally; setting
      // an HTTPS base URL also enables the primary flows for WebKit.
      testMatch: usesLocalHttp ? /smoke\.spec\.ts/ : undefined,
      use: { ...devices['Desktop Safari'] },
    },
  ],
});
