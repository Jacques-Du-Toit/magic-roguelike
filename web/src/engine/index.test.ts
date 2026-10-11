import { readFileSync } from 'node:fs'
import { beforeAll, describe, expect, it } from 'vitest'
import { initSync } from '../wasm/engine_wasm.js'
import { engineInfo } from './index.ts'

// Node can't fetch the .wasm file the way the browser does, so the test
// loads the bytes from disk. Run `npm run wasm` first.
beforeAll(() => {
  const wasm = readFileSync(new URL('../wasm/engine_wasm_bg.wasm', import.meta.url))
  initSync({ module: wasm })
})

describe('engineInfo', () => {
  it('returns the engine name and version from Rust', () => {
    const info = engineInfo()
    expect(info.name).toBe('engine')
    expect(info.version).toMatch(/^\d+\.\d+\.\d+$/)
  })
})
