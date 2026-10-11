import { describe, expect, it } from 'vitest'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { GameEvent } from '../engine/types/GameEvent.ts'
import { type NameContext, TIMING, describeEvent, planPresentation, targetKey } from './present.ts'

const names: Record<number, string> = { 1: 'Hill Giant', 2: 'Wind Drake', 3: 'Shock' }
const context: NameContext = {
  viewer: 'P1',
  objectName: (object) => names[object],
  cards: new Map([
    ['wind_drake', { name: 'Wind Drake' } as CardDef],
    ['shock', { name: 'Shock' } as CardDef],
  ]),
}
const drake = { object: 2, face: { card: 20, def: 'wind_drake' } }
const hidden = { object: 9, face: null }

function text(event: GameEvent): string | undefined {
  return describeEvent(event, context)?.text
}

describe('describeEvent', () => {
  it('names the viewer "You" and the other player "Opponent"', () => {
    expect(text({ type: 'card_drawn', player: 'P1', card: drake })).toBe('You draw Wind Drake')
    expect(text({ type: 'card_drawn', player: 'P2', card: hidden })).toBe('Opponent draws a card')
    expect(text({ type: 'life_changed', player: 'P1', change: -3, life: 17 })).toBe(
      'You lose 3 life (now 17)',
    )
    expect(text({ type: 'life_changed', player: 'P2', change: 2, life: 22 })).toBe(
      'Opponent gains 2 life (now 22)',
    )
  })

  it('names targets, attackers and blockers', () => {
    expect(
      text({
        type: 'spell_cast',
        player: 'P2',
        card: { object: 3, face: { card: 30, def: 'shock' } },
        targets: [{ type: 'object', object: 2 }],
      }),
    ).toBe('Opponent casts Shock targeting Wind Drake')
    expect(
      text({ type: 'damage_dealt', source: 1, target: { type: 'player', player: 'P1' }, amount: 3 }),
    ).toBe('Hill Giant deals 3 damage to you')
    expect(
      text({
        type: 'blockers_declared',
        player: 'P1',
        blocks: [{ attacker: 1, blockers: [2] }],
      }),
    ).toBe('You block Hill Giant with Wind Drake')
    expect(text({ type: 'blockers_declared', player: 'P2', blocks: [] })).toBe(
      'Opponent declares no blocks',
    )
  })

  it('describes bounces, keywords gained and scries', () => {
    expect(
      text({ type: 'permanent_returned_to_hand', player: 'P2', permanent: 2, card: drake }),
    ).toBe("Opponent's Wind Drake returns to its owner's hand")
    expect(text({ type: 'keyword_granted', permanent: 1, keyword: 'first_strike' })).toBe(
      'Hill Giant gains first strike until end of turn',
    )
    expect(text({ type: 'token_created', player: 'P2', card: drake })).toBe(
      'Opponent creates a Wind Drake token',
    )
    expect(text({ type: 'counters_added', permanent: 1, amount: 1, counters: 2 })).toBe(
      'Hill Giant gets a +1/+1 counter',
    )
    expect(text({ type: 'counters_added', permanent: 1, amount: 2, counters: 2 })).toBe(
      'Hill Giant gets 2 +1/+1 counters',
    )
    expect(
      text({
        type: 'ability_activated',
        player: 'P2',
        source: 1,
        ability: 9,
        targets: [{ type: 'player', player: 'P1' }],
      }),
    ).toBe("Opponent uses Hill Giant's ability targeting you")
    expect(text({ type: 'permanent_sacrificed', player: 'P1', permanent: 2 })).toBe(
      'You sacrifice Wind Drake',
    )
    expect(text({ type: 'card_milled', player: 'P1', card: drake })).toBe(
      'Your Wind Drake goes from the library to the graveyard',
    )
    expect(text({ type: 'card_returned_to_hand', player: 'P2', card: drake })).toBe(
      'Opponent returns Wind Drake from the graveyard to hand',
    )
    expect(text({ type: 'card_kept_on_top', player: 'P1' })).toBe('You keep a card on top')
    expect(text({ type: 'card_kept_on_top', player: 'P2' })).toBe('Opponent keeps a card on top')
    // A bounced permanent is shown leaving before the view changes.
    const plan = planPresentation(
      [{ type: 'permanent_returned_to_hand', player: 'P2', permanent: 2, card: drake }],
      context,
    )
    expect(plan.beats.map((beat) => beat.effects.leaving)).toEqual([[2]])
  })

  it('marks turns and the result, and skips bookkeeping events', () => {
    expect(describeEvent({ type: 'turn_began', turn: 3, player: 'P1' }, context)).toEqual({
      text: 'Turn 3: your turn',
      kind: 'turn',
    })
    expect(
      describeEvent(
        { type: 'game_over', result: { type: 'win', player: 'P2', reason: 'life_zero' } },
        context,
      ),
    ).toEqual({ text: 'Opponent wins the game', kind: 'result' })
    expect(describeEvent({ type: 'step_began', step: 'draw' }, context)).toBeNull()
    expect(describeEvent({ type: 'permanent_tapped', permanent: 1 }, context)).toBeNull()
  })
})

describe('planPresentation', () => {
  it('has nothing to play for a step of a declaration', () => {
    expect(planPresentation([], context)).toEqual({ beats: [], log: [], holdMs: 0 })
  })

  it('plays combat damage, then the deaths, before the view changes', () => {
    const events: GameEvent[] = [
      { type: 'step_began', step: 'combat_damage' },
      { type: 'damage_dealt', source: 1, target: { type: 'object', object: 2 }, amount: 3 },
      { type: 'damage_dealt', source: 2, target: { type: 'object', object: 1 }, amount: 2 },
      { type: 'damage_dealt', source: 1, target: { type: 'player', player: 'P1' }, amount: 1 },
      { type: 'life_changed', player: 'P1', change: -1, life: 19 },
      { type: 'creature_died', player: 'P1', permanent: 2, card: drake },
    ]
    const plan = planPresentation(events, context)
    expect(plan.beats).toEqual([
      {
        ms: TIMING.impactBeat,
        effects: {
          floats: [
            { key: 'o:2', text: '-3', tone: 'harm' },
            { key: 'o:1', text: '-2', tone: 'harm' },
            // Damage to a player shows once, as the life change.
            { key: 'p:P1', text: '-1', tone: 'harm' },
          ],
          leaving: [],
        },
      },
      { ms: TIMING.leavingBeat, effects: { floats: [], leaving: [2] } },
    ])
    expect(plan.log.map((entry) => entry.text)).toEqual([
      'Hill Giant deals 3 damage to Wind Drake',
      'Wind Drake deals 2 damage to Hill Giant',
      'Hill Giant deals 1 damage to you',
      'You lose 1 life (now 19)',
      'Your Wind Drake dies',
    ])
  })

  it('keeps separate rounds of damage apart', () => {
    const hit = (object: number): GameEvent => ({
      type: 'damage_dealt',
      source: 1,
      target: { type: 'object', object },
      amount: 1,
    })
    const died: GameEvent = { type: 'creature_died', player: 'P1', permanent: 2, card: drake }
    const kinds = planPresentation([hit(2), died, hit(3)], context).beats.map((beat) => beat.ms)
    expect(kinds).toEqual([TIMING.impactBeat, TIMING.leavingBeat, TIMING.impactBeat])
  })

  it("holds the opponent's moves on screen longer than the viewer's own", () => {
    const cast = (player: 'P1' | 'P2'): GameEvent => ({
      type: 'spell_cast',
      player,
      card: drake,
      targets: [],
    })
    expect(planPresentation([cast('P2')], context).holdMs).toBe(TIMING.hold.spellCast)
    expect(planPresentation([cast('P1')], context).holdMs).toBe(TIMING.hold.own)
    // The longest hold of the batch wins.
    const turn: GameEvent = { type: 'turn_began', turn: 2, player: 'P2' }
    expect(planPresentation([turn, cast('P2')], context).holdMs).toBe(TIMING.hold.spellCast)
  })
})

describe('targetKey', () => {
  it('tells players and objects apart', () => {
    expect(targetKey({ type: 'player', player: 'P2' })).toBe('p:P2')
    expect(targetKey({ type: 'object', object: 2 })).toBe('o:2')
  })
})
