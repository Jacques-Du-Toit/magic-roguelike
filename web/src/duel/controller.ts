// Runs one duel for the screen: sends the human's actions to the engine,
// lets the AI answer, and plays each batch of events before showing the
// view that follows it.
//
// This is the only orchestration in TypeScript (docs/ARCHITECTURE.md §8) and
// it holds no rules: who acts next comes from the view's decision, what the
// human may do from `legalActions`, and whether to pass for them from the
// engine's `shouldAutoPass`.

import { AI, HUMAN } from '../engine/index.ts'
import type { Action } from '../engine/types/Action.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { DuelUpdate } from '../engine/types/DuelUpdate.ts'
import type { PlayerView } from '../engine/types/PlayerView.ts'
import {
  type Effects,
  type LogEntry,
  type NameContext,
  NO_EFFECTS,
  planPresentation,
} from './present.ts'

/** The part of the engine's duel handle the controller uses. */
export interface DuelEngine {
  view(): PlayerView
  legalActions(): Action[]
  shouldAutoPass(holdPriority: boolean): boolean
  apply(action: Action): DuelUpdate
  aiStep(): DuelUpdate
}

export interface DuelInfo {
  humanDeck: string
  aiDeck: string
  seed: string
}

/** Everything the duel screen renders. Replaced, never mutated. */
export interface DuelSnapshot {
  info: DuelInfo
  /** The view on screen. It trails the engine while events play. */
  view: PlayerView
  /** What the human may do now. Empty while events play or the AI acts. */
  actions: Action[]
  /** Events are playing or the opponent is acting. */
  busy: boolean
  holdPriority: boolean
  effects: Effects
  log: LogEntry[]
  /** An engine error, if one stopped the duel. */
  error: string | null
  /** Every action the human has taken: with the seed, it replays the game. */
  history: Action[]
}

export type Sleep = (ms: number) => Promise<void>

const realSleep: Sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

/** The AI could not need more steps than this between two human decisions. */
const MAX_STEPS_WITHOUT_HUMAN = 10_000

export class DuelController {
  readonly #engine: DuelEngine
  readonly #cards: ReadonlyMap<string, CardDef>
  readonly #sleep: Sleep
  readonly #listeners = new Set<() => void>()
  /** Card definition ids of every object seen so far, for naming things. */
  readonly #objectDefs = new Map<number, string>()
  /** Names of the emblems, which are not cards. */
  readonly #emblemNames = new Map<number, string>()
  #snapshot: DuelSnapshot
  #disposed = false

  constructor(
    engine: DuelEngine,
    info: DuelInfo,
    cards: ReadonlyMap<string, CardDef>,
    sleep: Sleep = realSleep,
  ) {
    this.#engine = engine
    this.#cards = cards
    this.#sleep = sleep
    const view = engine.view()
    this.#remember(view)
    this.#snapshot = {
      info,
      view,
      actions: [],
      busy: true,
      holdPriority: false,
      effects: NO_EFFECTS,
      log: [],
      error: null,
      history: [],
    }
  }

  subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener)
    return () => this.#listeners.delete(listener)
  }

  getSnapshot = (): DuelSnapshot => this.#snapshot

  /** Stops everything. The controller can't be used afterwards. */
  dispose(): void {
    this.#disposed = true
    this.#listeners.clear()
  }

  /** Begins the duel: runs until the human has something to decide. */
  start(): Promise<void> {
    return this.#guard(() => this.#run())
  }

  /** Takes one of the human's legal actions. Ignored while busy. */
  act(action: Action): Promise<void> {
    if (this.#snapshot.busy || this.#disposed) return Promise.resolve()
    return this.#guard(async () => {
      await this.#applyHuman(action)
      await this.#run()
    })
  }

  /**
   * Takes the action `pick` chooses from the legal ones, again and again
   * until it chooses none, e.g. to attack with every creature.
   */
  actWhile(pick: (actions: Action[]) => Action | undefined): Promise<void> {
    if (this.#snapshot.busy || this.#disposed) return Promise.resolve()
    return this.#guard(async () => {
      for (let count = 0; count < MAX_STEPS_WITHOUT_HUMAN; count += 1) {
        const action = pick(this.#engine.legalActions())
        if (!action) break
        await this.#applyHuman(action)
      }
      await this.#run()
    })
  }

  setHoldPriority(holdPriority: boolean): void {
    this.#set({ holdPriority })
  }

  #set(change: Partial<DuelSnapshot>): void {
    if (this.#disposed) return
    this.#snapshot = { ...this.#snapshot, ...change }
    for (const listener of this.#listeners) listener()
  }

  /** Runs `work` with the screen marked busy; an engine error ends up on screen. */
  async #guard(work: () => Promise<void>): Promise<void> {
    this.#set({ busy: true, actions: [] })
    let error: string | null = this.#snapshot.error
    try {
      await work()
    } catch (thrown) {
      error = thrown instanceof Error ? thrown.message : String(thrown)
    }
    if (this.#disposed) return
    // Whatever happened, show where the engine really is and what it allows.
    const view = this.#engine.view()
    this.#remember(view)
    this.#set({
      view,
      effects: NO_EFFECTS,
      actions: this.#engine.legalActions(),
      busy: false,
      error,
    })
  }

  async #applyHuman(action: Action): Promise<void> {
    const update = this.#engine.apply(action)
    this.#set({ history: [...this.#snapshot.history, action] })
    await this.#present(update)
  }

  /** Lets the AI act, and passes for the human, until the human must decide. */
  async #run(): Promise<void> {
    for (let count = 0; count < MAX_STEPS_WITHOUT_HUMAN; count += 1) {
      if (this.#disposed) return
      const decision = this.#engine.view().decision
      if (decision === null) return
      if (decision.player === AI) {
        await this.#present(this.#engine.aiStep())
      } else if (this.#engine.shouldAutoPass(this.#snapshot.holdPriority)) {
        await this.#applyHuman({ type: 'pass_priority' })
      } else {
        return
      }
    }
    throw new Error('the opponent kept acting without the game moving on')
  }

  /** Plays one batch of events, then shows the view that follows it. */
  async #present(update: DuelUpdate): Promise<void> {
    if (this.#disposed) return
    this.#remember(update.view)
    const context: NameContext = {
      viewer: HUMAN,
      cards: this.#cards,
      objectName: (object) => {
        const emblem = this.#emblemNames.get(object)
        if (emblem !== undefined) return emblem
        const def = this.#objectDefs.get(object)
        return def === undefined ? undefined : (this.#cards.get(def)?.name ?? def)
      },
    }
    for (const event of update.events) {
      if ('card' in event && event.card.face) {
        this.#objectDefs.set(event.card.object, event.card.face.def)
      }
    }
    const plan = planPresentation(update.events, context)
    for (const beat of plan.beats) {
      this.#set({ effects: beat.effects })
      await this.#sleep(beat.ms)
      if (this.#disposed) return
    }
    this.#set({
      view: update.view,
      effects: NO_EFFECTS,
      log: plan.log.length > 0 ? [...this.#snapshot.log, ...plan.log] : this.#snapshot.log,
    })
    if (plan.holdMs > 0) await this.#sleep(plan.holdMs)
  }

  #remember(view: PlayerView): void {
    for (const permanent of view.battlefield) this.#objectDefs.set(permanent.object, permanent.def)
    for (const item of view.stack) {
      if (item.kind.type === 'spell') this.#objectDefs.set(item.id, item.kind.card.def)
    }
    for (const player of view.players) {
      for (const emblem of player.emblems) this.#emblemNames.set(emblem.id, emblem.name)
      const commander = player.commander
      if (commander && commander.object !== null) {
        this.#objectDefs.set(commander.object, commander.def)
      }
      for (const card of [...player.hand, ...player.graveyard]) {
        if (card.face) this.#objectDefs.set(card.object, card.face.def)
      }
    }
  }
}
