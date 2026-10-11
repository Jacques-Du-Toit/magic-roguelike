import { describe, expect, it } from 'vitest'
import type { Action } from '../engine/types/Action.ts'
import type { Target } from '../engine/types/Target.ts'
import {
  blockOptions,
  cardActions,
  clickAimTarget,
  clickAttacker,
  clickBlockedAttacker,
  clickBlocker,
  clickHandCard,
  clickTarget,
  completedCast,
  isAiming,
  nextAimTargets,
  nextTargets,
  simpleAction,
} from './interaction.ts'

const object = (id: number): Target => ({ type: 'object', object: id })
const player = (id: 'P1' | 'P2'): Target => ({ type: 'player', player: id })
const cast = (card: number, ...targets: Target[]): Action => ({ type: 'cast_spell', card, targets })

const pass: Action = { type: 'pass_priority' }
const land: Action = { type: 'play_land', card: 1 }
const drake = cast(2)
const shocks = [cast(3, player('P2')), cast(3, player('P1')), cast(3, object(50))]
const actions: Action[] = [pass, land, drake, ...shocks]

describe('hand cards', () => {
  it('finds the actions that use a card', () => {
    expect(cardActions(actions, 1)).toEqual([land])
    expect(cardActions(actions, 3)).toEqual(shocks)
    expect(cardActions(actions, 99)).toEqual([])
  })

  it('plays a land or a spell without targets at once', () => {
    expect(clickHandCard(actions, 1)).toEqual({ kind: 'send', action: land })
    expect(clickHandCard(actions, 2)).toEqual({ kind: 'send', action: drake })
  })

  it('does nothing for a card with no legal action', () => {
    expect(clickHandCard(actions, 99)).toEqual({ kind: 'nothing' })
  })

  it('asks for a target when a spell needs one, even if only one is legal', () => {
    expect(clickHandCard(actions, 3)).toEqual({ kind: 'cast', casting: { card: 3, chosen: [] } })
    expect(clickHandCard([cast(4, object(50))], 4)).toEqual({
      kind: 'cast',
      casting: { card: 4, chosen: [] },
    })
  })

  it('discards or bottoms the card when that is the decision', () => {
    const discard: Action = { type: 'discard', card: 2 }
    expect(clickHandCard([discard], 2)).toEqual({ kind: 'send', action: discard })
    const bottom: Action = { type: 'bottom_card', card: 2 }
    expect(clickHandCard([bottom], 2)).toEqual({ kind: 'send', action: bottom })
  })
})

describe('targets', () => {
  const casting = { card: 3, chosen: [] }

  it('offers exactly the targets of the legal casts', () => {
    expect(nextTargets(actions, casting)).toEqual([player('P2'), player('P1'), object(50)])
    expect(nextTargets(actions, { card: 2, chosen: [] })).toEqual([])
  })

  it('casts when a legal target is clicked and ignores anything else', () => {
    expect(clickTarget(actions, casting, object(50))).toEqual({ kind: 'send', action: shocks[2] })
    expect(clickTarget(actions, casting, object(51))).toEqual({ kind: 'nothing' })
  })

  it('walks through several targets one at a time', () => {
    const two = [cast(5, object(1), object(2)), cast(5, object(1), object(3)), cast(5, object(2), object(3))]
    const start = { card: 5, chosen: [] }
    expect(nextTargets(two, start)).toEqual([object(1), object(2)])
    const first = clickTarget(two, start, object(1))
    expect(first).toEqual({ kind: 'cast', casting: { card: 5, chosen: [object(1)] } })
    if (first.kind !== 'cast') throw new Error('expected to keep choosing')
    expect(nextTargets(two, first.casting)).toEqual([object(2), object(3)])
    expect(completedCast(two, first.casting)).toBeUndefined()
    expect(clickTarget(two, first.casting, object(3))).toEqual({ kind: 'send', action: two[1] })
  })
})

describe('combat', () => {
  it('adds or takes back an attacker', () => {
    const declaring: Action[] = [
      { type: 'attack', creature: 10 },
      { type: 'remove_attacker', creature: 11 },
      { type: 'confirm_attackers' },
    ]
    expect(clickAttacker(declaring, 10)).toEqual({ kind: 'send', action: declaring[0] })
    expect(clickAttacker(declaring, 11)).toEqual({ kind: 'send', action: declaring[1] })
    expect(clickAttacker(declaring, 12)).toEqual({ kind: 'nothing' })
  })

  const blocking: Action[] = [
    { type: 'block', blocker: 20, attacker: 10 },
    { type: 'block', blocker: 20, attacker: 11 },
    { type: 'block', blocker: 21, attacker: 11 },
    { type: 'remove_blocker', blocker: 22 },
    { type: 'confirm_blockers' },
  ]

  it('blocks at once when a creature has one possible block', () => {
    expect(blockOptions(blocking, 21)).toEqual([11])
    expect(clickBlocker(blocking, 21)).toEqual({ kind: 'send', action: blocking[2] })
  })

  it('picks the creature up when it could block several attackers', () => {
    expect(clickBlocker(blocking, 20)).toEqual({ kind: 'pick_attacker', blocker: 20 })
    expect(clickBlockedAttacker(blocking, 20, 11)).toEqual({ kind: 'send', action: blocking[1] })
    expect(clickBlockedAttacker(blocking, 20, 12)).toEqual({ kind: 'nothing' })
  })

  it('takes a declared blocker back, and ignores creatures that cannot block', () => {
    expect(clickBlocker(blocking, 22)).toEqual({ kind: 'send', action: blocking[3] })
    expect(clickBlocker(blocking, 23)).toEqual({ kind: 'nothing' })
  })
})

describe('simpleAction', () => {
  it('finds an action by its type', () => {
    expect(simpleAction(actions, 'pass_priority')).toEqual(pass)
    expect(simpleAction(actions, 'keep')).toBeUndefined()
  })
})

describe('aiming a triggered ability', () => {
  const aim = (...targets: Target[]): Action => ({ type: 'choose_targets', targets })
  const one = [aim(object(50)), aim(object(51))]
  const two = [aim(object(50), player('P2')), aim(object(50), player('P1')), aim(object(51), player('P2'))]

  it('is noticed from the legal actions', () => {
    expect(isAiming(one)).toBe(true)
    expect(isAiming(actions)).toBe(false)
  })

  it('offers the targets and sends the choice once it is complete', () => {
    expect(nextAimTargets(one, [])).toEqual([object(50), object(51)])
    expect(clickAimTarget(one, [], object(51))).toEqual({ kind: 'send', action: one[1] })
    expect(clickAimTarget(one, [], object(52))).toEqual({ kind: 'nothing' })
    expect(clickAimTarget(one, [], player('P1'))).toEqual({ kind: 'nothing' })
  })

  it('takes several targets one at a time', () => {
    expect(clickAimTarget(two, [], object(50))).toEqual({ kind: 'aim', chosen: [object(50)] })
    expect(nextAimTargets(two, [object(50)])).toEqual([player('P2'), player('P1')])
    expect(nextAimTargets(two, [object(51)])).toEqual([player('P2')])
    expect(clickAimTarget(two, [object(50)], player('P1'))).toEqual({ kind: 'send', action: two[1] })
  })
})
