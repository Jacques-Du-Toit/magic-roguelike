// The duel: renders the controller's snapshot and turns clicks into the
// engine's legal actions.

import { type ReactNode, useEffect, useState, useSyncExternalStore } from 'react'
import { Battlefield, type Highlight } from '../components/Battlefield.tsx'
import { CardBack, CardFrame } from '../components/Card.tsx'
import { DamagePanel } from '../components/DamagePanel.tsx'
import { Hand } from '../components/Hand.tsx'
import { GameOverOverlay, GraveyardOverlay, MulliganOverlay } from '../components/Overlays.tsx'
import { LogPanel, StackPanel } from '../components/SidePanel.tsx'
import type { DuelController } from '../duel/controller.ts'
import {
  type ActivateClick,
  type Activating,
  type BlockClick,
  type Casting,
  type Click,
  activationStep,
  blockOptions,
  canActivate,
  cardActions,
  clickAbilitySource,
  clickActivationChoice,
  clickAimTarget,
  clickAttacker,
  clickBlockedAttacker,
  clickBlocker,
  clickHandCard,
  clickTarget,
  completedCast,
  isAiming,
  nextActivationChoices,
  nextAimTargets,
  nextTargets,
  simpleAction,
} from '../duel/interaction.ts'
import { stackItemName } from '../duel/names.ts'
import { type Effects, targetKey } from '../duel/present.ts'
import {
  PHASES,
  type PromptMode,
  confirmAttackersLabel,
  confirmBlockersLabel,
  passLabel,
  phaseIndex,
  promptText,
} from '../duel/prompt.ts'
import { AI, HUMAN, playerIndex } from '../engine/index.ts'
import type { Action } from '../engine/types/Action.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { PermanentView } from '../engine/types/PermanentView.ts'
import type { PlayerId } from '../engine/types/PlayerId.ts'
import type { PlayerSummary } from '../engine/types/PlayerSummary.ts'
import type { Target } from '../engine/types/Target.ts'

export type Speed = 'normal' | 'fast' | 'instant'

/** How to start the next game from the duel screen. */
export type NextGame = 'new_seed' | 'same_seed' | 'menu'

/** Where the duel screen leads: another practice game, or back to the run. */
export type DuelExit =
  | { kind: 'practice'; onNextGame: (next: NextGame) => void }
  | {
      kind: 'run'
      /** The player's hearts, in words. */
      hearts: string
      /** What losing this duel costs, in words. */
      stakes: string
      onConcede: () => void
      /** Leave the finished duel and take its result into the run. */
      onContinue: () => void
    }

type Mode =
  | { kind: 'idle' }
  | { kind: 'casting'; casting: Casting }
  /** Aiming a triggered ability: the targets picked so far. */
  | { kind: 'aiming'; chosen: Target[] }
  /** Activating a permanent's ability: what has been picked for it so far. */
  | { kind: 'activating'; activating: Activating }
  | { kind: 'blocking'; blocker: number }

const IDLE: Mode = { kind: 'idle' }

interface PlayerPanelProps {
  name: string
  /** Said after the name while it is this player's turn. */
  turnLabel: string
  children?: ReactNode
  deck: string
  summary: PlayerSummary
  effects: Effects
  active: boolean
  targetable: boolean
  /** Something on the stack targets this player. */
  targeted: boolean
  onTarget: () => void
  onGraveyard: () => void
}

function PlayerPanel({
  name,
  turnLabel,
  children,
  deck,
  summary,
  effects,
  active,
  targetable,
  targeted,
  onTarget,
  onGraveyard,
}: PlayerPanelProps) {
  const floats = effects.floats.filter((float) => float.key === `p:${summary.player}`)
  return (
    <div
      className={`player-panel${active ? ' player-active' : ''}`}
      data-player={summary.player}
      data-testid={`player-${summary.player}`}
    >
      <button
        type="button"
        className={`life${targetable ? ' lit-target' : ''}`}
        onClick={onTarget}
        aria-label={`${name}: ${summary.life} life${targetable ? ', can be targeted' : ''}`}
      >
        <span className="life-total" data-testid={`life-${summary.player}`}>
          {summary.life}
        </span>
        <span className="life-label">life</span>
        {floats.map((float, index) => (
          <span key={index} className={`float float-${float.tone}`}>
            {float.text}
          </span>
        ))}
      </button>
      <div className="player-facts">
        <strong>
          {name}
          {active ? ` · ${turnLabel}` : ''}
        </strong>
        {targeted && <span className="badge badge-targeted">Targeted</span>}
        <span className="muted">{deck}</span>
        <span>
          Hand {summary.hand.length} · Library {summary.library_size}
        </span>
        <button type="button" className="link" onClick={onGraveyard}>
          Graveyard {summary.graveyard.length}
        </button>
        {summary.emblems.length > 0 && (
          <ul className="emblems" aria-label={`${name}: emblems`}>
            {summary.emblems.map((emblem) => (
              <li key={emblem.id} className="emblem" data-testid="emblem">
                <strong>{emblem.name}</strong> {emblem.text}
              </li>
            ))}
          </ul>
        )}
        {children}
      </div>
    </div>
  )
}

interface DuelScreenProps {
  controller: DuelController
  cards: ReadonlyMap<string, CardDef>
  deckNames: ReadonlyMap<string, string>
  speed: Speed
  onSpeed: (speed: Speed) => void
  exit: DuelExit
}

export function DuelScreen({
  controller,
  cards,
  deckNames,
  speed,
  onSpeed,
  exit,
}: DuelScreenProps) {
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot)
  const { view, actions, busy, effects, info } = snapshot

  // What is being pointed at only makes sense for the actions it was started
  // under: any change of legal actions drops it.
  const [modeState, setModeState] = useState<{ mode: Mode; actions: Action[] }>({
    mode: IDLE,
    actions,
  })
  const pointing = modeState.actions === actions ? modeState.mode : IDLE
  // An ability waiting for its targets is aimed without being asked to.
  const mode: Mode =
    pointing.kind === 'idle' && isAiming(actions) ? { kind: 'aiming', chosen: [] } : pointing
  const setMode = (next: Mode) => setModeState({ mode: next, actions })

  const [graveyardOf, setGraveyardOf] = useState<PlayerId | null>(null)
  const [resultDismissed, setResultDismissed] = useState(false)
  const [copied, setCopied] = useState(false)
  const [conceding, setConceding] = useState(false)

  const you = view.players[playerIndex(HUMAN)]
  const opponent = view.players[playerIndex(AI)]
  const name = (def: string | undefined) => (def ? (cards.get(def)?.name ?? def) : 'a card')
  const permanentName = (object: number) =>
    name(view.battlefield.find((permanent) => permanent.object === object)?.def)

  const targetName = (target: Target): string => {
    if (target.type === 'player') return target.player === HUMAN ? 'you' : 'the opponent'
    const item = view.stack.find((entry) => entry.id === target.object)
    return item ? stackItemName(item, cards) : permanentName(target.object)
  }

  const send = (action: Action) => {
    setMode(IDLE)
    void controller.act(action)
  }
  const resolve = (click: Click | BlockClick): boolean => {
    if (click.kind === 'send') send(click.action)
    else if (click.kind === 'cast') setMode({ kind: 'casting', casting: click.casting })
    else if (click.kind === 'pick_attacker') setMode({ kind: 'blocking', blocker: click.blocker })
    return click.kind !== 'nothing'
  }

  const resolveActivation = (click: ActivateClick): boolean => {
    if (click.kind === 'send') send(click.action)
    else if (click.kind === 'activate') {
      setMode({ kind: 'activating', activating: click.activating })
    }
    return click.kind !== 'nothing'
  }

  const onHandCard = (object: number) => {
    if (mode.kind === 'casting' && mode.casting.card === object) setMode(IDLE)
    else resolve(clickHandCard(actions, object))
  }
  const onTarget = (target: Target) => {
    if (mode.kind === 'casting') resolve(clickTarget(actions, mode.casting, target))
    if (mode.kind === 'aiming') {
      const click = clickAimTarget(actions, mode.chosen, target)
      if (click.kind === 'send') send(click.action)
      else if (click.kind === 'aim') setMode({ kind: 'aiming', chosen: click.chosen })
    }
    if (mode.kind === 'activating') {
      resolveActivation(clickActivationChoice(actions, mode.activating, target))
    }
  }
  const onPermanent = (permanent: PermanentView) => {
    const object = permanent.object
    if (mode.kind === 'casting' || mode.kind === 'aiming') {
      onTarget({ type: 'object', object })
    } else if (mode.kind === 'activating') {
      // Clicking the permanent again, when it isn't itself a choice, puts
      // its ability down.
      if (!resolveActivation(clickActivationChoice(actions, mode.activating, { type: 'object', object }))) {
        if (object === mode.activating.source) setMode(IDLE)
      }
    } else if (mode.kind === 'blocking') {
      if (!resolve(clickBlockedAttacker(actions, mode.blocker, object))) {
        // Clicking the picked-up creature again puts it down.
        if (object === mode.blocker || !resolve(clickBlocker(actions, object))) setMode(IDLE)
      }
    } else if (!resolve(clickAttacker(actions, object)) && !resolve(clickBlocker(actions, object))) {
      resolveActivation(clickAbilitySource(actions, object))
    }
  }

  const targets =
    mode.kind === 'casting'
      ? new Set(nextTargets(actions, mode.casting).map(targetKey))
      : mode.kind === 'aiming'
        ? new Set(nextAimTargets(actions, mode.chosen).map(targetKey))
        : mode.kind === 'activating'
          ? new Set(nextActivationChoices(actions, mode.activating).map(targetKey))
          : new Set<string>()
  // What the spells and abilities on the stack are aimed at.
  const aimedAt = new Set(view.stack.flatMap((item) => item.targets.map(targetKey)))
  const blockTargets = mode.kind === 'blocking' ? blockOptions(actions, mode.blocker) : []
  const highlight = (object: number): Highlight => {
    if (mode.kind === 'casting' || mode.kind === 'aiming') {
      return targets.has(`o:${object}`) ? 'target' : null
    }
    if (mode.kind === 'activating') {
      if (targets.has(`o:${object}`)) return 'target'
      return object === mode.activating.source ? 'selected' : null
    }
    if (mode.kind === 'blocking') {
      if (object === mode.blocker) return 'selected'
      return blockTargets.includes(object) ? 'target' : null
    }
    const usable =
      clickAttacker(actions, object).kind !== 'nothing' ||
      clickBlocker(actions, object).kind !== 'nothing' ||
      canActivate(actions, object)
    return usable ? 'action' : null
  }

  const pass = simpleAction(actions, 'pass_priority')
  const confirmAttackers = simpleAction(actions, 'confirm_attackers')
  const confirmBlockers = simpleAction(actions, 'confirm_blockers')
  const reset = simpleAction(actions, 'reset_declaration')
  const confirmDamage = simpleAction(actions, 'confirm_damage')
  // The division of combat damage the human is being asked for, if any.
  const dividing =
    !busy && view.decision?.type === 'assign_combat_damage' && view.decision.player === HUMAN
      ? view.decision.attacker
      : null
  const assignment =
    dividing === null
      ? undefined
      : view.combat?.assignments.find((entry) => entry.attacker === dividing && !entry.confirmed)
  const scryKeep = simpleAction(actions, 'scry_keep')
  const scryBottom = simpleAction(actions, 'scry_bottom')
  const scryGraveyard = simpleAction(actions, 'scry_graveyard')
  // A "you may" ability can be declined: the choice of no targets.
  const decline =
    mode.kind === 'aiming'
      ? actions.find((action) => action.type === 'choose_targets' && action.targets.length === 0)
      : undefined
  // Cards in a graveyard that can be picked now, with whose they are.
  const graveyardChoices = view.players.flatMap((player) =>
    player.graveyard.flatMap((card) =>
      targets.has(`o:${card.object}`) && card.face
        ? [{ object: card.object, def: cards.get(card.face.def), yours: player.player === HUMAN }]
        : [],
    ),
  )
  const lookingAt = view.looking_at?.face ? cards.get(view.looking_at.face.def) : undefined
  const keep = simpleAction(actions, 'keep')
  const mulligan = simpleAction(actions, 'mulligan')
  const canAttack = actions.some((action) => action.type === 'attack')
  const castNow = mode.kind === 'casting' ? completedCast(actions, mode.casting) : undefined
  const primary = pass ?? confirmAttackers ?? confirmBlockers ?? confirmDamage

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const element = event.target instanceof HTMLElement ? event.target.tagName : ''
      if (event.key === 'Escape') setModeState((current) => ({ ...current, mode: IDLE }))
      // A focused button already answers the space bar itself.
      const typing = ['BUTTON', 'INPUT', 'SELECT', 'TEXTAREA'].includes(element)
      if (event.key === ' ' && primary && !typing) {
        event.preventDefault()
        void controller.act(primary)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [controller, primary])

  const promptMode: PromptMode =
    mode.kind === 'casting'
      ? {
          kind: 'casting',
          cardName: name(you.hand.find((card) => card.object === mode.casting.card)?.face?.def),
        }
      : mode.kind === 'blocking'
        ? { kind: 'blocking', blockerName: permanentName(mode.blocker) }
        : mode.kind === 'activating'
          ? {
              kind: 'activating',
              sourceName: permanentName(mode.activating.source),
              step: activationStep(actions, mode.activating),
            }
          : { kind: 'idle' }
  const top = view.stack.at(-1)
  const prompt = promptText({
    view,
    busy,
    mode: promptMode,
    topOfStack: top ? stackItemName(top, cards) : null,
    canConfirmBlocks: confirmBlockers !== undefined,
    abilitySource: view.aiming ?? undefined,
  })

  const copyReport = () => {
    const report = JSON.stringify({ ...info, error: snapshot.error, actions: snapshot.history })
    void navigator.clipboard.writeText(report).then(() => setCopied(true))
  }

  const inMulligans = view.turn === 0 && view.result === null
  const playable = (object: number) => cardActions(actions, object).length > 0
  const phase = phaseIndex(view.step)
  const deckName = (deck: string) => deckNames.get(deck) ?? deck

  return (
    <div className="duel" data-busy={busy} data-testid="duel">
      <header className="topbar">
        <span className="turn" data-testid="turn">
          {view.turn === 0
            ? 'Before the game'
            : `Turn ${view.turn} · ${view.active_player === HUMAN ? 'your turn' : "opponent's turn"}`}
        </span>
        <ol className="phases" aria-label="Phases">
          {PHASES.map((label, index) => (
            <li
              key={label}
              className={index === phase && view.turn > 0 ? 'phase-current' : ''}
              aria-current={index === phase && view.turn > 0 ? 'step' : undefined}
            >
              {label}
            </li>
          ))}
        </ol>
        <span className="topbar-tools">
          <label>
            Speed{' '}
            <select value={speed} onChange={(event) => onSpeed(event.target.value as Speed)}>
              <option value="normal">Normal</option>
              <option value="fast">Fast</option>
              <option value="instant">Instant</option>
            </select>
          </label>
          <span className="muted" data-testid="seed">
            Seed {info.seed}
          </span>
          <button type="button" className="link" onClick={copyReport}>
            {copied ? 'Copied' : 'Copy bug report'}
          </button>
          {exit.kind === 'practice' && (
            <button type="button" className="link" onClick={() => exit.onNextGame('menu')}>
              Quit
            </button>
          )}
          {exit.kind === 'run' && (
            <span className="muted" data-testid="stakes">
              {exit.hearts}. {exit.stakes}
            </span>
          )}
          {exit.kind === 'run' && view.result === null && !conceding && (
            <button type="button" className="link" onClick={() => setConceding(true)}>
              Concede
            </button>
          )}
          {exit.kind === 'run' && view.result === null && conceding && (
            <>
              <span>Concede? It counts as a loss.</span>
              <button type="button" className="link" onClick={exit.onConcede} data-testid="concede">
                Yes, concede
              </button>
              <button type="button" className="link" onClick={() => setConceding(false)}>
                No
              </button>
            </>
          )}
        </span>
      </header>

      <div className="duel-main">
        <div className="players">
          <PlayerPanel
            name="Opponent"
            turnLabel="their turn"
            deck={deckName(info.aiDeck)}
            summary={opponent}
            effects={effects}
            active={view.active_player === AI && view.turn > 0}
            targetable={targets.has(`p:${AI}`)}
            targeted={aimedAt.has(`p:${AI}`)}
            onTarget={() => onTarget({ type: 'player', player: AI })}
            onGraveyard={() => setGraveyardOf(AI)}
          >
            <div className="opponent-hand" aria-label={`Opponent's hand: ${opponent.hand.length} cards`}>
              {opponent.hand.map((card) => (
                <CardBack key={card.object} />
              ))}
            </div>
          </PlayerPanel>
          <PlayerPanel
            name="You"
            turnLabel="your turn"
            deck={deckName(info.humanDeck)}
            summary={you}
            effects={effects}
            active={view.active_player === HUMAN && view.turn > 0}
            targetable={targets.has(`p:${HUMAN}`)}
            targeted={aimedAt.has(`p:${HUMAN}`)}
            onTarget={() => onTarget({ type: 'player', player: HUMAN })}
            onGraveyard={() => setGraveyardOf(HUMAN)}
          />
        </div>

        <Battlefield
          view={view}
          cards={cards}
          effects={effects}
          highlight={highlight}
          targeted={(object) => aimedAt.has(`o:${object}`)}
          onPermanent={onPermanent}
        />

        <div className="prompt-bar">
          {snapshot.error && (
            <p role="alert" className="error">
              The rules engine reported a problem: {snapshot.error}{' '}
              <button type="button" className="link" onClick={copyReport}>
                {copied ? 'Copied' : 'Copy bug report'}
              </button>
            </p>
          )}
          <p className="prompt-text" data-testid="prompt">
            {prompt}
          </p>
          {scryKeep && !busy && (
            <div className="scry-panel" data-testid="scry-panel">
              {lookingAt && <CardFrame def={lookingAt} size="normal" />}
            </div>
          )}
          {graveyardChoices.length > 0 && (
            <div className="graveyard-choices" data-testid="graveyard-choices">
              {graveyardChoices.map(
                (choice) =>
                  choice.def && (
                    <button
                      key={choice.object}
                      type="button"
                      className="graveyard-choice lit-target"
                      data-object={choice.object}
                      onClick={() => onTarget({ type: 'object', object: choice.object })}
                      aria-label={`${choice.def.name}, in ${choice.yours ? 'your' : "the opponent's"} graveyard`}
                    >
                      <CardFrame def={choice.def} size="small" />
                    </button>
                  ),
              )}
            </div>
          )}
          {assignment && (
            <DamagePanel
              assignment={assignment}
              actions={actions}
              attackerName={permanentName(assignment.attacker)}
              targetName={targetName}
              onAction={send}
            />
          )}
          <div className="button-row">
            <label className="hold">
              <input
                type="checkbox"
                checked={snapshot.holdPriority}
                onChange={(event) => controller.setHoldPriority(event.target.checked)}
              />{' '}
              Hold priority
            </label>
            {mode.kind !== 'idle' && mode.kind !== 'aiming' && (
              <button type="button" onClick={() => setMode(IDLE)}>
                Cancel
              </button>
            )}
            {mode.kind === 'aiming' && mode.chosen.length > 0 && (
              <button type="button" onClick={() => setMode(IDLE)}>
                Choose again
              </button>
            )}
            {decline && (
              <button type="button" onClick={() => send(decline)}>
                Don't use it
              </button>
            )}
            {scryBottom && (
              <button type="button" onClick={() => send(scryBottom)}>
                Put on the bottom
              </button>
            )}
            {scryGraveyard && (
              <button type="button" onClick={() => send(scryGraveyard)}>
                Put in the graveyard
              </button>
            )}
            {scryKeep && (
              <button type="button" className="primary" onClick={() => send(scryKeep)}>
                Keep on top
              </button>
            )}
            {castNow && (
              <button type="button" onClick={() => send(castNow)}>
                Cast with these targets
              </button>
            )}
            {canAttack && (
              <button
                type="button"
                onClick={() =>
                  void controller.actWhile((legal) => legal.find((action) => action.type === 'attack'))
                }
              >
                Attack with all
              </button>
            )}
            {reset && (
              <button type="button" onClick={() => send(reset)}>
                Reset
              </button>
            )}
            {confirmAttackers && (
              <button type="button" className="primary" onClick={() => send(confirmAttackers)}>
                {confirmAttackersLabel(view)}
              </button>
            )}
            {confirmBlockers && (
              <button type="button" className="primary" onClick={() => send(confirmBlockers)}>
                {confirmBlockersLabel(view)}
              </button>
            )}
            {assignment && (
              <button
                type="button"
                className="primary"
                disabled={!confirmDamage}
                onClick={() => confirmDamage && send(confirmDamage)}
              >
                Deal the damage
              </button>
            )}
            {pass && (
              <button type="button" className="primary" onClick={() => send(pass)}>
                {passLabel(view)}
              </button>
            )}
            {view.result && (
              <button type="button" className="primary" onClick={() => setResultDismissed(false)}>
                Show result
              </button>
            )}
          </div>
        </div>

        {!inMulligans && (
          <Hand
            hand={you.hand}
            cards={cards}
            playable={playable}
            selected={mode.kind === 'casting' ? mode.casting.card : null}
            onCard={onHandCard}
          />
        )}
      </div>

      <aside className="duel-side">
        <StackPanel
          view={view}
          cards={cards}
          targetName={targetName}
          isTarget={(object) => targets.has(`o:${object}`)}
          onItem={(item) => onTarget({ type: 'object', object: item.id })}
        />
        <LogPanel log={snapshot.log} />
      </aside>

      {inMulligans && (
        <MulliganOverlay
          view={view}
          cards={cards}
          prompt={prompt}
          playable={playable}
          onCard={onHandCard}
          buttons={
            <>
              {mulligan && (
                <button type="button" onClick={() => send(mulligan)}>
                  Mulligan
                </button>
              )}
              {keep && (
                <button type="button" className="primary" onClick={() => send(keep)}>
                  Keep
                </button>
              )}
            </>
          }
        />
      )}

      {graveyardOf && (
        <GraveyardOverlay
          title={graveyardOf === HUMAN ? 'Your graveyard' : "Opponent's graveyard"}
          graveyard={view.players[playerIndex(graveyardOf)].graveyard}
          cards={cards}
          onClose={() => setGraveyardOf(null)}
        />
      )}

      {view.result && !busy && !resultDismissed && (
        <GameOverOverlay result={view.result} turn={view.turn} seed={info.seed}>
          <button type="button" onClick={() => setResultDismissed(true)}>
            Look at the board
          </button>
          {exit.kind === 'practice' ? (
            <>
              <button type="button" onClick={() => exit.onNextGame('menu')}>
                Change decks
              </button>
              <button type="button" onClick={() => exit.onNextGame('same_seed')}>
                Replay this seed
              </button>
              <button type="button" className="primary" onClick={() => exit.onNextGame('new_seed')}>
                Play again
              </button>
            </>
          ) : (
            <button type="button" className="primary" onClick={exit.onContinue} data-testid="continue">
              Continue
            </button>
          )}
        </GameOverOverlay>
      )}
    </div>
  )
}
