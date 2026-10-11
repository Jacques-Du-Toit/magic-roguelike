// The words on the duel screen: what the game is waiting for, and what the
// buttons say. Wording only; the buttons themselves come from the engine's
// legal actions.

import { AI, HUMAN } from '../engine/index.ts'
import type { PlayerView } from '../engine/types/PlayerView.ts'
import type { Step } from '../engine/types/Step.ts'

/** Where the turn has got to, as said to a player who has priority there. */
const PRIORITY_MOMENTS: Record<Step, string> = {
  untap: 'Untap step',
  upkeep: 'Upkeep',
  draw: 'Draw step',
  precombat_main: 'Main phase',
  begin_combat: 'Beginning of combat',
  declare_attackers: 'Attackers are declared',
  declare_blockers: 'Blockers are declared',
  first_strike_damage: 'First-strike damage has been dealt',
  combat_damage: 'Combat damage has been dealt',
  end_combat: 'End of combat',
  postcombat_main: 'Second main phase',
  end: 'End step',
  cleanup: 'Cleanup step',
}

/** The turn as the strip across the screen shows it. */
export const PHASES = ['Upkeep', 'Draw', 'Main 1', 'Combat', 'Main 2', 'End'] as const

export function phaseIndex(step: Step): number {
  switch (step) {
    case 'untap':
    case 'upkeep':
      return 0
    case 'draw':
      return 1
    case 'precombat_main':
      return 2
    case 'begin_combat':
    case 'declare_attackers':
    case 'declare_blockers':
    case 'first_strike_damage':
    case 'combat_damage':
    case 'end_combat':
      return 3
    case 'postcombat_main':
      return 4
    case 'end':
    case 'cleanup':
      return 5
  }
}

function plural(count: number, noun: string): string {
  return count === 1 ? `1 ${noun}` : `${count} ${noun}s`
}

/** What is being pointed at with the mouse, on top of the engine's decision. */
export type PromptMode =
  | { kind: 'idle' }
  | { kind: 'casting'; cardName: string }
  | { kind: 'blocking'; blockerName: string }

export interface PromptInput {
  view: PlayerView
  busy: boolean
  mode: PromptMode
  /** While a triggered ability is being aimed: the name of what it is on. */
  abilitySource?: string
  /** The name of the top item of the stack, if there is one. */
  topOfStack: string | null
  /** Whether confirming the blocks is among the legal actions. */
  canConfirmBlocks: boolean
}

/** One line saying what the game is waiting for. */
export function promptText({
  view,
  busy,
  mode,
  topOfStack,
  canConfirmBlocks,
  abilitySource,
}: PromptInput): string {
  if (view.result) return 'The game is over.'
  const decision = view.decision
  if (busy || decision === null || decision.player === AI) {
    return decision?.player === AI ? 'Waiting for the opponent…' : 'Resolving…'
  }
  if (mode.kind === 'casting') return `Choose a target for ${mode.cardName}, or cancel.`
  if (mode.kind === 'blocking') return `Click the attacker ${mode.blockerName} should block.`

  switch (decision.type) {
    case 'mulligan':
      return 'Keep this hand, or mulligan?'
    case 'bottom_cards':
      return `Click ${plural(decision.remaining, 'card')} in your hand to put on the bottom of your library.`
    case 'discard':
      // At cleanup it is the hand size; anywhere else, a spell or ability.
      return view.step === 'cleanup'
        ? `You have too many cards: click ${plural(decision.remaining, 'card')} in your hand to discard.`
        : `You must discard: click ${plural(decision.remaining, 'card')} in your hand.`
    case 'choose_targets':
      return abilitySource
        ? `Choose a target for ${abilitySource}'s ability.`
        : 'Choose a target for the ability.'
    case 'scry':
      return decision.remaining > 1
        ? `Scry: keep this card on top of your library, or put it on the bottom? (${decision.remaining} to look at)`
        : 'Scry: keep this card on top of your library, or put it on the bottom?'
    case 'declare_attackers':
      return 'Declare attackers: click your creatures to attack with them, then confirm.'
    case 'declare_blockers':
      return canConfirmBlocks
        ? 'Declare blockers: click your creatures to block with them, then confirm.'
        : 'These blocks are not legal yet: add another blocker or take one back.'
    case 'assign_combat_damage':
      return 'Your attacker has more than one blocker: divide its damage among them, then deal it.'
    case 'priority': {
      const top = view.stack.at(-1)
      if (top) {
        const name = topOfStack ?? 'A spell'
        return top.controller === HUMAN
          ? `Your ${name} is on the stack. Cast something else, or let it resolve.`
          : `The opponent's ${name} is on the stack. Respond, or let it resolve.`
      }
      const yours = view.active_player === HUMAN
      if (yours && view.step === 'precombat_main') {
        return 'Your main phase: play a land or cast spells, then go to combat.'
      }
      if (yours && view.step === 'postcombat_main') {
        return 'Your second main phase: play a land or cast spells, then end the turn.'
      }
      const whose = yours ? 'your turn' : "the opponent's turn"
      return `${PRIORITY_MOMENTS[view.step]} (${whose}). Cast a spell, or pass.`
    }
  }
}

/** What the pass button does, in a word or two. */
export function passLabel(view: PlayerView): string {
  const top = view.stack.at(-1)
  if (top) return top.controller === HUMAN ? 'Resolve' : 'No response'
  if (view.active_player === HUMAN) {
    if (view.step === 'precombat_main') return 'Go to combat'
    if (view.step === 'postcombat_main') return 'End turn'
  }
  return 'Pass'
}

export function confirmAttackersLabel(view: PlayerView): string {
  const count = view.combat?.attackers.length ?? 0
  return count === 0 ? 'No attack' : `Attack with ${count}`
}

export function confirmBlockersLabel(view: PlayerView): string {
  const blocked = view.combat?.attackers.some((attacker) => attacker.blockers.length > 0) ?? false
  return blocked ? 'Confirm blocks' : 'No blocks'
}
