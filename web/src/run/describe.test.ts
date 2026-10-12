import { describe, expect, it } from 'vitest'
import { cardDefs, runContent } from '../engine/index.ts'
import { loadWasmForTests } from '../engine/testSupport.ts'
import type { RunEvent } from '../engine/types/RunEvent.ts'
import { type RunNames, describeRunEvents } from './describe.ts'

loadWasmForTests()

function names(): RunNames {
  return { cards: new Map(cardDefs().map((card) => [card.id, card])), content: runContent() }
}

describe('describeRunEvents', () => {
  it('tells the story of a duel won', () => {
    const events: RunEvent[] = [
      { type: 'duel_ended', enemy: 'goblin_raid', kind: 'normal', outcome: 'won' },
      { type: 'gold_changed', change: 30, gold: 60, reason: 'duel_won' },
      { type: 'gold_changed', change: 10, gold: 70, reason: 'relic' },
    ]
    expect(describeRunEvents(events, names())).toEqual([
      'You beat Goblin Raid.',
      '+30 gold for the win.',
      '+10 gold from a relic.',
    ])
  })

  it('tells the story of a duel lost', () => {
    const events: RunEvent[] = [
      { type: 'duel_ended', enemy: 'forest_pack', kind: 'normal', outcome: 'lost' },
      { type: 'gold_changed', change: 10, gold: 40, reason: 'duel_lost' },
      { type: 'hearts_changed', change: -1, hearts: 2 },
    ]
    expect(describeRunEvents(events, names())).toEqual([
      'You lost to Forest Pack.',
      '+10 gold all the same.',
      'You lose 1 heart (2 left).',
    ])
  })

  it('names cards, relics and boons, and skips what needs no saying', () => {
    const events: RunEvent[] = [
      { type: 'travelled', node: 3, kind: 'shop' },
      { type: 'gold_changed', change: -40, gold: 5, reason: 'purchase' },
      { type: 'card_added', card: 'grizzly_bears' },
      { type: 'relic_gained', relic: 'war_banner' },
      { type: 'card_removed', card: 'shock' },
      { type: 'boon_taken', boon: 'boon_life' },
      { type: 'hearts_changed', change: 1, hearts: 3 },
      { type: 'run_ended', won: true },
    ]
    expect(describeRunEvents(events, names())).toEqual([
      'You pay 40 gold.',
      'Grizzly Bears joins your deck.',
      'You gain a relic: War Banner. Creatures you control get +1/+0.',
      'Shock leaves your deck.',
      'Boon taken: Fortitude.',
      'You restore 1 heart (3 now).',
    ])
  })

  it('tells what a treasure gave', () => {
    const gold: RunEvent[] = [
      { type: 'travelled', node: 9, kind: 'treasure' },
      { type: 'gold_changed', change: 40, gold: 130, reason: 'treasure' },
    ]
    expect(describeRunEvents(gold, names())).toEqual(['+40 gold from the treasure.'])
    const relic: RunEvent[] = [{ type: 'relic_gained', relic: 'rally_horn' }]
    expect(describeRunEvents(relic, names())).toEqual([
      'You gain a relic: Rally Horn. You start each duel with a 1/1 Soldier token on the battlefield.',
    ])
  })

  it('tells what an event cost and gave', () => {
    const events: RunEvent[] = [
      { type: 'travelled', node: 14, kind: 'event' },
      { type: 'gold_changed', change: -70, gold: 5, reason: 'event' },
      { type: 'hearts_changed', change: 1, hearts: 3 },
      { type: 'gold_changed', change: 50, gold: 55, reason: 'event' },
      { type: 'next_duel_modified', modifier: 'rivals_wager_wound' },
    ]
    expect(describeRunEvents(events, names())).toEqual([
      'You pay 70 gold.',
      'You restore 1 heart (3 now).',
      '+50 gold.',
      "Rival's Wager: You start your next duel with 5 less life.",
    ])
  })

  it('says a drawn boss duel is fought again', () => {
    const drawn = (kind: 'normal' | 'boss'): RunEvent[] => [
      { type: 'duel_ended', enemy: 'pyre_tyrant', kind, outcome: 'drawn' },
    ]
    expect(describeRunEvents(drawn('boss'), names())).toEqual([
      'The duel with The Pyre Tyrant was a draw. You fight again.',
    ])
    expect(describeRunEvents(drawn('normal'), names())).toEqual([
      'The duel with The Pyre Tyrant was a draw.',
    ])
  })
})
