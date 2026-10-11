import { describe, expect, it } from 'vitest'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { StackItemView } from '../engine/types/StackItemView.ts'
import { stackItemDef, stackItemName } from './names.ts'

const cards = new Map([
  ['shock', { name: 'Shock' } as CardDef],
  ['monastery_swiftspear', { name: 'Monastery Swiftspear' } as CardDef],
])
const item = (kind: StackItemView['kind']): StackItemView => ({
  id: 9,
  controller: 'P1',
  kind,
  targets: [],
})

describe('stackItemName', () => {
  it('names a spell by its card and an ability by what it is on', () => {
    expect(stackItemName(item({ type: 'spell', card: { card: 1, def: 'shock' } }), cards)).toBe(
      'Shock',
    )
    const ability = item({
      type: 'ability',
      ability: { source: 4, def: 'monastery_swiftspear', index: 0 },
    })
    expect(stackItemName(ability, cards)).toBe("Monastery Swiftspear's ability")
    expect(stackItemDef(ability, cards)?.name).toBe('Monastery Swiftspear')
  })

  it('names an emblem ability by the emblem and a keyword ability by the keyword', () => {
    const emblem = item({ type: 'emblem', emblem: 3, name: 'The Pyre' })
    expect(stackItemName(emblem, cards)).toBe("The Pyre's ability")
    expect(stackItemDef(emblem, cards)).toBeUndefined()
    const prowess = item({
      type: 'keyword',
      source: 4,
      def: 'monastery_swiftspear',
      keyword: 'prowess',
    })
    expect(stackItemName(prowess, cards)).toBe("Monastery Swiftspear's prowess")
  })
})