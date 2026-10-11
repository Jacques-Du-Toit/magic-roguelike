import { useEffect, useMemo, useRef, useState } from 'react'
import { PreviewProvider } from './components/Preview.tsx'
import { DuelController, type Sleep } from './duel/controller.ts'
import { Duel, cardDefs, deckDefs, engineInfo, loadEngine, randomSeed } from './engine/index.ts'
import type { CardDef } from './engine/types/CardDef.ts'
import type { DeckDef } from './engine/types/DeckDef.ts'
import { DuelScreen, type NextGame, type Speed } from './screens/DuelScreen.tsx'
import { type GameChoice, MenuScreen } from './screens/MenuScreen.tsx'

/**
 * The default matchup (docs/ROADMAP.md, M2): the human plays the control
 * deck, and the AI one of the decks it can pilot.
 */
const DEFAULT_HUMAN_DECK = 'deck_b'
const RANDOM_AI_DECKS = ['deck_a', 'deck_c']

/** How much of each pause is kept at each speed. */
const SPEED_FACTOR: Record<Speed, number> = { normal: 1, fast: 0.4, instant: 0 }
const SPEED_KEY = 'duel-speed'

function isSpeed(value: string | null): value is Speed {
  return value !== null && value in SPEED_FACTOR
}

type EngineStatus =
  | { kind: 'loading' }
  | { kind: 'ready'; engine: string; cards: Map<string, CardDef>; decks: DeckDef[] }
  | { kind: 'failed'; message: string }

interface Game {
  duel: Duel
  controller: DuelController
  choice: GameChoice
  /** Counts the games of this visit, so each gets a fresh screen. */
  id: number
}

function endGame(game: Game | null): void {
  if (!game) return
  game.controller.dispose()
  game.duel.free()
}

/** The game the address asks for, e.g. `?seed=7&you=deck_b&ai=deck_a`. */
function choiceFromAddress(): { choice: GameChoice; start: boolean; speed: Speed | null } {
  const params = new URLSearchParams(window.location.search)
  const speed = params.get('speed')
  return {
    choice: {
      humanDeck: params.get('you') ?? DEFAULT_HUMAN_DECK,
      aiDeck: params.get('ai') ?? 'random',
      seed: params.get('seed') ?? '',
    },
    start: params.has('seed'),
    speed: isSpeed(speed) ? speed : null,
  }
}

function App() {
  const [status, setStatus] = useState<EngineStatus>({ kind: 'loading' })
  const [game, setGame] = useState<Game | null>(null)
  const [menuError, setMenuError] = useState<string | null>(null)
  const [lastChoice, setLastChoice] = useState<GameChoice>(() => choiceFromAddress().choice)
  const [speed, setSpeed] = useState<Speed>(() => {
    const stored = window.localStorage.getItem(SPEED_KEY)
    return choiceFromAddress().speed ?? (isSpeed(stored) ? stored : 'normal')
  })
  // The controller reads the speed at each pause, so a change applies at once.
  const speedRef = useRef(speed)
  const changeSpeed = (next: Speed) => {
    speedRef.current = next
    setSpeed(next)
    window.localStorage.setItem(SPEED_KEY, next)
  }

  const startGame = (
    choice: GameChoice,
    cards: ReadonlyMap<string, CardDef>,
    previous: Game | null,
  ) => {
    const seedText = choice.seed.trim()
    if (seedText !== '' && !/^\d+$/.test(seedText)) {
      setMenuError('The seed must be a whole number.')
      return
    }
    const seed = seedText === '' ? randomSeed() : seedText
    // The seed decides a random opponent too, so a seed replays the same game.
    const aiDeck =
      choice.aiDeck === 'random'
        ? RANDOM_AI_DECKS[Number(BigInt(seed) % BigInt(RANDOM_AI_DECKS.length))]
        : choice.aiDeck
    const sleep: Sleep = (ms) => {
      const scaled = ms * SPEED_FACTOR[speedRef.current]
      return scaled <= 0 ? Promise.resolve() : new Promise((done) => setTimeout(done, scaled))
    }
    try {
      const duel = new Duel(choice.humanDeck, aiDeck, seed)
      const info = { humanDeck: choice.humanDeck, aiDeck, seed }
      const controller = new DuelController(duel, info, cards, sleep)
      endGame(previous)
      setMenuError(null)
      setLastChoice(choice)
      setGame({ duel, controller, choice, id: (previous?.id ?? 0) + 1 })
      void controller.start()
    } catch (error) {
      setMenuError(error instanceof Error ? error.message : String(error))
    }
  }

  useEffect(() => {
    let cancelled = false
    loadEngine()
      .then(() => {
        if (cancelled) return
        const info = engineInfo()
        const cards = new Map(cardDefs().map((card) => [card.id, card]))
        setStatus({
          kind: 'ready',
          engine: `Rules engine ${info.name} ${info.version}.`,
          cards,
          decks: deckDefs(),
        })
        const asked = choiceFromAddress()
        if (asked.start) startGame(asked.choice, cards, null)
      })
      .catch((error: unknown) => {
        if (!cancelled) setStatus({ kind: 'failed', message: String(error) })
      })
    return () => {
      cancelled = true
    }
    // Runs once: it loads the engine and starts the game the address names.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const deckNames = useMemo(
    () =>
      new Map(status.kind === 'ready' ? status.decks.map((deck) => [deck.id, deck.name]) : []),
    [status],
  )

  if (status.kind === 'loading') {
    return (
      <main className="menu">
        <h1>Magic Roguelike</h1>
        <p>Loading the rules engine…</p>
      </main>
    )
  }
  if (status.kind === 'failed') {
    return (
      <main className="menu">
        <h1>Magic Roguelike</h1>
        <p role="alert">The rules engine failed to load: {status.message}</p>
      </main>
    )
  }

  const onNextGame = (next: NextGame) => {
    if (!game) return
    if (next === 'menu') {
      endGame(game)
      setGame(null)
    } else {
      const { info } = game.controller.getSnapshot()
      // "Play again" keeps the choice, so a random opponent is picked afresh.
      const choice: GameChoice =
        next === 'same_seed'
          ? { ...game.choice, seed: info.seed }
          : { ...game.choice, seed: '' }
      startGame(choice, status.cards, game)
    }
  }

  return (
    <PreviewProvider>
      {game ? (
        <DuelScreen
          key={game.id}
          controller={game.controller}
          cards={status.cards}
          deckNames={deckNames}
          speed={speed}
          onSpeed={changeSpeed}
          onNextGame={onNextGame}
        />
      ) : (
        <MenuScreen
          decks={status.decks}
          cards={status.cards}
          initial={lastChoice}
          randomDecks={RANDOM_AI_DECKS.map((deck) => deckNames.get(deck) ?? deck)}
          error={menuError}
          engine={status.engine}
          onStart={(choice) => startGame(choice, status.cards, null)}
        />
      )}
    </PreviewProvider>
  )
}

export default App
