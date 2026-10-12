import { describe, expect, it } from 'vitest'
import { loadWasmForTests } from './testSupport.ts'
import {
  AI,
  Duel,
  HUMAN,
  Run,
  cardDefs,
  deckDefs,
  engineInfo,
  randomSeed,
  runContent,
} from './index.ts'

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
    // The whole pool crosses; its exact size is checked on the Rust side.
    expect(cards.length).toBeGreaterThanOrEqual(53)
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
    expect(deckDefs().map((deck) => deck.id)).toEqual(
      expect.arrayContaining(['deck_a', 'deck_b', 'deck_c']),
    )
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

  it('starts with the emblem asked for, or the usual one, and shows the commander in a duel', () => {
    const usual = new Run('warden', '', '1')
    expect(usual.view().emblem).toBe('emblem_welcome')
    usual.free()
    expect(() => new Run('warden', 'nothing', '1')).toThrow('there is no starting emblem `nothing`')

    const run = new Run('warden', 'emblem_fury', '1')
    const view = run.view()
    expect(view.emblem).toBe('emblem_fury')
    const travel = view.actions[0]
    // Arriving offers the feats; the duel starts once two are picked.
    const offered = run.apply(travel).view
    expect(offered.phase.type).toBe('feats')
    expect(() => run.duel.view()).toThrow('no duel is being played')
    expect(offered.actions[0]).toEqual({ type: 'choose_feats', feats: [0, 1] })
    const started = run.apply(offered.actions[0]).view
    expect(started.feats.map((feat) => feat.feat)).toEqual(
      offered.phase.type === 'feats' ? offered.phase.offered.slice(0, 2) : [],
    )
    expect(run.duel.feats()).toEqual(started.feats)
    const [you, enemy] = run.duel.view().players
    expect(you.emblems.map((emblem) => emblem.name)).toEqual(['Battle Fury'])
    expect(you.commander?.def).toBe('jasmine_boreal')
    expect(you.commander?.cost).toBe('{3}{G}{W}')
    expect(you.commander?.object).not.toBeNull()
    // The first duel is against an easy enemy, which has no commander.
    expect(enemy.commander).toBeNull()
    run.free()
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

describe('Run', () => {
  it('brings the run content across', () => {
    const content = runContent()
    expect(content.planeswalkers.map((planeswalker) => planeswalker.id)).toEqual([
      'warden',
      'firebrand',
    ])
    expect(content.wounds.map((wound) => wound.id)).toEqual(['festering_wound'])
    expect(content.feats).toHaveLength(9)
    expect(content.feat_rewards[0]).toMatchObject({ points: 0, choices: 2 })
    expect(content.emblems.map((emblem) => emblem.id)).toEqual(['emblem_welcome', 'emblem_fury'])
    expect(content.planeswalkers.map((planeswalker) => planeswalker.commander)).toEqual([
      'jasmine_boreal',
      'boris_devilboon',
    ])
    expect(content.enemies.some((enemy) => enemy.tier === 'boss')).toBe(true)
  })

  it('starts with the emblem asked for, or the usual one, and shows the commander in a duel', () => {
    const usual = new Run('warden', '', '1')
    expect(usual.view().emblem).toBe('emblem_welcome')
    usual.free()
    expect(() => new Run('warden', 'nothing', '1')).toThrow('there is no starting emblem `nothing`')

    const run = new Run('warden', 'emblem_fury', '1')
    const view = run.view()
    expect(view.emblem).toBe('emblem_fury')
    const travel = view.actions[0]
    // Arriving offers the feats; the duel starts once two are picked.
    const offered = run.apply(travel).view
    expect(offered.phase.type).toBe('feats')
    expect(() => run.duel.view()).toThrow('no duel is being played')
    expect(offered.actions[0]).toEqual({ type: 'choose_feats', feats: [0, 1] })
    const started = run.apply(offered.actions[0]).view
    expect(started.feats.map((feat) => feat.feat)).toEqual(
      offered.phase.type === 'feats' ? offered.phase.offered.slice(0, 2) : [],
    )
    expect(run.duel.feats()).toEqual(started.feats)
    const [you, enemy] = run.duel.view().players
    expect(you.emblems.map((emblem) => emblem.name)).toEqual(['Battle Fury'])
    expect(you.commander?.def).toBe('jasmine_boreal')
    expect(you.commander?.cost).toBe('{3}{G}{W}')
    expect(you.commander?.object).not.toBeNull()
    // The first duel is against an easy enemy, which has no commander.
    expect(enemy.commander).toBeNull()
    run.free()
  })

  it('throws the engine message for bad input', () => {
    expect(() => new Run('nobody', '', '1')).toThrow('there is no planeswalker `nobody`')
    const run = new Run('warden', '', '1')
    expect(() => run.apply({ type: 'leave' })).toThrow(/not a legal action/)
    expect(() => run.duel.view()).toThrow('no duel is being played')
    expect(run.inputs).toEqual([])
    run.free()
  })

  it('plays a whole run: the last action on offer each time, passing through duels', () => {
    const run = new Run('firebrand', 'emblem_welcome', '5')
    let view = run.view()
    expect(view.phase.type).toBe('map')
    let steps = 0
    let duels = 0
    while (view.phase.type !== 'over') {
      steps += 1
      expect(steps).toBeLessThan(200_000)
      expect(view.actions.length).toBeGreaterThan(0)
      if (view.actions[0].type === 'concede') {
        // A duel is on: play it by passing and confirming.
        const duel = run.duel.view()
        if (duel.decision?.player === AI) {
          run.duel.aiStep()
        } else {
          const actions = run.duel.legalActions()
          const easy = actions.find((action) =>
            ['pass_priority', 'confirm_attackers', 'confirm_blockers', 'keep'].includes(action.type),
          )
          run.duel.apply(easy ?? actions[0])
        }
        view = run.view()
        continue
      }
      const action = view.actions[view.actions.length - 1]
      const update = run.apply(action)
      duels += update.events.filter((event) => event.type === 'duel_started').length
      view = update.view
    }
    expect(duels).toBeGreaterThan(1)
    expect(view.actions).toEqual([])
    expect(run.inputs.length).toBeGreaterThan(duels)
    run.free()
  })
})