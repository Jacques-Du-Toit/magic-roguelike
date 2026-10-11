import { describe, expect, it } from 'vitest'
import { AI, Duel, HUMAN, cardDefs } from '../engine/index.ts'
import { loadWasmForTests } from '../engine/testSupport.ts'
import type { Action } from '../engine/types/Action.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import { DuelController, type DuelEngine, type Sleep } from './controller.ts'

loadWasmForTests()

const instantly: Sleep = () => Promise.resolve()

function cardMap(): Map<string, CardDef> {
  return new Map(cardDefs().map((card) => [card.id, card]))
}

function newController(aiDeck: string, seed: string, sleep: Sleep = instantly) {
  const duel = new Duel('deck_b', aiDeck, seed)
  const info = { humanDeck: 'deck_b', aiDeck, seed }
  return { duel, controller: new DuelController(duel, info, cardMap(), sleep) }
}

/** A small seeded generator, so a failing game can be played again. */
function chooser(seed: number): (count: number) => number {
  let state = seed
  return (count) => {
    state = (state * 1_103_515_245 + 12_345) % 2_147_483_648
    return state % count
  }
}

/** Keeps the opening hand, whichever seat goes first. */
async function keepHand(controller: DuelController): Promise<void> {
  await controller.start()
  await controller.act({ type: 'keep' })
}

describe('DuelController', () => {
  it('starts by running until the human has a decision', async () => {
    const { duel, controller } = newController('deck_a', '7')
    expect(controller.getSnapshot().busy).toBe(true)
    await controller.start()
    const snapshot = controller.getSnapshot()
    expect(snapshot.busy).toBe(false)
    expect(snapshot.view.decision).toEqual({ type: 'mulligan', player: HUMAN })
    expect(snapshot.actions).toEqual([{ type: 'keep' }, { type: 'mulligan' }])
    expect(snapshot.view).toEqual(duel.view())
  })

  it('never leaves the human without a move until the game is over', async () => {
    for (const [aiDeck, seed] of [
      ['deck_a', 1],
      ['deck_c', 2],
      ['deck_a', 3],
      ['deck_c', 4],
    ] as const) {
      const { duel, controller } = newController(aiDeck, String(seed))
      const choose = chooser(seed)
      await controller.start()
      let turns = 0
      while (controller.getSnapshot().view.result === null) {
        const snapshot = controller.getSnapshot()
        expect(snapshot.error).toBeNull()
        expect(snapshot.busy).toBe(false)
        // The screen shows where the engine is, and it is the human's move.
        expect(snapshot.view).toEqual(duel.view())
        expect(snapshot.view.decision?.player).toBe(HUMAN)
        expect(snapshot.actions).toEqual(duel.legalActions())
        expect(snapshot.actions.length).toBeGreaterThan(0)
        await controller.act(snapshot.actions[choose(snapshot.actions.length)])
        turns += 1
        expect(turns).toBeLessThan(50_000)
      }
      const end = controller.getSnapshot()
      expect(end.busy).toBe(false)
      expect(end.actions).toEqual([])
      expect(end.log.at(-1)?.kind).toBe('result')
      expect(end.error).toBeNull()
    }
  })

  it('records the actions that replay the game from its seed', async () => {
    const { controller } = newController('deck_c', '9')
    const choose = chooser(9)
    await controller.start()
    for (let count = 0; count < 60 && controller.getSnapshot().actions.length > 0; count += 1) {
      const { actions } = controller.getSnapshot()
      await controller.act(actions[choose(actions.length)])
    }
    const first = controller.getSnapshot()

    // The history holds every action taken in the human's seat, passes made
    // for them included, so the bare engine can follow it.
    const again = new Duel('deck_b', 'deck_c', '9')
    const letAiAct = () => {
      while (again.view().decision?.player === AI) again.aiStep()
    }
    for (const action of first.history) {
      letAiAct()
      again.apply(action)
    }
    letAiAct()
    expect(first.history.length).toBeGreaterThan(60)
    expect(again.view()).toEqual(first.view)
  })

  it('passes for the human unless they hold priority', async () => {
    const quiet = newController('deck_a', '7').controller
    await keepHand(quiet)
    // Nothing can be done in an upkeep, so the first stop is later.
    const stopped = quiet.getSnapshot().view
    expect(stopped.decision?.player).toBe(HUMAN)
    expect(stopped.step === 'upkeep' && stopped.decision?.type === 'priority').toBe(false)

    const holding = newController('deck_a', '7').controller
    holding.setHoldPriority(true)
    await keepHand(holding)
    expect(holding.getSnapshot().view.step).toBe('upkeep')
    expect(holding.getSnapshot().view.decision).toEqual({ type: 'priority', player: HUMAN })
    expect(holding.getSnapshot().actions).toEqual([{ type: 'pass_priority' }])
  })

  it('ignores input while events are playing, then plays them in order', async () => {
    const waiting: (() => void)[] = []
    const manual: Sleep = () => new Promise((resolve) => waiting.push(resolve))
    const { controller } = newController('deck_a', '7', manual)
    const seen: boolean[] = []
    controller.subscribe(() => seen.push(controller.getSnapshot().busy))

    const started = controller.start()
    void controller.act({ type: 'keep' })
    expect(controller.getSnapshot().history).toEqual([])

    // Let every pause finish as soon as it starts.
    const drain = setInterval(() => waiting.splice(0).forEach((resolve) => resolve()), 0)
    await started
    clearInterval(drain)
    expect(controller.getSnapshot().busy).toBe(false)
    expect(controller.getSnapshot().history).toEqual([])
    expect(seen.at(-1)).toBe(false)
  })

  it('shows an engine error and stays usable', async () => {
    const { duel, controller } = newController('deck_a', '7')
    await controller.start()
    await controller.act({ type: 'confirm_blockers' })
    const snapshot = controller.getSnapshot()
    expect(snapshot.error).toMatch(/not a legal action/)
    expect(snapshot.busy).toBe(false)
    expect(snapshot.actions).toEqual(duel.legalActions())
    await controller.act({ type: 'keep' })
    expect(controller.getSnapshot().history[0]).toEqual({ type: 'keep' })
  })

  it('reports an opponent that fails instead of hanging', async () => {
    const duel = new Duel('deck_b', 'deck_a', '7')
    const broken: DuelEngine = {
      view: () => duel.view(),
      legalActions: () => duel.legalActions(),
      shouldAutoPass: (hold) => duel.shouldAutoPass(hold),
      apply: (action: Action) => duel.apply(action),
      aiStep: () => {
        throw new Error('the AI fell over')
      },
    }
    const info = { humanDeck: 'deck_b', aiDeck: 'deck_a', seed: '7' }
    const controller = new DuelController(broken, info, cardMap(), instantly)
    await controller.start()
    if (duel.view().decision?.player === HUMAN) await controller.act({ type: 'keep' })
    expect(controller.getSnapshot().error).toBe('the AI fell over')
    expect(controller.getSnapshot().busy).toBe(false)
    expect(controller.getSnapshot().view.decision?.player).toBe(AI)
  })

  it('stops updating once disposed', async () => {
    const { controller } = newController('deck_a', '7')
    await controller.start()
    const before = controller.getSnapshot()
    controller.dispose()
    await controller.act({ type: 'keep' })
    expect(controller.getSnapshot()).toBe(before)
  })
})
