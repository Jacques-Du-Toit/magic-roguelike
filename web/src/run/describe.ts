// Wording for the run: what just happened, and the names of things.
// Display only: nothing here decides anything about the run.

import type { CardDef } from '../engine/types/CardDef.ts'
import type { FeatDef } from '../engine/types/FeatDef.ts'
import type { FeatReward } from '../engine/types/FeatReward.ts'
import type { ModifierDef } from '../engine/types/ModifierDef.ts'
import type { NodeKind } from '../engine/types/NodeKind.ts'
import type { RunContent } from '../engine/types/RunContent.ts'
import type { RunEvent } from '../engine/types/RunEvent.ts'

/** Lookups over the run content, for showing names and texts. */
export interface RunNames {
  cards: ReadonlyMap<string, CardDef>
  content: RunContent
}

export function cardName(names: RunNames, id: string): string {
  return names.cards.get(id)?.name ?? id
}

export function enemyName(names: RunNames, id: string): string {
  return names.content.enemies.find((enemy) => enemy.id === id)?.name ?? id
}

export function relicDef(names: RunNames, id: string): ModifierDef | undefined {
  return names.content.relics.find((relic) => relic.id === id)
}

export function woundDef(names: RunNames, id: string): ModifierDef | undefined {
  return names.content.wounds.find((wound) => wound.id === id)
}

export function featDef(names: RunNames, id: string): FeatDef | undefined {
  return names.content.feats.find((feat) => feat.id === id)
}

/** A feat's text with its tiers written in: "Never fall below 12 / 15 / 18 life." */
export function featText(feat: FeatDef): string {
  return feat.text.replace('{n}', feat.tiers.join(' / '))
}

/** What a won duel pays for a row of the feats' rewards, in words. */
export function featRewardText(reward: FeatReward): string {
  const cards = `${plural(reward.choices, 'card')} to choose from`
  const rarer = reward.rarity_steps > 0 ? ', rarer ones more often' : ''
  const gold = reward.gold > 0 ? `, and ${reward.gold} gold` : ''
  return `${cards}${rarer}${gold}`
}

/** A modifier an event leaves for the next duel. */
export function nextDuelDef(names: RunNames, id: string): ModifierDef | undefined {
  for (const event of names.content.events) {
    for (const option of event.options) {
      for (const effect of option.effects) {
        if (effect.type === 'next_duel' && effect.modifier.id === id) return effect.modifier
      }
    }
  }
  return undefined
}

/** A node kind in words and a symbol. The symbol never stands alone. */
export const NODE_LABEL: Record<NodeKind, { name: string; symbol: string; about: string }> = {
  duel: {
    name: 'Duel',
    symbol: '⚔',
    about: 'Fight an opponent, with two feats to try for. Win for gold and a card.',
  },
  elite: {
    name: 'Elite',
    symbol: '☠',
    about: 'A stronger opponent. Win for a relic, gold and a card.',
  },
  rest: { name: 'Rest', symbol: '⛺', about: 'Heal a wound or remove a card.' },
  shop: { name: 'Shop', symbol: '◈', about: 'Buy cards, a land or a relic, or pay to remove a card.' },
  draft: { name: 'Draft', symbol: '❖', about: 'Open a pack of five cards and keep up to two.' },
  treasure: { name: 'Treasure', symbol: '✦', about: 'Take one of two relics, or gold instead.' },
  event: { name: 'Event', symbol: '?', about: 'A chance meeting: take the trade it offers, or walk on.' },
  boss: { name: 'Boss', symbol: '♛', about: 'Beat the boss to win the act. Lose and the run is over.' },
}

function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? '' : 's'}`
}

/** One line for an event, or null if it needs no saying. */
export function describeRunEvent(event: RunEvent, names: RunNames): string | null {
  switch (event.type) {
    case 'duel_ended': {
      const enemy = enemyName(names, event.enemy)
      if (event.outcome === 'won') return `You beat ${enemy}.`
      if (event.outcome === 'lost') return `You lost to ${enemy}.`
      return event.kind === 'boss'
        ? `The duel with ${enemy} was a draw. You fight again.`
        : `The duel with ${enemy} was a draw.`
    }
    case 'wound_taken': {
      const wound = woundDef(names, event.wound)
      return wound ? `You take a wound: ${wound.name}. ${wound.text}` : 'You take a wound.'
    }
    case 'wound_healed': {
      const wound = woundDef(names, event.wound)
      return wound ? `Your ${wound.name} is healed.` : 'A wound is healed.'
    }
    case 'feats_scored': {
      const each = event.scores
        .map((score) => `${featDef(names, score.feat)?.name ?? score.feat} ${score.score}`)
        .join(', ')
      return `Your feats scored ${plural(event.points, 'point')}: ${each}.`
    }
    case 'gold_changed': {
      const amount = `${Math.abs(event.change)} gold`
      switch (event.reason) {
        case 'duel_won':
          return `+${amount} for the win.`
        case 'duel_lost':
          return `+${amount} all the same.`
        case 'duel_drawn':
          return `+${amount} for the draw.`
        case 'relic':
          return `+${amount} from a relic.`
        case 'purchase':
          return `You pay ${amount}.`
        case 'removal':
          return `You pay ${amount} for the removal.`
        case 'treasure':
          return `+${amount} from the treasure.`
        case 'event':
          return event.change < 0 ? `You pay ${amount}.` : `+${amount}.`
        case 'feats':
          return `+${amount} for your feats.`
      }
      return null
    }
    case 'relic_gained': {
      const relic = relicDef(names, event.relic)
      return relic ? `You gain a relic: ${relic.name}. ${relic.text}` : 'You gain a relic.'
    }
    case 'card_added':
      return `${cardName(names, event.card)} joins your deck.`
    case 'card_removed':
      return `${cardName(names, event.card)} leaves your deck.`
    case 'next_duel_modified': {
      const modifier = nextDuelDef(names, event.modifier)
      return modifier ? `${modifier.name}: ${modifier.text}` : 'Your next duel is changed.'
    }
    case 'travelled':
    case 'feats_offered':
    case 'duel_started':
    case 'run_ended':
      return null
  }
}

/** The lines to show for a batch of events. */
export function describeRunEvents(events: RunEvent[], names: RunNames): string[] {
  return events.flatMap((event) => describeRunEvent(event, names) ?? [])
}
