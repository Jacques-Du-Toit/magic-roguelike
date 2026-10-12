// The menu: start a run with a planeswalker, or a practice duel between
// two of the test decks.

import { useState } from 'react'
import { ManaCost } from '../components/Card.tsx'
import { usePreview } from '../components/usePreview.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { DeckDef } from '../engine/types/DeckDef.ts'
import type { ModifierDef } from '../engine/types/ModifierDef.ts'
import type { PlaneswalkerDef } from '../engine/types/PlaneswalkerDef.ts'

/** The opponent's deck, or a pick between the decks the AI plays well. */
export type AiDeckChoice = string | 'random'

export interface GameChoice {
  humanDeck: string
  aiDeck: AiDeckChoice
  /** Empty for a random seed. */
  seed: string
}

export interface RunChoice {
  planeswalker: string
  /** The starting emblem; empty for the planeswalker's usual one. */
  emblem: string
  /** Empty for a random seed. */
  seed: string
}

interface MenuScreenProps {
  decks: DeckDef[]
  cards: ReadonlyMap<string, CardDef>
  planeswalkers: PlaneswalkerDef[]
  /** The starting emblems to choose from. */
  emblems: ModifierDef[]
  initial: GameChoice
  initialRun: RunChoice
  /** Names of the decks "random" picks between. */
  randomDecks: string[]
  error: string | null
  runError: string | null
  engine: string
  onStart: (choice: GameChoice) => void
  onStartRun: (choice: RunChoice) => void
}

function DeckLine({ count, def, id }: { count: number; def: CardDef | undefined; id: string }) {
  const preview = usePreview(def)
  return (
    <li {...preview}>
      {count}× {def?.name ?? id} {def && <ManaCost cost={def.mana_cost} />}
    </li>
  )
}

/** A character's commander: its name and cost, with the card on hover. */
function CommanderLine({ def, id }: { def: CardDef | undefined; id: string }) {
  const preview = usePreview(def)
  return (
    <span className="commander-line" data-testid="menu-commander" {...preview}>
      Commander: <strong>{def?.name ?? id}</strong> {def && <ManaCost cost={def.mana_cost} />}
    </span>
  )
}

export function MenuScreen({
  decks,
  cards,
  planeswalkers,
  emblems,
  initial,
  initialRun,
  randomDecks,
  error,
  runError,
  engine,
  onStart,
  onStartRun,
}: MenuScreenProps) {
  const [choice, setChoice] = useState(initial)
  const [run, setRun] = useState(initialRun)
  const chosen =
    planeswalkers.find((planeswalker) => planeswalker.id === run.planeswalker) ?? planeswalkers[0]
  // The emblem picked, or the one the character usually takes.
  const emblem = emblems.find((def) => def.id === (run.emblem || chosen.emblem)) ?? emblems[0]
  const shown = decks.find((deck) => deck.id === choice.humanDeck)

  return (
    <main className="menu">
      <h1>Magic Roguelike</h1>
      <p className="muted">{engine}</p>

      <h2>Start a run</h2>
      <form
        onSubmit={(event) => {
          event.preventDefault()
          onStartRun({ ...run, planeswalker: chosen.id, emblem: emblem.id })
        }}
      >
        <p>
          One act: follow a path across the map, win duels, grow your deck and beat the boss. You
          have three hearts; a lost duel costs one. Your commander waits beside your hand in every
          duel, to be cast when you can pay for it; if it dies it comes back, costing 2 more each
          time.
        </p>
        <fieldset className="planeswalkers">
          <legend>1. Your commander</legend>
          {planeswalkers.map((planeswalker) => (
            <label
              key={planeswalker.id}
              className={`planeswalker${planeswalker.id === chosen.id ? ' planeswalker-chosen' : ''}`}
            >
              <input
                type="radio"
                name="planeswalker"
                value={planeswalker.id}
                checked={planeswalker.id === chosen.id}
                // A new character comes with its usual emblem.
                onChange={() => setRun({ ...run, planeswalker: planeswalker.id, emblem: '' })}
                data-testid={`planeswalker-${planeswalker.id}`}
              />
              <span>
                <strong>{planeswalker.name}</strong>{' '}
                <ManaCost cost={planeswalker.colors.map((color) => `{${color}}`).join('')} />
              </span>
              <span>{planeswalker.description}</span>
              {planeswalker.commander && (
                <CommanderLine
                  id={planeswalker.commander}
                  def={cards.get(planeswalker.commander)}
                />
              )}
            </label>
          ))}
        </fieldset>
        <fieldset className="planeswalkers">
          <legend>2. Your emblem</legend>
          {emblems.map((def) => (
            <label
              key={def.id}
              className={`planeswalker${def.id === emblem.id ? ' planeswalker-chosen' : ''}`}
            >
              <input
                type="radio"
                name="emblem"
                value={def.id}
                checked={def.id === emblem.id}
                onChange={() => setRun({ ...run, emblem: def.id })}
                data-testid={`emblem-${def.id}`}
              />
              <span>
                <strong>{def.name}</strong>
              </span>
              <span>{def.text}</span>
            </label>
          ))}
        </fieldset>
        <label>
          Seed (leave empty for a new random run)
          <input
            type="text"
            inputMode="numeric"
            value={run.seed}
            onChange={(event) => setRun({ ...run, seed: event.target.value })}
            data-testid="run-seed-input"
          />
        </label>
        {runError && (
          <p role="alert" className="error">
            {runError}
          </p>
        )}
        <button type="submit" className="primary" data-testid="start-run">
          Start run
        </button>
        <section>
          <h3>{chosen.name}'s starting spells</h3>
          <ul className="deck-list">
            {chosen.spells.map((entry) => (
              <DeckLine
                key={entry.card}
                count={entry.count}
                id={entry.card}
                def={cards.get(entry.card)}
              />
            ))}
          </ul>
          <p className="muted">Basic lands are added for you and follow your deck as it changes.</p>
        </section>
      </form>

      <h2>Practice duel</h2>
      <p className="muted">A single duel against the computer with the test decks.</p>
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
          <h3>{shown.name}</h3>
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
