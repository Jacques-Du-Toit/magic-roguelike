// How long the AI takes to decide in WebAssembly, which is what the browser
// runs and is slower than the native sim.
//
//   node tools/ai-timing.mjs [runs]      (from the repo root, after `npm run wasm` in web/)
//
// It plays whole runs through the same wasm handle the web app uses, with
// the AI on both sides (the enemy's moves and the hints), and times every
// decision. Node runs the same V8 engine as Chrome and Edge.

import { readFile } from 'node:fs/promises'
import { performance } from 'node:perf_hooks'

import init, { Run } from '../web/src/wasm/engine_wasm.js'

const wasm = new URL('../web/src/wasm/engine_wasm_bg.wasm', import.meta.url)
await init({ module_or_path: await readFile(wasm) })

const runs = Number(process.argv[2] ?? 4)
/** Every decision's time in ms, and those the AI had to think about. */
const all = []
const thought = []
let duels = 0

function timed(decide, isForced) {
  const start = performance.now()
  const result = decide()
  const took = performance.now() - start
  all.push(took)
  if (!isForced) thought.push(took)
  return result
}

for (let index = 0; index < runs; index++) {
  const run = new Run(index % 2 === 0 ? 'warden' : 'firebrand', '', String(index + 1))
  for (let steps = 0; steps < 200000; steps++) {
    const view = JSON.parse(run.view())
    if (view.phase.type === 'over') break
    if (view.phase.type !== 'duel') {
      // Take what is offered and move on.
      run.apply(JSON.stringify(view.actions[0]))
      continue
    }
    const duel = JSON.parse(run.duel_view())
    if (duel.result) {
      duels++
      run.apply(JSON.stringify({ type: 'finish_duel' }))
      continue
    }
    if (duel.decision.player === 'P1') {
      // A decision with one legal action takes no thought.
      const forced = JSON.parse(run.duel_legal_actions()).length === 1
      run.duel_apply(timed(() => run.duel_suggest(), forced))
    } else {
      // The enemy's legal actions aren't shown; anything under a tenth of
      // a millisecond was a forced pass.
      const start = performance.now()
      run.duel_ai_step()
      const took = performance.now() - start
      all.push(took)
      if (took > 0.1) thought.push(took)
    }
  }
}

function summary(name, times) {
  const sorted = [...times].sort((a, b) => a - b)
  const at = (share) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * share))]
  const mean = sorted.reduce((sum, time) => sum + time, 0) / sorted.length
  const ms = (time) => time.toFixed(1).padStart(8)
  console.log(
    `${name.padEnd(22)} ${String(sorted.length).padStart(8)} ${ms(mean)} ${ms(at(0.5))} ${ms(at(0.9))} ${ms(at(0.99))} ${ms(sorted[sorted.length - 1])}`,
  )
}

console.log(`${runs} runs, ${duels} duels, in WebAssembly (ms)`)
console.log(`${''.padEnd(22)} ${'count'.padStart(8)} ${'average'.padStart(8)} ${'median'.padStart(8)} ${'90%'.padStart(8)} ${'99%'.padStart(8)} ${'worst'.padStart(8)}`)
summary('every decision', all)
summary('decisions with thought', thought)
const total = all.reduce((sum, time) => sum + time, 0)
console.log(`AI time per duel: ${(total / duels / 1000).toFixed(2)} s, both sides together`)
