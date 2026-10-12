// The human's hand, face up, with their commander beside it while it waits
// in the command zone.

import type { CardDef } from '../engine/types/CardDef.ts'
import type { CardRef } from '../engine/types/CardRef.ts'
import type { CommanderView } from '../engine/types/CommanderView.ts'
import { CardBack, CardFrame, ManaCost } from './Card.tsx'
import { usePreview } from './usePreview.ts'

interface HandProps {
  hand: CardRef[]
  /** The human's commander, if they have one. */
  commander: CommanderView | null
  cards: ReadonlyMap<string, CardDef>
  /** Whether clicking the card would do something. */
  playable: (object: number) => boolean
  selected: number | null
  onCard: (object: number) => void
}

type CardProps = Omit<HandProps, 'hand' | 'commander'>

function HandCard({ card, cards, playable, selected, onCard }: CardProps & { card: CardRef }) {
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

/**
 * The commander in the command zone: clicked like a card in hand, and
 * labelled with what it costs now, tax included.
 */
function CommandCard({
  commander,
  object,
  cards,
  playable,
  selected,
  onCard,
}: CardProps & { commander: CommanderView; object: number }) {
  const def = cards.get(commander.def)
  const preview = usePreview(def)
  if (!def) return null
  const canPlay = playable(object)
  const classes = ['hand-card', canPlay ? 'lit-action' : '', selected === object ? 'lit-selected' : '']
  return (
    <div className="command-zone" data-testid="command-zone">
      <span className="command-label">
        Commander <ManaCost cost={commander.cost} />
      </span>
      <button
        type="button"
        className={classes.filter(Boolean).join(' ')}
        data-object={object}
        data-card={def.id}
        data-playable={canPlay}
        aria-label={`Your commander, ${def.name}: costs ${commander.cost}`}
        onClick={() => onCard(object)}
        {...preview}
      >
        <CardFrame def={def} size="normal" />
      </button>
    </div>
  )
}

export function Hand({ hand, commander, ...rest }: HandProps) {
  return (
    <div className="hand" aria-label="Your hand" data-testid="hand">
      {commander && commander.object !== null && (
        <CommandCard commander={commander} object={commander.object} {...rest} />
      )}
      {hand.map((card) => (
        <HandCard key={card.face?.card ?? `object-${card.object}`} card={card} {...rest} />
      ))}
    </div>
  )
}
