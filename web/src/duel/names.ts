// Names for things on the stack.

import type { CardDef } from '../engine/types/CardDef.ts'
import type { StackItemView } from '../engine/types/StackItemView.ts'

/** The card a spell is, or the card an ability comes from. */
export function stackItemDef(
  item: StackItemView,
  cards: ReadonlyMap<string, CardDef>,
): CardDef | undefined {
  if (item.kind.type === 'emblem') return undefined
  return cards.get(item.kind.type === 'spell' ? item.kind.card.def : item.kind.ability.def)
}

/** The name of a spell or ability on the stack. */
export function stackItemName(item: StackItemView, cards: ReadonlyMap<string, CardDef>): string {
  if (item.kind.type === 'emblem') return `'s ability`
  const name = stackItemDef(item, cards)?.name ?? 'Unknown'
  return item.kind.type === 'spell' ? name : `${name}'s ability`
}
