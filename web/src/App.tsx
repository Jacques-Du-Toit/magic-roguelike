import { useEffect, useState } from 'react'
import { cardDefs, deckDefs, engineInfo, loadEngine } from './engine/index.ts'
import type { CardDef } from './engine/types/CardDef.ts'
import type { DeckDef } from './engine/types/DeckDef.ts'
import type { EngineInfo } from './engine/types/EngineInfo.ts'

type EngineStatus =
  | { kind: 'loading' }
  | { kind: 'ready'; info: EngineInfo; cards: CardDef[]; decks: DeckDef[] }
  | { kind: 'failed'; message: string }

function App() {
  const [status, setStatus] = useState<EngineStatus>({ kind: 'loading' })

  useEffect(() => {
    let cancelled = false
    loadEngine()
      .then(() => {
        if (!cancelled) {
          setStatus({ kind: 'ready', info: engineInfo(), cards: cardDefs(), decks: deckDefs() })
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) setStatus({ kind: 'failed', message: String(error) })
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <main>
      <h1>Magic Roguelike</h1>
      {status.kind === 'loading' && <p>Loading the rules engine…</p>}
      {status.kind === 'ready' && (
        <>
          <p data-testid="engine-info">
            Rules engine loaded from Rust: {status.info.name} {status.info.version}
          </p>
          {status.decks.map((deck) => (
            <section key={deck.id}>
              <h2>{deck.name}</h2>
              <ul>
                {deck.cards.map((entry) => {
                  const card = status.cards.find((def) => def.id === entry.card)
                  return (
                    <li key={entry.card}>
                      {entry.count}× {card?.name ?? entry.card} {card?.mana_cost}
                    </li>
                  )
                })}
              </ul>
            </section>
          ))}
        </>
      )}
      {status.kind === 'failed' && (
        <p role="alert">The rules engine failed to load: {status.message}</p>
      )}
    </main>
  )
}

export default App
