// Wasm loading and the typed wrapper around the engine. This is the only
// module that talks to wasm (see docs/ARCHITECTURE.md §9). Data crosses the
// boundary as JSON strings and is typed with the ts-rs generated types.

import init, { engine_info } from '../wasm/engine_wasm.js'
import type { EngineInfo } from './types/EngineInfo.ts'

let loading: Promise<void> | undefined

/** Loads the wasm module. Safe to call more than once. */
export function loadEngine(): Promise<void> {
  loading ??= init().then(() => undefined)
  return loading
}

/** Name and version of the engine build. Call after `loadEngine` resolves. */
export function engineInfo(): EngineInfo {
  return JSON.parse(engine_info()) as EngineInfo
}
