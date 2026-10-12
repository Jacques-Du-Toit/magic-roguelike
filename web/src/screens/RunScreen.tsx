// The run: the map and what happens at its nodes. It renders the engine's
// run view, offers the legal run actions and sends one back. Nothing here
// decides what a node gives, what something costs or what a heart buys.

import { type ReactNode, useEffect, useRef, useState } from 'react'
import { ManaCost } from '../components/Card.tsx'
import { DeckView, RunCard } from '../components/RunCards.tsx'
import { RunMap } from '../components/RunMap.tsx'
import { usePreview } from '../components/usePreview.ts'
import { DuelController, type Sleep } from '../duel/controller.ts'
import type { Run } from '../engine/index.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { ModifierDef } from '../engine/types/ModifierDef.ts'
import type { RunAction } from '../engine/types/RunAction.ts'
import type { RunContent } from '../engine/types/RunContent.ts'
import type { RunEvent } from '../engine/types/RunEvent.ts'
import type { RunView } from '../engine/types/RunView.ts'
import {
  NODE_LABEL,
  type RunNames,
  boonDef,
  describeRunEvents,
  enemyName,
  relicDef,
} from '../run/describe.ts'
import { DuelScreen, type Speed } from './DuelScreen.tsx'

interface RunScreenProps {
  run: Run
  content: RunContent
  cards: ReadonlyMap<string, CardDef>
  sleep: Sleep
  speed: Speed
  onSpeed: (speed: Speed) => void
  /** Leave the run: back to the menu, or straight into a new run. */
  onExit: (next: 'menu' | 'new_run') => void
}

interface RunDuel {
  controller: DuelController
  /** Counts the duels of the run, so each gets a fresh screen. */
  id: number
}

function Modifier({ def, kind }: { def: ModifierDef; kind: string }) {
  return (
    <li className="modifier" data-testid={kind}>
      <strong>{def.name}</strong> {def.text}
    </li>
  )
}

export function RunScreen({ run, content, cards, sleep, speed, onSpeed, onExit }: RunScreenProps) {
  const names: RunNames = { cards, content }
  const planeswalker = content.planeswalkers.find((def) => def.id === run.planeswalker)
  const commander = planeswalker?.commander ? cards.get(planeswalker.commander) : undefined
  const commanderPreview = usePreview(commander)

  const duels = useRef(0)
  const startDuel = (view: RunView): RunDuel | null => {
    if (view.phase.type !== 'duel') return null
    const info = {
      humanDeck: planeswalker?.name ?? run.planeswalker,
      aiDeck: enemyName(names, view.phase.enemy),
      seed: run.seed,
    }
    const controller = new DuelController(run.duel, info, cards, sleep)
    void controller.start()
    duels.current += 1
    return { controller, id: duels.current }
  }

  const [view, setView] = useState(() => run.view())
  const [news, setNews] = useState<string[]>([])
  const [error, setError] = useState<string | null>(null)
  // A run starts on the map, so there is no duel to pick up here.
  const [duel, setDuel] = useState<RunDuel | null>(null)
  const [deckOpen, setDeckOpen] = useState(false)
  const [removing, setRemoving] = useState(false)
  const [copied, setCopied] = useState(false)
  const [abandoning, setAbandoning] = useState(false)

  // The controller of the duel on screen stops when the screen goes away.
  const current = useRef(duel)
  useEffect(() => {
    current.current = duel
  }, [duel])
  useEffect(() => () => current.current?.controller.dispose(), [])

  // With `?hints` in the address, the page offers the engine's suggested
  // duel action. The end-to-end tests follow it to play whole runs.
  useEffect(() => {
    if (!new URLSearchParams(window.location.search).has('hints')) return
    const hints = window as unknown as { suggestDuelAction?: () => unknown }
    hints.suggestDuelAction = () => run.duel.suggest()
    return () => {
      delete hints.suggestDuelAction
    }
  }, [run])

  const send = (action: RunAction) => {
    let events: RunEvent[]
    let next: RunView
    try {
      const update = run.apply(action)
      events = update.events
      next = update.view
    } catch (thrown) {
      setError(thrown instanceof Error ? thrown.message : String(thrown))
      return
    }
    setError(null)
    setRemoving(false)
    // A duel that has been left, or replaced by a rematch, is over.
    const started = events.some((event) => event.type === 'duel_started')
    if (duel && (started || next.phase.type !== 'duel')) {
      duel.controller.dispose()
      setDuel(null)
    }
    if (started) setDuel(startDuel(next))
    setView(next)
    setNews(describeRunEvents(events, names))
  }

  const copyReport = () => {
    const report = JSON.stringify({
      planeswalker: run.planeswalker,
      emblem: view.emblem,
      seed: run.seed,
      error,
      inputs: run.inputs,
    })
    void navigator.clipboard.writeText(report).then(() => setCopied(true))
  }

  const { phase, actions } = view
  const emblem = content.emblems.find((def) => def.id === view.emblem)
  const can =(type: RunAction['type']) => actions.find((action) => action.type === type)

  if (phase.type === 'duel' && duel) {
    return (
      <DuelScreen
        key={duel.id}
        controller={duel.controller}
        cards={cards}
        deckNames={new Map()}
        speed={speed}
        onSpeed={onSpeed}
        exit={{
          kind: 'run',
          hearts: `${view.hearts} of ${view.max_hearts} hearts`,
          stakes:
            phase.kind === 'boss'
              ? 'Lose this duel and the run is over.'
              : 'Losing this duel costs a heart.',
          onConcede: () => send({ type: 'concede' }),
          onContinue: () => send({ type: 'finish_duel' }),
        }}
      />
    )
  }

  const removable = new Set(
    actions.flatMap((action) => (action.type === 'remove_card' ? [action.card] : [])),
  )
  const basics = view.basic_lands.reduce((sum, entry) => sum + entry.count, 0)
  const here = view.map.nodes.find((node) => node.id === view.path.at(-1))

  let title: string
  let body: ReactNode
  switch (phase.type) {
    case 'map':
      title = view.path.length === 0 ? 'Choose where to start' : 'Choose where to go next'
      body = (
        <>
          <RunMap view={view} onTravel={(node) => send({ type: 'travel', node })} />
          <ul className="map-legend">
            {(['duel', 'elite', 'rest', 'shop', 'boss'] as const).map((kind) => (
              <li key={kind}>
                <span aria-hidden="true">{NODE_LABEL[kind].symbol}</span>{' '}
                <strong>{NODE_LABEL[kind].name}</strong> {NODE_LABEL[kind].about}
              </li>
            ))}
          </ul>
        </>
      )
      break
    case 'reward':
      title = 'Choose a card to add to your deck'
      body = (
        <>
          <div className="card-grid choice-row" data-testid="reward">
            {phase.choices.map((card, choice) => (
              <RunCard
                key={card}
                def={cards.get(card)}
                onPick={() => send({ type: 'take_reward', choice })}
                testId="reward-card"
              />
            ))}
          </div>
          <div className="button-row">
            <button type="button" onClick={() => send({ type: 'skip_reward' })} data-testid="skip">
              Take none
            </button>
          </div>
        </>
      )
      break
    case 'rest': {
      const heal = can('rest_heal')
      title = 'Rest site: choose one'
      body = (
        <div className="choice-list">
          <button
            type="button"
            disabled={!heal}
            onClick={() => heal && send(heal)}
            data-testid="rest-heal"
          >
            Restore a heart
            <span className="muted">
              {heal ? ` You have ${view.hearts} of ${view.max_hearts}.` : ' Your hearts are full.'}
            </span>
          </button>
          <button
            type="button"
            disabled={removable.size === 0}
            onClick={() => setRemoving(true)}
            data-testid="rest-remove"
          >
            Remove a card from your deck
            <span className="muted">
              {removable.size === 0 ? ' Your deck is too small.' : ' A leaner deck draws its best cards more often.'}
            </span>
          </button>
          <button type="button" className="primary" onClick={() => send({ type: 'leave' })} data-testid="leave">
            Leave without doing either
          </button>
        </div>
      )
      break
    }
    case 'shop': {
      title = 'Shop'
      const affordable = new Set(
        actions.flatMap((action) => (action.type === 'buy' ? [action.item] : [])),
      )
      const reason = (item: (typeof phase.items)[number], index: number) =>
        item.sold ? 'Sold' : affordable.has(index) ? `Buy for ${item.price} gold` : `${item.price} gold: too dear`
      body = (
        <>
          <div className="card-grid choice-row" data-testid="shop">
            {phase.items.map((item, index) => {
              const buy = (
                <button
                  type="button"
                  disabled={!affordable.has(index)}
                  onClick={() => send({ type: 'buy', item: index })}
                  data-testid="buy"
                >
                  {reason(item, index)}
                </button>
              )
              if (item.ware.type === 'card') {
                return (
                  <RunCard key={index} def={cards.get(item.ware.card)}>
                    {buy}
                  </RunCard>
                )
              }
              const relic = relicDef(names, item.ware.relic)
              return (
                <div key={index} className="run-card relic-ware">
                  <div className="relic-card">
                    <span className="muted">Relic</span>
                    <strong>{relic?.name ?? item.ware.relic}</strong>
                    <span>{relic?.text}</span>
                  </div>
                  {buy}
                </div>
              )
            })}
          </div>
          <div className="button-row">
            <button
              type="button"
              disabled={removable.size === 0}
              onClick={() => setRemoving(true)}
              data-testid="shop-remove"
            >
              Remove a card for {phase.removal_price} gold
              {phase.removals_left === 0
                ? ' (done here)'
                : removable.size === 0
                  ? ' (not now)'
                  : ''}
            </button>
            <button type="button" className="primary" onClick={() => send({ type: 'leave' })} data-testid="leave">
              Leave the shop
            </button>
          </div>
        </>
      )
      break
    }
    case 'boons': {
      title = 'Before the boss: spend your hearts on boons'
      const boss = content.enemies.find((enemy) => enemy.tier === 'boss')
      body = (
        <>
          <p>
            Each heart you still have buys one boon for the boss duel. Picks left:{' '}
            <strong data-testid="picks-left">{phase.picks_left}</strong>.
          </p>
          <div className="choice-list" data-testid="boons">
            {content.boons.map((boon) => {
              const taken = view.boons.includes(boon.id)
              const take = actions.find(
                (action) => action.type === 'take_boon' && action.boon === boon.id,
              )
              return (
                <button
                  key={boon.id}
                  type="button"
                  disabled={!take}
                  onClick={() => take && send(take)}
                  data-testid="boon"
                >
                  <strong>{boon.name}</strong> {boon.text}
                  {taken && <span className="badge badge-granted">Taken</span>}
                </button>
              )
            })}
          </div>
          {boss && (
            <section className="boss-brief">
              <h3>{boss.name}</h3>
              <ul className="modifiers">
                {boss.modifiers.map((modifier) => (
                  <Modifier key={modifier.id} def={modifier} kind="boss-emblem" />
                ))}
              </ul>
              <p className="muted">Lose this duel and the run is over.</p>
            </section>
          )}
          <div className="button-row">
            <button type="button" className="primary" onClick={() => send({ type: 'start_boss' })} data-testid="start-boss">
              {phase.picks_left > 0 && can('take_boon') ? 'Fight the boss without the rest' : 'Fight the boss'}
            </button>
          </div>
        </>
      )
      break
    }
    case 'over':
      title = phase.won ? 'You won the act' : 'Your run is over'
      body = (
        <>
          <p data-testid="run-result">
            {phase.won
              ? 'The boss is beaten. That is the whole of the game so far.'
              : view.hearts === 0
                ? 'You have no hearts left.'
                : 'The boss was too strong this time.'}
          </p>
          <p className="muted">
            You visited {view.path.length} of {view.map.rows} rows and finished with {view.gold}{' '}
            gold, {view.deck.length + basics} cards and {view.relics.length} relics.
          </p>
          <div className="button-row">
            <button type="button" onClick={() => onExit('menu')}>
              Back to the menu
            </button>
            <button type="button" className="primary" onClick={() => onExit('new_run')} data-testid="new-run">
              Start a new run
            </button>
          </div>
        </>
      )
      break
    case 'duel':
      // Only reached if the duel's controller is missing.
      title = 'Duel'
      body = <p>The duel could not be shown.</p>
      break
  }

  return (
    <div className="run" data-testid="run" data-phase={phase.type}>
      <header className="topbar run-topbar">
        <strong>{planeswalker?.name ?? run.planeswalker}</strong>
        <span
          className="hearts"
          data-testid="hearts"
          aria-label={`${view.hearts} of ${view.max_hearts} hearts`}
        >
          {Array.from({ length: view.max_hearts }, (_, index) => (
            <span key={index} className={index < view.hearts ? 'heart' : 'heart heart-lost'} aria-hidden="true">
              {index < view.hearts ? '♥' : '♡'}
            </span>
          ))}{' '}
          {view.hearts}/{view.max_hearts} hearts
        </span>
        <span data-testid="gold">{view.gold} gold</span>
        <button type="button" className="link" onClick={() => setDeckOpen(true)} data-testid="open-deck">
          Deck ({view.deck.length + basics} cards)
        </button>
        <span className="topbar-tools">
          <span className="muted" data-testid="seed">
            Seed {run.seed}
          </span>
          <button type="button" className="link" onClick={copyReport}>
            {copied ? 'Copied' : 'Copy bug report'}
          </button>
          {abandoning ? (
            <>
              <span>Abandon this run?</span>
              <button type="button" className="link" onClick={() => onExit('menu')}>
                Yes, abandon
              </button>
              <button type="button" className="link" onClick={() => setAbandoning(false)}>
                No
              </button>
            </>
          ) : (
            <button type="button" className="link" onClick={() => setAbandoning(true)}>
              {phase.type === 'over' ? 'Menu' : 'Abandon run'}
            </button>
          )}
        </span>
      </header>

      <div className="run-body">
        <main className="run-main">
          {news.length > 0 && (
            <div className="news" role="status" data-testid="news">
              {news.map((line, index) => (
                <p key={index}>{line}</p>
              ))}
            </div>
          )}
          {error && (
            <p role="alert" className="error">
              The rules engine reported a problem: {error}
            </p>
          )}
          <h1 className="run-title" data-testid="run-title">
            {title}
          </h1>
          {here && phase.type !== 'map' && phase.type !== 'over' && (
            <p className="muted">
              Row {here.row + 1} of {view.map.rows}: {NODE_LABEL[here.kind].name}
            </p>
          )}
          {body}
        </main>

        <aside className="run-side">
          {commander && (
            <section className="panel">
              <h2>Commander</h2>
              <ul className="modifiers">
                <li className="modifier" data-testid="run-commander" {...commanderPreview}>
                  <strong>{commander.name}</strong> <ManaCost cost={commander.mana_cost} />{' '}
                  {commander.power}/{commander.toughness}
                  {commander.oracle_text && <> · {commander.oracle_text}</>}
                </li>
              </ul>
            </section>
          )}
          <section className="panel">
            <h2>Emblem</h2>
            <ul className="modifiers">
              {emblem && <Modifier def={emblem} kind="starting-emblem" />}
            </ul>
          </section>
          <section className="panel">
            <h2>Relics</h2>
            {view.relics.length === 0 ? (
              <p className="muted">None yet. Elites and shops have them.</p>
            ) : (
              <ul className="modifiers">
                {view.relics.map((id) => {
                  const relic = relicDef(names, id)
                  return relic && <Modifier key={id} def={relic} kind="relic" />
                })}
              </ul>
            )}
          </section>
          {view.boons.length > 0 && (
            <section className="panel">
              <h2>Boons for the boss</h2>
              <ul className="modifiers">
                {view.boons.map((id) => {
                  const boon = boonDef(names, id)
                  return boon && <Modifier key={id} def={boon} kind="taken-boon" />
                })}
              </ul>
            </section>
          )}
        </aside>
      </div>

      {deckOpen && !removing && (
        <DeckView view={view} cards={cards} title="Your deck" onClose={() => setDeckOpen(false)} />
      )}
      {removing && (
        <DeckView
          view={view}
          cards={cards}
          title="Remove a card"
          hint="Click the card to remove. It is gone for the rest of the run."
          pickable={removable}
          onPick={(card) => send({ type: 'remove_card', card })}
          onClose={() => setRemoving(false)}
        />
      )}
    </div>
  )
}
