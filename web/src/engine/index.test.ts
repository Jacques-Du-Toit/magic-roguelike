import { describe, expect, it } from 'vitest'
import { loadWasmForTests } from './testSupport.ts'
import { AI, Duel, HUMAN, cardDefs, deckDefs, engineInfo, randomSeed } from './index.ts'

loadWasmForTests()

describe('engineInfo', () => {
  it('returns the engine name and version from Rust', () => {
    const info = engineInfo()
    expect(info.name).toBe('engine')
    expect(info.version).toMatch(/^\d+\.\d+\.\d+$/)
  })
})

describe('card and deck data', () => {
  it('brings every card definition across', () => {
    const cards = cardDefs()
    expect(cards).toHaveLength(29)
    const nighthawk = cards.find((card) => card.id === 'vampire_nighthawk')
    expect(nighthawk).toMatchObject({
      name: 'Vampire Nighthawk',
      mana_cost: '{1}{B}{B}',
      power: 2,
      toughness: 3,
    })
    expect(nighthawk?.oracle_text).toContain('Lifelink')
  })

  it('brings the decklists across', () => {
    expect(deckDefs().map((deck) => deck.id)).toEqual(['deck_a', 'deck_b', 'deck_c'])
  })
})

describe('randomSeed', () => {
  it('is a u64 in decimal that the engine accepts', () => {
    const seed = randomSeed()
    expect(seed).toMatch(/^\d+$/)
    expect(BigInt(seed) < 2n ** 64n).toBe(true)
    const duel = new Duel('deck_b', 'deck_a', seed)
    expect(duel.seed).toBe(seed)
    duel.free()
  })
})

describe('Duel', () => {
  it('keeps a seed beyond the range of a JS number intact', () => {
    const duel = new Duel('deck_b', 'deck_a', '18446744073709551615')
    expect(duel.seed).toBe('18446744073709551615')
    duel.free()
  })

  it('throws the engine message for bad input', () => {
    expect(() => new Duel('deck_z', 'deck_a', '1')).toThrow('no deck `deck_z`')
    expect(() => new Duel('deck_b', 'deck_a', '1.5')).toThrow(/seed/)
    const duel = new Duel('deck_b', 'deck_a', '1')
    expect(() => duel.apply({ type: 'confirm_blockers' })).toThrow()
    duel.free()
  })

  it('starts with the mulligan decision and hides the AI hand', () => {
    const duel = new Duel('deck_b', 'deck_a', '7')
    const view = duel.view()
    expect(view.viewer).toBe(HUMAN)
    expect(view.decision?.type).toBe('mulligan')
    const [human, ai] = view.players
    expect(human.hand.every((card) => card.face !== null)).toBe(true)
    expect(ai.hand).toHaveLength(7)
    expect(ai.hand.every((card) => card.face === null)).toBe(true)
    duel.free()
  })

  it('plays a whole game: the human takes the first legal action each time', () => {
    const duel = new Duel('deck_b', 'deck_c', '5')
    let view = duel.view()
    let steps = 0
    while (view.result === null) {
      expect(view.decision).not.toBeNull()
      if (view.decision?.player === AI) {
        expect(duel.legalActions()).toEqual([])
        view = duel.aiStep().view
      } else {
        const actions = duel.legalActions()
        expect(actions.length).toBeGreaterThan(0)
        // Passing first keeps the game moving; otherwise answer the decision.
        const pass = actions.find((action) => action.type === 'pass_priority')
        const confirm = actions.find(
          (action) => action.type === 'confirm_attackers' || action.type === 'confirm_blockers',
        )
        const update = duel.apply(pass ?? confirm ?? actions[0])
        for (const event of update.events) {
          if (event.type === 'card_drawn' && event.player === AI) {
            expect(event.card.face).toBeNull()
          }
        }
        view = update.view
      }
      steps += 1
      expect(steps).toBeLessThan(20_000)
    }
    expect(view.result).toMatchObject({ type: 'win', player: AI })
    expect(view.decision).toBeNull()
    expect(() => duel.aiStep()).toThrow()
    duel.free()
  })
})
