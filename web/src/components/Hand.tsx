// The human's hand, face up.

import type { CardDef } from '../engine/types/CardDef.ts'
import type { CardRef } from '../engine/types/CardRef.ts'
import { CardBack, CardFrame } from './Card.tsx'
import { usePreview } from './usePreview.ts'

interface HandProps {
  hand: CardRef[]
  cards: ReadonlyMap<string, CardDef>
  /** Whether clicking the card would do something. */
  playable: (object: number) => boolean
  selected: number | null
  onCard: (object: number) => void
}

function HandCard({
  card,
  cards,
  playable,
  selected,
  onCard,
}: Omit<HandProps, 'hand'> & { card: CardRef }) {
  const def = card.face ? cards.get(card.face.def) : undefined
  const preview = usePreview(def)
  if (!def) return <CardBack size="normal" />
  const canPlay = playable(card.object)
  const classes = ['hand-card', canPlay ? 'lit-action' : '', selected === card.object ? 'lit-selected' : '']
  return (
    <button
      type="button"
      className={classes.filter(Boolean).join(' ')}
      data-object={card.object}
      data-card={def.id}
      data-playable={canPlay}
      onClick={() => onCard(card.object)}
      {...preview}
    >
      <CardFrame def={def} size="normal" />
    </button>
  )
}

export function Hand({ hand, ...rest }: HandProps) {
  return (
    <div className="hand" aria-label="Your hand" data-testid="hand">
      {hand.map((card) => (
        <HandCard key={card.face?.card ?? `object-${card.object}`} card={card} {...rest} />
      ))}
    </div>
  )
}
