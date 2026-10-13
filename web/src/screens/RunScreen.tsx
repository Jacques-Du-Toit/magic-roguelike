// The run: the map and what happens at its nodes. It renders the engine's
// run view, offers the legal run actions and sends one back. Nothing here
// decides what a node gives, what something costs or what a feat scores.

import { type ReactNode, useEffect, useRef, useState } from 'react'
import { ManaCost } from '../components/Card.tsx'
import { DeckView, RunCard } from '../components/RunCards.tsx'
import { RunMap } from '../components/RunMap.tsx'
import { usePreview } from '../components/usePreview.ts'
import { DuelController, type Sleep } from '../duel/controller.ts'
import type { Run } from '../engine/index.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { EnemyDef } from '../engine/types/EnemyDef.ts'
import type { ModifierDef } from '../engine/types/ModifierDef.ts'
import type { RunAction } from '../engine/types/RunAction.ts'
import type { RunContent } from '../engine/types/RunContent.ts'
import type { RunEvent } from '../engine/types/RunEvent.ts'
import type { RunView } from '../engine/types/RunView.ts'
import {
  NODE_LABEL,
  type RunNames,
  describeRunEvents,
  enemyName,
  featDef,
  featRewardText,
  featText,
  nextDuelDef,
  relicDef,
  woundDef,
} from '../run/describe.ts'
import { type DuelFeat, DuelScreen, type Speed } from './DuelScreen.tsx'

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

/**
 * An enemy's commander and emblems: the same on the feats screen before a
 * duel and in the side panel for the boss.
 */
function EnemyDetails({ enemy, commander }: { enemy: EnemyDef; commander: CardDef | undefined }) {
  const preview = usePreview(commander)
  return (
    <>
      {commander && (
        <p data-testid="enemy-commander" data-card={commander.id} {...preview}>
          Its commander is <strong>{commander.name}</strong> <ManaCost cost={commander.mana_cost} />{' '}
          {commander.power}/{commander.toughness}
          {commander.oracle_text && <> · {commander.oracle_text}</>}
        </p>
      )}
      {enemy.modifiers.length > 0 && (
        <ul className="modifiers">
          {enemy.modifiers.map((modifier) => (
            <Modifier key={modifier.id} def={modifier} kind="enemy-emblem" />
          ))}
        </ul>
      )}
    </>
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
  const [upgrading, setUpgrading] = useState(false)
  // The option of an event that a card of the deck is being chosen for.
  const [picking, setPicking] = useState<number | null>(null)
  // The feats ticked so far before a duel: their places among those offered.
  const [ticked, setTicked] = useState<number[]>([])
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
    setUpgrading(false)
    setPicking(null)
    setTicked([])
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

  // Everything needed to play the run again exactly: `sim --explain` does.
  const runReport = () => ({
    planeswalker: run.planeswalker,
    emblem: view.emblem,
    seed: run.seed,
    error,
    inputs: run.inputs,
  })
  const copyReport = () => {
    void navigator.clipboard.writeText(JSON.stringify(runReport())).then(() => setCopied(true))
  }

  const { phase, actions } = view
  const emblem = content.emblems.find((def) => def.id === view.emblem)
  const can = (type: RunAction['type']) => actions.find((action) => action.type === type)
  const wounds = `${view.wounds.length} of ${view.max_wounds} wounds`
  // How the feats picked for the duel stand, in the duel screen's words.
  const duelFeats = (): DuelFeat[] => {
    try {
      return run.duel.feats().flatMap((standing) => {
        const feat = featDef(names, standing.feat)
        return feat
          ? [
              {
                id: feat.id,
                name: feat.name,
                text: featText(feat),
                score: standing.score,
                tiers: feat.tiers.length,
              },
            ]
          : []
      })
    } catch {
      // No duel to ask about: it has just been left.
      return []
    }
  }

  if (phase.type === 'duel' && duel) {
    return (
      <DuelScreen
        key={duel.id}
        controller={duel.controller}
        cards={cards}
        deckNames={new Map()}
        speed={speed}
        onSpeed={onSpeed}
        report={runReport}
        exit={{
          kind: 'run',
          wounds,
          carried: view.wounds.flatMap((id) => {
            const wound = woundDef(names, id)
            return wound ? [{ name: wound.name, text: wound.text }] : []
          }),
          stakes:
            phase.kind === 'boss' || view.wounds.length + 1 >= view.max_wounds
              ? 'Lose this duel and the run is over.'
              : 'Losing this duel leaves a wound.',
          feats: duelFeats,
          onConcede: () => send({ type: 'concede' }),
          onContinue: () => send({ type: 'finish_duel' }),
        }}
      />
    )
  }

  const removable = new Set(
    actions.flatMap((action) => (action.type === 'remove_card' ? [action.card] : [])),
  )
  const upgradable = new Set(
    actions.flatMap((action) => (action.type === 'upgrade_card' ? [action.card] : [])),
  )
  const basics = view.basic_lands.reduce((sum, entry) => sum + entry.count, 0)
  const here =view.map.nodes.find((node) => node.id === view.path.at(-1))
  // The act's boss, which the engine names from the start.
  const boss = content.enemies.find((def) => def.id === view.boss?.enemy)
  const bossCommander = boss?.commander ? cards.get(boss.commander) : undefined

  let title: string
  let body: ReactNode
  switch (phase.type) {
    case 'map':
      title = view.path.length === 0 ? 'Choose where to start' : 'Choose where to go next'
      body = (
        <>
          <RunMap
            view={view}
            enemies={content.enemies}
            cards={cards}
            onTravel={(node) => send({ type: 'travel', node })}
          />
          <ul className="map-legend">
            {(['duel', 'elite', 'draft', 'treasure', 'event', 'rest', 'shop', 'boss'] as const).map((kind) => (
              <li key={kind}>
                <span aria-hidden="true">{NODE_LABEL[kind].symbol}</span>{' '}
                <strong>{NODE_LABEL[kind].name}</strong> {NODE_LABEL[kind].about}
              </li>
            ))}
          </ul>
        </>
      )
      break
    case 'feats': {
      const enemy = content.enemies.find((def) => def.id === phase.enemy)
      const enemyCommander = enemy?.commander ? cards.get(enemy.commander) : undefined
      // The engine lists every set of feats that may be picked; the ticks
      // are sent once they are one of them.
      const picks = actions.find((action) => action.type === 'choose_feats')?.feats.length ?? 0
      const sorted = ticked.toSorted((a, b) => a - b)
      const choice = actions.find(
        (action) =>
          action.type === 'choose_feats' &&
          action.feats.length === sorted.length &&
          action.feats.every((place, index) => place === sorted[index]),
      )
      const tick = (place: number) =>
        setTicked((before) =>
          before.includes(place)
            ? before.filter((other) => other !== place)
            : // Ticking one more than may be picked lets go of the oldest.
              [...before, place].slice(-picks),
        )
      title = `Before the duel: choose ${picks} feats`
      body = (
        <>
          <section className="boss-brief" data-testid="feats-enemy">
            <h3>
              {phase.kind === 'elite' ? 'Elite: ' : 'Against '}
              {enemy?.name ?? phase.enemy}
            </h3>
            {enemy && <EnemyDetails enemy={enemy} commander={enemyCommander} />}
          </section>
          <p className="muted">
            Each feat scores a point for each of its numbers you meet, if you win. The points of
            both set your reward; missing them costs nothing.
          </p>
          <div className="choice-list" data-testid="feats">
            {phase.offered.map((id, place) => {
              const feat = featDef(names, id)
              const on = ticked.includes(place)
              return (
                <button
                  key={id}
                  type="button"
                  className={on ? 'feat feat-ticked' : 'feat'}
                  aria-pressed={on}
                  onClick={() => tick(place)}
                  data-testid="feat"
                >
                  <strong>{feat?.name ?? id}</strong> {feat && featText(feat)}
                </button>
              )
            })}
          </div>
          <ul className="feat-rewards" data-testid="feat-rewards">
            {content.feat_rewards.map((reward, index) => {
              const next = content.feat_rewards[index + 1]
              const points =
                next === undefined
                  ? `${reward.points} or more`
                  : next.points - 1 === reward.points
                    ? `${reward.points}`
                    : `${reward.points} to ${next.points - 1}`
              return (
                <li key={reward.points}>
                  <strong>
                    {points} {points === '1' ? 'point' : 'points'}:
                  </strong>{' '}
                  {featRewardText(reward)}
                </li>
              )
            })}
          </ul>
          <div className="button-row">
            <button
              type="button"
              className="primary"
              disabled={!choice}
              onClick={() => choice && send(choice)}
              data-testid="start-duel"
            >
              {choice ? 'Start the duel' : `Choose ${picks - ticked.length} more`}
            </button>
          </div>
        </>
      )
      break
    }
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
    case 'draft':
      title =
        phase.picks_left === 1 ? 'Draft: choose one more card' : `Draft: choose ${phase.picks_left} cards`
      body = (
        <>
          <p className="muted">
            Picks left: <strong data-testid="draft-picks">{phase.picks_left}</strong>. The cards
            you leave are gone.
          </p>
          <div className="card-grid choice-row" data-testid="draft">
            {phase.choices.map((card, choice) => (
              <RunCard
                key={card}
                def={cards.get(card)}
                onPick={() => send({ type: 'take_draft', choice })}
                testId="draft-card"
              />
            ))}
          </div>
          <div className="button-row">
            <button type="button" onClick={() => send({ type: 'leave' })} data-testid="leave">
              Take no more
            </button>
          </div>
        </>
      )
      break
    case 'treasure': {
      const gold = can('take_gold')
      title = phase.relics.length > 0 ? 'Treasure: choose one' : 'Treasure'
      body = (
        <>
          {phase.relics.length === 0 && (
            <p className="muted">You already have every relic there is.</p>
          )}
          <div className="card-grid choice-row" data-testid="treasure">
            {phase.relics.map((id, choice) => {
              const relic = relicDef(names, id)
              return (
                <div key={id} className="run-card relic-ware" data-testid="treasure-relic">
                  <div className="relic-card">
                    <span className="muted">Relic</span>
                    <strong>{relic?.name ?? id}</strong>
                    <span>{relic?.text}</span>
                  </div>
                  <button type="button" onClick={() => send({ type: 'take_treasure', choice })}>
                    Take
                  </button>
                </div>
              )
            })}
          </div>
          <div className="button-row">
            <button
              type="button"
              className={phase.relics.length === 0 ? 'primary' : undefined}
              disabled={!gold}
              onClick={() => gold && send(gold)}
              data-testid="take-gold"
            >
              {phase.relics.length > 0 ? `Take ${phase.gold} gold instead` : `Take ${phase.gold} gold`}
            </button>
          </div>
        </>
      )
      break
    }
    case 'event': {
      const def = content.events.find((event) => event.id === phase.event)
      const takes = actions.flatMap((action) => (action.type === 'take_event' ? [action] : []))
      const unavailable = <span className="muted"> (Not possible for you now.)</span>
      title = def?.name ?? 'Event'
      body = (
        <>
          <p className="event-text" data-testid="event-text">
            {def?.text}
          </p>
          {phase.cards.length > 0 && (
            <div className="card-grid choice-row" data-testid="event-cards">
              {phase.cards.map((card) => (
                <RunCard key={card} def={cards.get(card)} testId="event-card" />
              ))}
            </div>
          )}
          <div className="choice-list" data-testid="event-options">
            {def?.options.flatMap((option, index) => {
              const mine = takes.filter((action) => action.option === index)
              // An option that gives a relic is shown once for each relic.
              if (option.effects.some((effect) => effect.type === 'gain_relic') && phase.relics.length > 0) {
                return phase.relics.map((id, pick) => {
                  const relic = relicDef(names, id)
                  const take = mine.find((action) => action.pick === pick)
                  return (
                    <button
                      key={`${index}-${id}`}
                      type="button"
                      disabled={!take}
                      onClick={() => take && send(take)}
                      data-testid="event-option"
                    >
                      {option.text} <strong>{relic?.name ?? id}</strong>{' '}
                      <span className="muted">{relic?.text}</span>
                      {!take && unavailable}
                    </button>
                  )
                })
              }
              // One that is about a card of the deck opens the deck to pick it.
              const fromDeck = option.effects.some((effect) => effect.type === 'transmute')
              const take = mine[0]
              return [
                <button
                  key={index}
                  type="button"
                  disabled={!take}
                  onClick={() => take && (fromDeck ? setPicking(index) : send(take))}
                  data-testid="event-option"
                >
                  {option.text}
                  {!take && unavailable}
                </button>,
              ]
            })}
            <button type="button" className="primary" onClick={() => send({ type: 'leave' })} data-testid="leave">
              Walk on
            </button>
          </div>
        </>
      )
      break
    }
    case 'rest': {
      const canHeal = actions.some((action) => action.type === 'heal_wounds')
      const carried = view.wounds.map((id) => woundDef(names, id)?.name ?? id).join(', ')
      title = 'Rest site: choose one'
      body = (
        <div className="choice-list">
          <button
            type="button"
            disabled={!canHeal}
            onClick={() => send({ type: 'heal_wounds' })}
            data-testid="rest-heal"
          >
            Heal all your wounds
            <span className="muted">{canHeal ? ` Every wound you carry is removed: ${carried}.` : ' You have none.'}</span>
          </button>
          <button
            type="button"
            disabled={upgradable.size === 0}
            onClick={() => setUpgrading(true)}
            data-testid="rest-upgrade"
          >
            Upgrade a card
            <span className="muted">
              {upgradable.size === 0
                ? ' You have none left to upgrade.'
                : ' It becomes its upgraded side for the rest of the run.'}
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
    case 'over':
      title = phase.won ? 'You won the act' : 'Your run is over'
      body = (
        <>
          <p data-testid="run-result">
            {phase.won
              ? 'The boss is beaten. That is the whole of the game so far.'
              : view.wounds.length >= view.max_wounds
                ? 'That was one wound too many.'
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
          className="wounds"
          data-testid="wounds"
          aria-label={`${wounds}: the last one ends the run`}
          title="A lost duel leaves a wound. The last one ends the run."
        >
          {Array.from({ length: view.max_wounds }, (_, index) => (
            <span key={index} className={index < view.wounds.length ? 'wound' : 'wound wound-free'} aria-hidden="true">
              {index < view.wounds.length ? '✖' : '○'}
            </span>
          ))}{' '}
          {view.wounds.length}/{view.max_wounds} wounds
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
          {boss && view.boss && phase.type !== 'over' && (
            <section className="panel boss-panel" data-testid="boss-panel">
              <h2>The boss</h2>
              <p>
                <strong data-testid="boss-name">{boss.name}</strong> starts with{' '}
                <span data-testid="boss-life">{view.boss.life}</span> life.
              </p>
              <EnemyDetails enemy={boss} commander={bossCommander} />
            </section>
          )}
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
          {emblem && (
            <section className="panel">
              <h2>Emblem</h2>
              <ul className="modifiers">
                <Modifier def={emblem} kind="starting-emblem" />
              </ul>
            </section>
          )}
          <section className="panel">
            <h2>Relics</h2>
            {view.relics.length === 0 ? (
              <p className="muted">None yet. Elites, treasures and shops have them.</p>
            ) : (
              <ul className="modifiers">
                {view.relics.map((id) => {
                  const relic = relicDef(names, id)
                  return relic && <Modifier key={id} def={relic} kind="relic" />
                })}
              </ul>
            )}
          </section>
          {view.next_duel.length > 0 && (
            <section className="panel">
              <h2>Your next duel</h2>
              <ul className="modifiers">
                {view.next_duel.map((id, index) => {
                  const modifier = nextDuelDef(names, id)
                  return modifier && <Modifier key={index} def={modifier} kind="next-duel" />
                })}
              </ul>
            </section>
          )}
          {view.wounds.length > 0 && (
            <section className="panel">
              <h2>Wounds</h2>
              <ul className="modifiers">
                {view.wounds.map((id, index) => {
                  const wound = woundDef(names, id)
                  return wound && <Modifier key={index} def={wound} kind="wound" />
                })}
              </ul>
              <p className="muted">A rest site heals them all.</p>
            </section>
          )}
        </aside>
      </div>

      {picking !== null && (
        <DeckView
          view={view}
          cards={cards}
          title="Choose a card"
          hint="Click the card to give up. It is gone for the rest of the run."
          pickable={
            new Set(
              actions.flatMap((action) =>
                action.type === 'take_event' && action.option === picking && action.pick !== null
                  ? [action.pick]
                  : [],
              ),
            )
          }
          onPick={(card) => send({ type: 'take_event', option: picking, pick: card })}
          onClose={() => setPicking(null)}
        />
      )}
      {deckOpen && !removing && !upgrading && picking === null && (
        <DeckView view={view} cards={cards} title="Your deck" onClose={() => setDeckOpen(false)} />
      )}
      {upgrading && (
        <DeckView
          view={view}
          cards={cards}
          title="Upgrade a card"
          hint="The cards you can upgrade are shown as they would become. Click one to upgrade it."
          pickable={upgradable}
          upgrades
          onPick={(card) => send({ type: 'upgrade_card', card })}
          onClose={() => setUpgrading(false)}
        />
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
