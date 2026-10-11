// Things shown over the duel: the mulligan, a graveyard, and the result.

import type { ReactNode } from 'react'
import { AI, HUMAN, playerIndex } from '../engine/index.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { CardRef } from '../engine/types/CardRef.ts'
import type { GameResult } from '../engine/types/GameResult.ts'
import type { PlayerView } from '../engine/types/PlayerView.ts'
import { CardFrame } from './Card.tsx'
import { Hand } from './Hand.tsx'
import { usePreview } from './usePreview.ts'

export function Overlay({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="overlay" role="dialog" aria-label={label}>
      <div className="overlay-box">{children}</div>
    </div>
  )
}

interface MulliganProps {
  view: PlayerView
  cards: ReadonlyMap<string, CardDef>
  prompt: string
  playable: (object: number) => boolean
  onCard: (object: number) => void
  /** The keep and mulligan buttons, when they are legal. */
  buttons: ReactNode
}

export function MulliganOverlay({ view, cards, prompt, playable, onCard, buttons }: MulliganProps) {
  const you = view.players[playerIndex(HUMAN)]
  const opponent = view.players[playerIndex(AI)]
  return (
    <Overlay label="Opening hand">
      <h2>Opening hand</h2>
      <p>
        {view.starting_player === HUMAN ? 'You take the first turn.' : 'The opponent takes the first turn.'}{' '}
        Mulligans so far: you {you.mulligans_taken}, opponent {opponent.mulligans_taken}.
      </p>
      <Hand hand={you.hand} cards={cards} playable={playable} selected={null} onCard={onCard} />
      <p className="prompt-text" data-testid="prompt">
        {prompt}
      </p>
      <div className="button-row">{buttons}</div>
    </Overlay>
  )
}

function GraveyardCard({ card, cards }: { card: CardRef; cards: ReadonlyMap<string, CardDef> }) {
  const def = card.face ? cards.get(card.face.def) : undefined
  const preview = usePreview(def)
  if (!def) return null
  return (
    <div {...preview}>
      <CardFrame def={def} size="normal" />
    </div>
  )
}

export function GraveyardOverlay({
  title,
  graveyard,
  cards,
  onClose,
}: {
  title: string
  graveyard: CardRef[]
  cards: ReadonlyMap<string, CardDef>
  onClose: () => void
}) {
  return (
    <Overlay label={title}>
      <h2>{title}</h2>
      {graveyard.length === 0 ? (
        <p className="muted">No cards.</p>
      ) : (
        <>
          <p className="muted">Most recent first.</p>
          <div className="card-grid">
            {graveyard.toReversed().map((card) => (
              <GraveyardCard key={card.object} card={card} cards={cards} />
            ))}
          </div>
        </>
      )}
      <div className="button-row">
        <button type="button" className="primary" onClick={onClose}>
          Close
        </button>
      </div>
    </Overlay>
  )
}

function resultText(result: GameResult): { title: string; detail: string } {
  if (result.type === 'draw') {
    return { title: 'Draw', detail: 'Both players lost at the same time.' }
  }
  const won = result.player === HUMAN
  const loser = won ? "The opponent's" : 'Your'
  const detail =
    result.reason === 'fatigue'
      ? `${loser} life reached 0 from drawing out of an empty library.`
      : `${loser} life reached 0.`
  return { title: won ? 'You win' : 'You lose', detail }
}

export function GameOverOverlay({
  result,
  turn,
  seed,
  children,
}: {
  result: GameResult
  turn: number
  seed: string
  /** The buttons. */
  children: ReactNode
}) {
  const { title, detail } = resultText(result)
  return (
    <Overlay label="Game over">
      <h2 data-testid="result">{title}</h2>
      <p>
        {detail} The game lasted {turn} turns.
      </p>
      <p className="muted">Seed {seed}</p>
      <div className="button-row">{children}</div>
    </Overlay>
  )
}
