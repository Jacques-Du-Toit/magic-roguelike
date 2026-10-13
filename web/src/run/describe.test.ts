import { describe, expect, it } from 'vitest'
import { cardDefs, runContent } from '../engine/index.ts'
import { loadWasmForTests } from '../engine/testSupport.ts'
import type { RunEvent } from '../engine/types/RunEvent.ts'
import { type RunNames, describeRunEvents, featRewardText, featText } from './describe.ts'

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
      {
        type: 'feats_scored',
        scores: [
          { feat: 'unbroken', score: 2, value: 16 },
          { feat: 'swift', score: 1, value: 8 },
        ],
        points: 3,
      },
      { type: 'gold_changed', change: 15, gold: 85, reason: 'feats' },
    ]
    expect(describeRunEvents(events, names())).toEqual([
      'You beat Goblin Raid.',
      '+30 gold for the win.',
      '+10 gold from a relic.',
      'Your feats scored 3 points: Unbroken 2, Swift 1.',
      '+15 gold for your feats.',
    ])
  })

  it('writes a feat with its tiers and a reward in words', () => {
    const { content } = names()
    const unbroken = content.feats.find((feat) => feat.id === 'unbroken')
    expect(unbroken && featText(unbroken)).toBe('Never fall below 12 / 15 / 18 life.')
    expect(content.feat_rewards.map(featRewardText)).toEqual([
      '2 cards to choose from',
      '3 cards to choose from',
      '3 cards to choose from, rarer ones more often, and 15 gold',
      '4 cards to choose from, rarer ones more often, and 30 gold',
    ])
  })

  it('tells the story of a duel lost', () => {
    const events: RunEvent[] = [
      { type: 'duel_ended', enemy: 'forest_pack', kind: 'normal', outcome: 'lost' },
      { type: 'gold_changed', change: 10, gold: 40, reason: 'duel_lost' },
      { type: 'wound_taken', wound: 'festering_wound' },
    ]
    const lines = describeRunEvents(events, names())
    expect(lines.slice(0, 2)).toEqual(['You lost to Forest Pack.', '+10 gold all the same.'])
    expect(lines[2]).toMatch(/^You take a wound: Festering Wound\. You start each duel with/)
  })

  it('names cards, relics and wounds, and skips what needs no saying', () => {
    const events: RunEvent[] = [
      { type: 'travelled', node: 3, kind: 'shop' },
      { type: 'gold_changed', change: -40, gold: 5, reason: 'purchase' },
      { type: 'card_added', card: 'grizzly_bears' },
      { type: 'relic_gained', relic: 'war_banner' },
      { type: 'card_removed', card: 'shock' },
      { type: 'wound_healed', wound: 'festering_wound' },
      { type: 'feats_offered', enemy: 'goblin_raid', kind: 'normal' },
      { type: 'run_ended', won: true },
    ]
    expect(describeRunEvents(events, names())).toEqual([
      'You pay 40 gold.',
      'Grizzly Bears joins your deck.',
      'You gain a relic: War Banner. Creatures you control get +1/+0.',
      'Shock leaves your deck.',
      'Your Festering Wound is healed.',
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
      'You gain a relic: Rally Horn. The first time a creature token enters under your control each turn, create a 1/1 Soldier token.',
    ])
  })

  it('tells what an event cost and gave', () => {
    const events: RunEvent[] = [
      { type: 'travelled', node: 14, kind: 'event' },
      { type: 'gold_changed', change: -70, gold: 5, reason: 'event' },
      { type: 'wound_healed', wound: 'festering_wound' },
      { type: 'gold_changed', change: 50, gold: 55, reason: 'event' },
      { type: 'next_duel_modified', modifier: 'rivals_wager_wound' },
    ]
    expect(describeRunEvents(events, names())).toEqual([
      'You pay 70 gold.',
      'Your Festering Wound is healed.',
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
