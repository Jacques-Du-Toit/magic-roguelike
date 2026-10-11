// Mapping clicks onto the engine's legal actions. Nothing here knows any
// rules: every function only searches the list the engine returned.

import type { Action } from '../engine/types/Action.ts'
import type { Target } from '../engine/types/Target.ts'
import { targetKey } from './present.ts'

/** A card being cast, with the targets picked for it so far. */
export interface Casting {
  card: number
  chosen: Target[]
}

type CastSpell = Extract<Action, { type: 'cast_spell' }>

/** The legal actions that use the card `card` from hand. */
export function cardActions(actions: Action[], card: number): Action[] {
  return actions.filter(
    (action) =>
      (action.type === 'play_land' ||
        action.type === 'cast_spell' ||
        action.type === 'discard' ||
        action.type === 'bottom_card') &&
      action.card === card,
  )
}

function castsMatching(actions: Action[], casting: Casting): CastSpell[] {
  return actions.filter(
    (action): action is CastSpell =>
      action.type === 'cast_spell' &&
      action.card === casting.card &&
      casting.chosen.every((target, index) => {
        const other = action.targets.at(index)
        return other !== undefined && targetKey(other) === targetKey(target)
      }),
  )
}

/** The targets that could be picked next for the card being cast. */
export function nextTargets(actions: Action[], casting: Casting): Target[] {
  const seen = new Map<string, Target>()
  for (const action of castsMatching(actions, casting)) {
    const next = action.targets.at(casting.chosen.length)
    if (next) seen.set(targetKey(next), next)
  }
  return [...seen.values()]
}

/** The action that casts the card with exactly the targets picked so far. */
export function completedCast(actions: Action[], casting: Casting): Action | undefined {
  return castsMatching(actions, casting).find(
    (action) => action.targets.length === casting.chosen.length,
  )
}

export type Click =
  | { kind: 'send'; action: Action }
  /** Start (or continue) choosing targets. */
  | { kind: 'cast'; casting: Casting }
  | { kind: 'nothing' }

const NOTHING: Click = { kind: 'nothing' }

/** What clicking a card in hand does. */
export function clickHandCard(actions: Action[], card: number): Click {
  const options = cardActions(actions, card)
  if (options.length === 0) return NOTHING
  if (options.length === 1) {
    const [only] = options
    if (only.type !== 'cast_spell' || only.targets.length === 0) {
      return { kind: 'send', action: only }
    }
  }
  return { kind: 'cast', casting: { card, chosen: [] } }
}

/** What clicking a possible target does while a card is being cast. */
export function clickTarget(actions: Action[], casting: Casting, target: Target): Click {
  const legal = nextTargets(actions, casting).some(
    (option) => targetKey(option) === targetKey(target),
  )
  if (!legal) return NOTHING
  const next: Casting = { card: casting.card, chosen: [...casting.chosen, target] }
  // Done once no legal cast wants a further target.
  const done = completedCast(actions, next)
  if (done && nextTargets(actions, next).length === 0) return { kind: 'send', action: done }
  return { kind: 'cast', casting: next }
}

type ChooseTargets = Extract<Action, { type: 'choose_targets' }>

function aimsMatching(actions: Action[], chosen: Target[]): ChooseTargets[] {
  return actions.filter(
    (action): action is ChooseTargets =>
      action.type === 'choose_targets' &&
      chosen.every((target, index) => {
        const other = action.targets.at(index)
        return other !== undefined && targetKey(other) === targetKey(target)
      }),
  )
}

/** Whether the game is asking for the targets of a triggered ability. */
export function isAiming(actions: Action[]): boolean {
  return actions.some((action) => action.type === 'choose_targets')
}

/** The targets that could be picked next for the ability being aimed. */
export function nextAimTargets(actions: Action[], chosen: Target[]): Target[] {
  const seen = new Map<string, Target>()
  for (const action of aimsMatching(actions, chosen)) {
    const next = action.targets.at(chosen.length)
    if (next) seen.set(targetKey(next), next)
  }
  return [...seen.values()]
}

export type AimClick =
  | { kind: 'send'; action: Action }
  /** More targets are still to be picked. */
  | { kind: 'aim'; chosen: Target[] }
  | { kind: 'nothing' }

/** What clicking a possible target does while an ability is being aimed. */
export function clickAimTarget(actions: Action[], chosen: Target[], target: Target): AimClick {
  const legal = nextAimTargets(actions, chosen).some(
    (option) => targetKey(option) === targetKey(target),
  )
  if (!legal) return { kind: 'nothing' }
  const next = [...chosen, target]
  const done = aimsMatching(actions, next).find((action) => action.targets.length === next.length)
  return done ? { kind: 'send', action: done } : { kind: 'aim', chosen: next }
}

type Activate = Extract<Action, { type: 'activate_ability' }>

/** A permanent whose ability is being activated, with what has been picked for it so far. */
export interface Activating {
  source: number
  chosen: Target[]
}

/**
 * What activating needs picked, in the order it is clicked: the creature to
 * sacrifice, if the cost asks for one, and then the targets.
 */
function activationChoices(action: Activate): Target[] {
  const sacrifice: Target[] =
    action.sacrifice === null ? [] : [{ type: 'object', object: action.sacrifice }]
  return [...sacrifice, ...action.targets]
}

function activationsMatching(actions: Action[], activating: Activating): Activate[] {
  return actions.filter(
    (action): action is Activate =>
      action.type === 'activate_ability' &&
      action.source === activating.source &&
      activating.chosen.every((target, index) => {
        const other = activationChoices(action).at(index)
        return other !== undefined && targetKey(other) === targetKey(target)
      }),
  )
}

/** Whether the permanent has an ability that could be activated now. */
export function canActivate(actions: Action[], source: number): boolean {
  return actions.some((action) => action.type === 'activate_ability' && action.source === source)
}

/** What could be picked next for the ability being activated. */
export function nextActivationChoices(actions: Action[], activating: Activating): Target[] {
  const seen = new Map<string, Target>()
  for (const action of activationsMatching(actions, activating)) {
    const next = activationChoices(action).at(activating.chosen.length)
    if (next) seen.set(targetKey(next), next)
  }
  return [...seen.values()]
}

/** Whether the next pick is the creature to sacrifice or a target. */
export function activationStep(actions: Action[], activating: Activating): 'sacrifice' | 'target' {
  const [first] = activationsMatching(actions, activating)
  return first && first.sacrifice !== null && activating.chosen.length === 0
    ? 'sacrifice'
    : 'target'
}

export type ActivateClick =
  | { kind: 'send'; action: Action }
  /** Something is still to be picked. */
  | { kind: 'activate'; activating: Activating }
  | { kind: 'nothing' }

function activationClick(actions: Action[], activating: Activating): ActivateClick {
  const matching = activationsMatching(actions, activating)
  if (matching.length === 0) return { kind: 'nothing' }
  const done = matching.find(
    (action) => activationChoices(action).length === activating.chosen.length,
  )
  return done ? { kind: 'send', action: done } : { kind: 'activate', activating }
}

/** Clicking one of your permanents when it has an ability to activate. */
export function clickAbilitySource(actions: Action[], source: number): ActivateClick {
  return activationClick(actions, { source, chosen: [] })
}

/** Clicking something lit while an ability is being activated. */
export function clickActivationChoice(
  actions: Action[],
  activating: Activating,
  target: Target,
): ActivateClick {
  const legal = nextActivationChoices(actions, activating).some(
    (option) => targetKey(option) === targetKey(target),
  )
  if (!legal) return { kind: 'nothing' }
  return activationClick(actions, {
    source: activating.source,
    chosen: [...activating.chosen, target],
  })
}

/** Clicking one of your creatures while declaring attackers. */
export function clickAttacker(actions: Action[], creature: number): Click {
  const action = actions.find(
    (option) =>
      (option.type === 'attack' || option.type === 'remove_attacker') &&
      option.creature === creature,
  )
  return action ? { kind: 'send', action } : NOTHING
}

/** The attackers `blocker` could be declared as blocking. */
export function blockOptions(actions: Action[], blocker: number): number[] {
  return actions.flatMap((action) =>
    action.type === 'block' && action.blocker === blocker ? [action.attacker] : [],
  )
}

export type BlockClick = Click | { kind: 'pick_attacker'; blocker: number }

/**
 * Clicking one of your creatures while declaring blockers: a declared
 * blocker is taken back, a creature with one possible block makes it, and
 * otherwise the creature is picked up to be pointed at an attacker.
 */
export function clickBlocker(actions: Action[], blocker: number): BlockClick {
  const remove = actions.find(
    (action) => action.type === 'remove_blocker' && action.blocker === blocker,
  )
  if (remove) return { kind: 'send', action: remove }
  const options = blockOptions(actions, blocker)
  if (options.length === 0) return NOTHING
  if (options.length === 1) {
    return { kind: 'send', action: { type: 'block', blocker, attacker: options[0] } }
  }
  return { kind: 'pick_attacker', blocker }
}

/** Clicking an attacker with a blocker picked up. */
export function clickBlockedAttacker(actions: Action[], blocker: number, attacker: number): Click {
  const action = actions.find(
    (option) =>
      option.type === 'block' && option.blocker === blocker && option.attacker === attacker,
  )
  return action ? { kind: 'send', action } : NOTHING
}

/** The first legal action of a kind that needs no further choice. */
export function simpleAction<T extends Action['type']>(
  actions: Action[],
  type: T,
): Extract<Action, { type: T }> | undefined {
  return actions.find((action): action is Extract<Action, { type: T }> => action.type === type)
}
