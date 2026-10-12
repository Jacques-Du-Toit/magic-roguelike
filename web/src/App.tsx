import { useEffect, useMemo, useRef, useState } from 'react'
import { PreviewProvider } from './components/Preview.tsx'
import { DuelController, type Sleep } from './duel/controller.ts'
import {
  Duel,
  Run,
  cardDefs,
  deckDefs,
  engineInfo,
  loadEngine,
  randomSeed,
  runContent,
} from './engine/index.ts'
import type { CardDef } from './engine/types/CardDef.ts'
import type { DeckDef } from './engine/types/DeckDef.ts'
import type { RunContent } from './engine/types/RunContent.ts'
import { DuelScreen, type NextGame, type Speed } from './screens/DuelScreen.tsx'
import { type GameChoice, MenuScreen, type RunChoice } from './screens/MenuScreen.tsx'
import { RunScreen } from './screens/RunScreen.tsx'

/**
 * The default practice matchup (docs/ROADMAP.md, M2): the human plays the
 * control deck, and the AI one of the decks it can pilot.
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
  | {
      kind: 'ready'
      engine: string
      cards: Map<string, CardDef>
      decks: DeckDef[]
      content: RunContent
    }
  | { kind: 'failed'; message: string }

interface Game {
  duel: Duel
  controller: DuelController
  choice: GameChoice
  /** Counts the games of this visit, so each gets a fresh screen. */
  id: number
}

interface RunGame {
  run: Run
  choice: RunChoice
  /** Counts the runs of this visit, so each gets a fresh screen. */
  id: number
}

function endGame(game: Game | null): void {
  if (!game) return
  game.controller.dispose()
  game.duel.free()
}

/**
 * What the address asks for: a practice duel such as
 * `?seed=7&you=deck_b&ai=deck_a`, or a run such as `?run=warden&seed=7`
 * (with `&emblem=emblem_fury` for a starting emblem other than the usual).
 */
function choiceFromAddress(): {
  choice: GameChoice
  run: RunChoice
  start: 'duel' | 'run' | null
  speed: Speed | null
} {
  const params = new URLSearchParams(window.location.search)
  const speed = params.get('speed')
  const seed = params.get('seed') ?? ''
  const run = params.get('run')
  return {
    choice: {
      humanDeck: params.get('you') ?? DEFAULT_HUMAN_DECK,
      aiDeck: params.get('ai') ?? 'random',
      seed: run === null ? seed : '',
    },
    run: {
      planeswalker: run ?? '',
      emblem: params.get('emblem') ?? '',
      seed: run === null ? '' : seed,
    },
    start: run !== null ? 'run' : params.has('seed') ? 'duel' : null,
    speed: isSpeed(speed) ? speed : null,
  }
}

/** The seed to use: the one typed, a new random one, or null if it is no number. */
function readSeed(text: string): string | null {
  const seed = text.trim()
  if (seed === '') return randomSeed()
  return /^\d+$/.test(seed) ? seed : null
}

function App() {
  const [status, setStatus] = useState<EngineStatus>({ kind: 'loading' })
  const [game, setGame] = useState<Game | null>(null)
  const [runGame, setRunGame] = useState<RunGame | null>(null)
  const [menuError, setMenuError] = useState<string | null>(null)
  const [runError, setRunError] = useState<string | null>(null)
  const [lastChoice, setLastChoice] = useState<GameChoice>(() => choiceFromAddress().choice)
  const [lastRun, setLastRun] = useState<RunChoice>(() => choiceFromAddress().run)
  const [speed, setSpeed] = useState<Speed>(() => {
    const stored = window.localStorage.getItem(SPEED_KEY)
    return choiceFromAddress().speed ?? (isSpeed(stored) ? stored : 'normal')
  })
  // The controller reads the speed at each pause, so a change applies at once.
  // A run left for the menu may still have a duel winding down; its handle
  // is freed when the next run starts.
  const staleRun = useRef<Run | null>(null)
  const speedRef = useRef(speed)
  const changeSpeed = (next: Speed) => {
    speedRef.current = next
    setSpeed(next)
    window.localStorage.setItem(SPEED_KEY, next)
  }
  const sleep: Sleep = (ms) => {
    const scaled = ms * SPEED_FACTOR[speedRef.current]
    return scaled <= 0 ? Promise.resolve() : new Promise((done) => setTimeout(done, scaled))
  }

  const startGame = (
    choice: GameChoice,
    cards: ReadonlyMap<string, CardDef>,
    previous: Game | null,
  ) => {
    const seed = readSeed(choice.seed)
    if (seed === null) {
      setMenuError('The seed must be a whole number.')
      return
    }
    // The seed decides a random opponent too, so a seed replays the same game.
    const aiDeck =
      choice.aiDeck === 'random'
        ? RANDOM_AI_DECKS[Number(BigInt(seed) % BigInt(RANDOM_AI_DECKS.length))]
        : choice.aiDeck
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

  const startRun = (choice: RunChoice, previous: RunGame | null) => {
    const seed = readSeed(choice.seed)
    if (seed === null) {
      setRunError('The seed must be a whole number.')
      return
    }
    try {
      const run = new Run(choice.planeswalker, choice.emblem, seed)
      previous?.run.free()
      staleRun.current?.free()
      staleRun.current = null
      setRunError(null)
      setLastRun(choice)
      setRunGame({ run, choice, id: (previous?.id ?? 0) + 1 })
    } catch (error) {
      setRunError(error instanceof Error ? error.message : String(error))
    }
  }

  useEffect(() => {
    let cancelled = false
    loadEngine()
      .then(() => {
        if (cancelled) return
        const info = engineInfo()
        const cards = new Map(cardDefs().map((card) => [card.id, card]))
        const content = runContent()
        setStatus({
          kind: 'ready',
          engine: `Rules engine ${info.name} ${info.version}.`,
          cards,
          decks: deckDefs(),
          content,
        })
        const asked = choiceFromAddress()
        setLastRun((last) => ({
          ...last,
          planeswalker: last.planeswalker || content.planeswalkers[0].id,
        }))
        if (asked.start === 'duel') startGame(asked.choice, cards, null)
        if (asked.start === 'run') startRun(asked.run, null)
      })
      .catch((error: unknown) => {
        if (!cancelled) setStatus({ kind: 'failed', message: String(error) })
      })
    return () => {
      cancelled = true
    }
    // Runs once: it loads the engine and starts what the address names.
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

  const onRunExit = (next: 'menu' | 'new_run') => {
    if (!runGame) return
    if (next === 'new_run') {
      startRun({ ...runGame.choice, seed: '' }, runGame)
    } else {
      staleRun.current = runGame.run
      setLastRun({ ...runGame.choice, seed: runGame.run.seed })
      setRunGame(null)
    }
  }

  let screen
  if (runGame) {
    screen = (
      <RunScreen
        key={runGame.id}
        run={runGame.run}
        content={status.content}
        cards={status.cards}
        sleep={sleep}
        speed={speed}
        onSpeed={changeSpeed}
        onExit={onRunExit}
      />
    )
  } else if (game) {
    screen = (
      <DuelScreen
        key={game.id}
        controller={game.controller}
        cards={status.cards}
        deckNames={deckNames}
        speed={speed}
        onSpeed={changeSpeed}
        exit={{ kind: 'practice', onNextGame }}
      />
    )
  } else {
    screen = (
      <MenuScreen
        decks={status.decks}
        cards={status.cards}
        planeswalkers={status.content.planeswalkers}
        emblems={status.content.emblems}
        initial={lastChoice}
        initialRun={lastRun}
        randomDecks={RANDOM_AI_DECKS.map((deck) => deckNames.get(deck) ?? deck)}
        error={menuError}
        runError={runError}
        engine={status.engine}
        onStart={(choice) => startGame(choice, status.cards, null)}
        onStartRun={(choice) => startRun(choice, null)}
      />
    )
  }
  return <PreviewProvider>{screen}</PreviewProvider>
}

export default App
