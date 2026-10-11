// Names for things on the stack.

import type { CardDef } from '../engine/types/CardDef.ts'
import type { StackItemView } from '../engine/types/StackItemView.ts'

/** The card a spell is, or the card an ability comes from. */
export function stackItemDef(
  item: StackItemView,
  cards: ReadonlyMap<string, CardDef>,
): CardDef | undefined {
  switch (item.kind.type) {
    case 'emblem':
      return undefined
    case 'spell':
      return cards.get(item.kind.card.def)
    case 'ability':
      return cards.get(item.kind.ability.def)
    case 'keyword':
      return cards.get(item.kind.def)
  }
}

/** The name of a spell or ability on the stack. */
export function stackItemName(item: StackItemView, cards: ReadonlyMap<string, CardDef>): string {
  if (item.kind.type === 'emblem') return `${item.kind.name}'s ability`
  const name = stackItemDef(item, cards)?.name ?? 'Unknown'
  if (item.kind.type === 'spell') return name
  // "Monastery Swiftspear's prowess".
  if (item.kind.type === 'keyword') return `${name}'s ${item.kind.keyword.replaceAll('_', ' ')}`
  return `${name}'s ability`
}