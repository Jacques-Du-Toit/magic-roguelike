// Wording for the run: what just happened, and the names of things.
// Display only: nothing here decides anything about the run.

import type { CardDef } from '../engine/types/CardDef.ts'
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

export function boonDef(names: RunNames, id: string): ModifierDef | undefined {
  return names.content.boons.find((boon) => boon.id === id)
}

/** A node kind in words and a symbol. The symbol never stands alone. */
export const NODE_LABEL: Record<NodeKind, { name: string; symbol: string; about: string }> = {
  duel: { name: 'Duel', symbol: '⚔', about: 'Fight an opponent. Win for gold and a card.' },
  elite: {
    name: 'Elite',
    symbol: '☠',
    about: 'A stronger opponent. Win for a relic, gold and a card.',
  },
  rest: { name: 'Rest', symbol: '⛺', about: 'Restore a heart or remove a card.' },
  shop: { name: 'Shop', symbol: '◈', about: 'Buy cards, a land or a relic, or pay to remove a card.' },
  boss: { name: 'Boss', symbol: '♛', about: 'Beat the boss to win the act.' },
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
    case 'hearts_changed':
      return event.change < 0
        ? `You lose ${plural(-event.change, 'heart')} (${event.hearts} left).`
        : `You restore ${plural(event.change, 'heart')} (${event.hearts} now).`
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
    case 'boon_taken': {
      const boon = boonDef(names, event.boon)
      return boon ? `Boon taken: ${boon.name}.` : 'Boon taken.'
    }
    case 'travelled':
    case 'duel_started':
    case 'run_ended':
      return null
  }
}

/** The lines to show for a batch of events. */
export function describeRunEvents(events: RunEvent[], names: RunNames): string[] {
  return events.flatMap((event) => describeRunEvent(event, names) ?? [])
}
