// The deck picker shown before a duel.

import { useState } from 'react'
import { ManaCost } from '../components/Card.tsx'
import { usePreview } from '../components/usePreview.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { DeckDef } from '../engine/types/DeckDef.ts'

/** The opponent's deck, or a pick between the decks the AI plays well. */
export type AiDeckChoice = string | 'random'

export interface GameChoice {
  humanDeck: string
  aiDeck: AiDeckChoice
  /** Empty for a random seed. */
  seed: string
}

interface MenuScreenProps {
  decks: DeckDef[]
  cards: ReadonlyMap<string, CardDef>
  initial: GameChoice
  /** Names of the decks "random" picks between. */
  randomDecks: string[]
  error: string | null
  engine: string
  onStart: (choice: GameChoice) => void
}

function DeckLine({ count, def, id }: { count: number; def: CardDef | undefined; id: string }) {
  const preview = usePreview(def)
  return (
    <li {...preview}>
      {count}× {def?.name ?? id} {def && <ManaCost cost={def.mana_cost} />}
    </li>
  )
}

export function MenuScreen({
  decks,
  cards,
  initial,
  randomDecks,
  error,
  engine,
  onStart,
}: MenuScreenProps) {
  const [choice, setChoice] = useState(initial)
  const shown = decks.find((deck) => deck.id === choice.humanDeck)

  return (
    <main className="menu">
      <h1>Magic Roguelike</h1>
      <p className="muted">A duel against the computer. {engine}</p>
      <form
        onSubmit={(event) => {
          event.preventDefault()
          onStart(choice)
        }}
      >
        <label>
          Your deck
          <select
            value={choice.humanDeck}
            onChange={(event) => setChoice({ ...choice, humanDeck: event.target.value })}
            data-testid="human-deck"
          >
            {decks.map((deck) => (
              <option key={deck.id} value={deck.id}>
                {deck.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Opponent's deck
          <select
            value={choice.aiDeck}
            onChange={(event) => setChoice({ ...choice, aiDeck: event.target.value })}
            data-testid="ai-deck"
          >
            <option value="random">Random ({randomDecks.join(' or ')})</option>
            {decks.map((deck) => (
              <option key={deck.id} value={deck.id}>
                {deck.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Seed (leave empty for a new random game)
          <input
            type="text"
            inputMode="numeric"
            value={choice.seed}
            onChange={(event) => setChoice({ ...choice, seed: event.target.value })}
            data-testid="seed-input"
          />
        </label>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <button type="submit" className="primary" data-testid="start">
          Start duel
        </button>
      </form>
      {shown && (
        <section>
          <h2>{shown.name}</h2>
          <ul className="deck-list">
            {shown.cards.map((entry) => (
              <DeckLine
                key={entry.card}
                count={entry.count}
                id={entry.card}
                def={cards.get(entry.card)}
              />
            ))}
          </ul>
        </section>
      )}
    </main>
  )
}
