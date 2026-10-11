import { describe, expect, it } from 'vitest'
import type { Decision } from '../engine/types/Decision.ts'
import type { PlayerView } from '../engine/types/PlayerView.ts'
import type { StackItemView } from '../engine/types/StackItemView.ts'
import {
  PHASES,
  type PromptInput,
  confirmAttackersLabel,
  confirmBlockersLabel,
  passLabel,
  phaseIndex,
  promptText,
} from './prompt.ts'

function viewWith(change: Partial<PlayerView>): PlayerView {
  return {
    viewer: 'P1',
    turn: 3,
    active_player: 'P1',
    starting_player: 'P1',
    step: 'precombat_main',
    decision: { type: 'priority', player: 'P1' },
    players: [] as unknown as PlayerView['players'],
    battlefield: [],
    stack: [],
    combat: null,
    aiming: null,
    looking_at: null,
    result: null,
    ...change,
  }
}

function prompt(change: Partial<PlayerView>, input: Partial<PromptInput> = {}): string {
  return promptText({
    view: viewWith(change),
    busy: false,
    mode: { kind: 'idle' },
    topOfStack: null,
    canConfirmBlocks: true,
    ...input,
  })
}

const spell = (controller: 'P1' | 'P2'): StackItemView => ({
  id: 9,
  controller,
  kind: { type: 'spell', card: { card: 1, def: 'shock' } },
  targets: [],
})

describe('promptText', () => {
  it('says something for every decision the human can face', () => {
    const decisions: Decision[] = [
      { type: 'mulligan', player: 'P1' },
      { type: 'bottom_cards', player: 'P1', remaining: 2 },
      { type: 'priority', player: 'P1' },
      { type: 'discard', player: 'P1', remaining: 1 },
      { type: 'declare_attackers', player: 'P1' },
      { type: 'declare_blockers', player: 'P1' },
      { type: 'assign_combat_damage', player: 'P1', attacker: 4 },
      { type: 'choose_targets', player: 'P1', source: 7 },
      { type: 'scry', player: 'P1', kept: 0, remaining: 1, surveil: false },
    ]
    const texts = decisions.map((decision) => prompt({ decision }))
    expect(new Set(texts).size).toBe(decisions.length)
    expect(texts[1]).toContain('2 cards')
    expect(texts[3]).toContain('1 card ')
  })

  it('tells a discard at cleanup from one a spell or ability asks for', () => {
    const decision: Decision = { type: 'discard', player: 'P1', remaining: 2 }
    expect(prompt({ decision, step: 'cleanup' })).toBe(
      'You have too many cards: click 2 cards in your hand to discard.',
    )
    expect(prompt({ decision })).toBe('You must discard: click 2 cards in your hand.')
  })

  it('names the ability being aimed and counts the cards of a scry', () => {
    const aim: Decision = { type: 'choose_targets', player: 'P1', source: 7 }
    expect(prompt({ decision: aim }, { abilitySource: "Man-o'-War" })).toBe(
      "Choose a target for Man-o'-War's ability.",
    )
    expect(prompt({ decision: aim })).toBe('Choose a target for the ability.')
    const scry = (surveil: boolean, remaining: number): Decision => ({
      type: 'scry',
      player: 'P1',
      kept: 0,
      remaining,
      surveil,
    })
    expect(prompt({ decision: scry(false, 2) })).toContain('(2 to look at)')
    expect(prompt({ decision: scry(false, 1) })).toContain('put it on the bottom?')
    expect(prompt({ decision: scry(true, 1) })).toBe(
      'Surveil: keep this card on top of your library, or put it into your graveyard?',
    )
  })

  it('says who is being waited for', () => {
    expect(prompt({ decision: { type: 'priority', player: 'P2' } })).toBe(
      'Waiting for the opponent…',
    )
    expect(prompt({}, { busy: true })).toBe('Resolving…')
    expect(prompt({ result: { type: 'draw' }, decision: null })).toBe('The game is over.')
  })

  it('describes the stack and the step when the human has priority', () => {
    expect(prompt({ stack: [spell('P2')] }, { topOfStack: 'Shock' })).toContain(
      "The opponent's Shock is on the stack",
    )
    expect(prompt({ stack: [spell('P1')] }, { topOfStack: 'Shock' })).toContain('Your Shock')
    expect(prompt({})).toContain('Your main phase')
    expect(prompt({ step: 'declare_attackers', active_player: 'P2' })).toBe(
      "Attackers are declared (the opponent's turn). Cast a spell or use an ability, or pass.",
    )
  })

  it('follows what is being pointed at', () => {
    expect(prompt({}, { mode: { kind: 'casting', cardName: 'Murder' } })).toContain(
      'Choose a target for Murder',
    )
    expect(
      prompt(
        { decision: { type: 'declare_blockers', player: 'P1' } },
        { mode: { kind: 'blocking', blockerName: 'Wind Drake' } },
      ),
    ).toContain('Wind Drake should block')
    const activating = (step: 'sacrifice' | 'target') =>
      prompt({}, { mode: { kind: 'activating', sourceName: 'Viscera Seer', step } })
    expect(activating('sacrifice')).toBe('Choose a creature to sacrifice to Viscera Seer, or cancel.')
    expect(activating('target')).toBe("Choose a target for Viscera Seer's ability, or cancel.")
  })

  it('explains why blocks cannot be confirmed', () => {
    expect(
      prompt({ decision: { type: 'declare_blockers', player: 'P1' } }, { canConfirmBlocks: false }),
    ).toContain('not legal yet')
  })
})

describe('button labels', () => {
  it('names what passing does', () => {
    expect(passLabel(viewWith({}))).toBe('Go to combat')
    expect(passLabel(viewWith({ step: 'postcombat_main' }))).toBe('End turn')
    expect(passLabel(viewWith({ step: 'postcombat_main', active_player: 'P2' }))).toBe('Pass')
    expect(passLabel(viewWith({ stack: [spell('P2')] }))).toBe('No response')
    expect(passLabel(viewWith({ stack: [spell('P1')] }))).toBe('Resolve')
  })

  it('counts the declared attackers and notices blocks', () => {
    const combat = (blockers: number[]) => ({
      attackers: [{ attacker: 1, blockers, blocked: false }],
      dealt_first_strike_damage: [],
      assignments: [],
    })
    expect(confirmAttackersLabel(viewWith({}))).toBe('No attack')
    expect(confirmAttackersLabel(viewWith({ combat: combat([]) }))).toBe('Attack with 1')
    expect(confirmBlockersLabel(viewWith({ combat: combat([]) }))).toBe('No blocks')
    expect(confirmBlockersLabel(viewWith({ combat: combat([2]) }))).toBe('Confirm blocks')
  })
})

describe('phaseIndex', () => {
  it('puts every combat step under Combat', () => {
    expect(PHASES[phaseIndex('declare_blockers')]).toBe('Combat')
    expect(PHASES[phaseIndex('upkeep')]).toBe('Upkeep')
    expect(PHASES[phaseIndex('cleanup')]).toBe('End')
  })
})
