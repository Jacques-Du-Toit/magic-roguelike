// Test-only: loading wasm under Vitest.

import { readFileSync } from 'node:fs'
import { beforeAll } from 'vitest'
import { initSync } from '../wasm/engine_wasm.js'

/**
 * Node can't fetch the .wasm file the way the browser does, so tests load
 * the bytes from disk. Run `npm run wasm` first.
 */
export function loadWasmForTests(): void {
  beforeAll(() => {
    const wasm = readFileSync(new URL('../wasm/engine_wasm_bg.wasm', import.meta.url))
    initSync({ module: wasm })
  })
}
