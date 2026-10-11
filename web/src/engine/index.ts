// Wasm loading and the typed wrapper around the engine. This is the only
// module that talks to wasm (see docs/ARCHITECTURE.md §9). Data crosses the
// boundary as JSON strings and is typed with the ts-rs generated types.

import init, { Duel as WasmDuel, card_defs, decks, engine_info } from '../wasm/engine_wasm.js'
import type { Action } from './types/Action.ts'
import type { CardDef } from './types/CardDef.ts'
import type { DeckDef } from './types/DeckDef.ts'
import type { DuelUpdate } from './types/DuelUpdate.ts'
import type { EngineInfo } from './types/EngineInfo.ts'
import type { PlayerId } from './types/PlayerId.ts'
import type { PlayerView } from './types/PlayerView.ts'

let loading: Promise<void> | undefined

/** Loads the wasm module. Safe to call more than once. */
export function loadEngine(): Promise<void> {
  loading ??= init().then(() => undefined)
  return loading
}

/** Name and version of the engine build. Call after `loadEngine` resolves. */
export function engineInfo(): EngineInfo {
  return JSON.parse(engine_info()) as EngineInfo
}

/** Every card definition, in id order. */
export function cardDefs(): CardDef[] {
  return JSON.parse(card_defs()) as CardDef[]
}

/** Every decklist, in id order. */
export function deckDefs(): DeckDef[] {
  return JSON.parse(decks()) as DeckDef[]
}

/** The seats of a duel against the AI: the engine-wasm crate fixes them. */
export const HUMAN: PlayerId = 'P1'
export const AI: PlayerId = 'P2'

/**
 * A new random seed: a u64 as a decimal string, which is how seeds cross the
 * boundary (a JS number would lose precision).
 */
export function randomSeed(): string {
  const [seed] = crypto.getRandomValues(new BigUint64Array(1))
  return seed.toString()
}

/**
 * A duel between the human and the AI. The state lives in wasm memory; this
 * is a handle to it. Every method throws an `Error` with the engine's
 * message if the engine refuses.
 */
export class Duel {
  readonly #duel: WasmDuel

  constructor(humanDeck: string, aiDeck: string, seed: string) {
    this.#duel = new WasmDuel(humanDeck, aiDeck, seed)
  }

  get seed(): string {
    return this.#duel.seed()
  }

  /** The duel as the human sees it. */
  view(): PlayerView {
    return JSON.parse(this.#duel.view()) as PlayerView
  }

  /** The human's legal actions; empty unless the game is waiting on them. */
  legalActions(): Action[] {
    return JSON.parse(this.#duel.legal_actions()) as Action[]
  }

  hasMeaningfulAction(): boolean {
    return this.#duel.has_meaningful_action()
  }

  /**
   * Whether to pass priority for the human without asking. The rule is the
   * engine's; `holdPriority` is the human's toggle.
   */
  shouldAutoPass(holdPriority: boolean): boolean {
    return this.#duel.should_auto_pass(holdPriority)
  }

  /** Applies one of the human's legal actions. */
  apply(action: Action): DuelUpdate {
    return JSON.parse(this.#duel.apply(JSON.stringify(action))) as DuelUpdate
  }

  /** Lets the AI take one action. */
  aiStep(): DuelUpdate {
    return JSON.parse(this.#duel.ai_step()) as DuelUpdate
  }

  /** Releases the wasm memory. The handle can't be used afterwards. */
  free(): void {
    this.#duel.free()
  }
}
