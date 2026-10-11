import { defineConfig } from '@playwright/test'

// End-to-end tests drive the real app in a headless browser. Run `npm run
// wasm` first, then `npm run e2e`.
const PORT = 5199

export default defineConfig({
  testDir: 'e2e',
  testMatch: '*.e2e.ts',
  timeout: 120_000,
  fullyParallel: true,
  reporter: 'list',
  use: {
    baseURL: `http://localhost:${PORT}`,
    viewport: { width: 1440, height: 900 },
  },
  webServer: {
    command: `npm run dev -- --port ${PORT} --strictPort`,
    port: PORT,
    reuseExistingServer: true,
  },
})
