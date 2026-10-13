// Cards as the run shows them: one to pick, and the whole deck.

import type { ReactNode } from 'react'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { RunView } from '../engine/types/RunView.ts'
import { playedAs, upgradeChange } from '../run/describe.ts'
import { CardFrame } from './Card.tsx'
import { Overlay } from './Overlays.tsx'
import { usePreview } from './usePreview.ts'

interface RunCardProps {
  def: CardDef | undefined
  /** With `onPick`, the card is a button. */
  onPick?: () => void
  disabled?: boolean
  /** Shown under the card, e.g. its price. */
  children?: ReactNode
  testId?: string
}

/** A card with hover-to-enlarge, optionally clickable. */
export function RunCard({ def, onPick, disabled, children, testId }: RunCardProps) {
  const preview = usePreview(def)
  if (!def) return null
  const frame = <CardFrame def={def} size="normal" />
  return (
    <div className="run-card" data-card={def.id} data-testid={testId}>
      {onPick ? (
        <button
          type="button"
          className="run-card-button lit-action"
          onClick={onPick}
          disabled={disabled}
          aria-label={def.name}
          {...preview}
        >
          {frame}
        </button>
      ) : (
        <div {...preview}>{frame}</div>
      )}
      {children}
    </div>
  )
}

interface DeckViewProps {
  view: RunView
  cards: ReadonlyMap<string, CardDef>
  title: string
  /** What clicking a card will do, in words, when cards can be picked. */
  hint?: string
  /** Called with a card's position in the deck. Only those listed can be picked. */
  onPick?: (index: number) => void
  pickable?: ReadonlySet<number>
  /** Show each card that can be picked as its upgraded side, with what changes. */
  upgrades?: boolean
  onClose: () => void
}

/**
 * The player's deck: every spell and non-basic land, and the basic lands
 * the game adds. Also used to choose a card to remove or to upgrade.
 */
export function DeckView({ view, cards, title, hint, onPick, pickable, upgrades, onClose }: DeckViewProps) {
  const basics = view.basic_lands.reduce((sum, entry) => sum + entry.count, 0)
  // Cheapest first, but each card keeps its position in the deck.
  const order = view.deck
    .map((instance, index) => {
      const def = playedAs(cards, instance)
      const side = upgrades && pickable?.has(index) && def?.upgrade ? cards.get(def.upgrade) : undefined
      return side && def
        ? { index, def: side, change: upgradeChange(def, side) }
        : { index, def, change: undefined }
    })
    .sort(
      (a, b) =>
        (a.def?.mana_cost.length ?? 0) - (b.def?.mana_cost.length ?? 0) ||
        (a.def?.name ?? '').localeCompare(b.def?.name ?? ''),
    )
  return (
    <Overlay label={title}>
      <h2>{title}</h2>
      <p className="muted">
        {view.deck.length + basics} cards: {view.deck.length} of your own and {basics} basic lands.
        {hint ? ` ${hint}` : ''}
      </p>
      <div className="card-grid" data-testid="deck-cards">
        {order.map(({ index, def, change }) => (
          <RunCard
            key={index}
            def={def}
            onPick={onPick && pickable?.has(index) ? () => onPick(index) : undefined}
          >
            {change && (
              <span className="upgrade-change muted" data-testid="upgrade-change">
                {change}
              </span>
            )}
          </RunCard>
        ))}
      </div>
      <p>
        <strong>Basic lands</strong> (added for you, to fit your spells):{' '}
        {view.basic_lands.length === 0
          ? 'none'
          : view.basic_lands
              .map((entry) => `${entry.count} ${cards.get(entry.card)?.name ?? entry.card}`)
              .join(', ')}
      </p>
      <div className="button-row">
        <button type="button" className="primary" onClick={onClose} data-testid="close-deck">
          {onPick ? 'Cancel' : 'Close'}
        </button>
      </div>
    </Overlay>
  )
}
