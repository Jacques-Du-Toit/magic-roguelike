import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import type { Action } from '../engine/types/Action.ts'
import type { DamageAssignment } from '../engine/types/DamageAssignment.ts'
import type { Target } from '../engine/types/Target.ts'
import { DamagePanel } from './DamagePanel.tsx'

const lions: Target = { type: 'object', object: 11 }
const spider: Target = { type: 'object', object: 12 }
const opponent: Target = { type: 'player', player: 'P2' }

function render(assignment: DamageAssignment, actions: Action[]): string {
  const names = new Map([
    [11, 'Savannah Lions'],
    [12, 'Giant Spider'],
  ])
  return renderToStaticMarkup(
    <DamagePanel
      assignment={assignment}
      actions={actions}
      attackerName="Colossal Dreadmaw"
      targetName={(target) =>
        target.type === 'player' ? 'the opponent' : (names.get(target.object) ?? '?')
      }
      onAction={() => {}}
    />,
  )
}

/** The `disabled` state of the minus and plus buttons, row by row. */
function buttons(html: string): boolean[][] {
  return [...html.matchAll(/<li[^>]*>(.*?)<\/li>/g)].map((row) =>
    [...row[1].matchAll(/<button[^>]*>/g)].map((button) => button[0].includes('disabled')),
  )
}

describe('DamagePanel', () => {
  it('shows the automatic division, with only the legal buttons enabled', () => {
    const assignment: DamageAssignment = {
      attacker: 5,
      power: 6,
      shares: [
        { target: lions, amount: 1, lethal: 1 },
        { target: spider, amount: 4, lethal: 4 },
        { target: opponent, amount: 1, lethal: 0 },
      ],
      confirmed: false,
    }
    // Everything is assigned: damage can only be taken off.
    const html = render(assignment, [
      { type: 'unassign_damage', target: lions },
      { type: 'unassign_damage', target: spider },
      { type: 'unassign_damage', target: opponent },
      { type: 'confirm_damage' },
    ])
    expect(html).toContain('<strong>Colossal Dreadmaw</strong> deals 6 damage.')
    expect(html).toContain('Savannah Lions')
    expect(html).toContain('The opponent (trample)')
    expect(html).toContain('lethal (needs 4)')
    expect(buttons(html)).toEqual([
      [false, true],
      [false, true],
      [false, true],
    ])
    // Nothing is wrong, so nothing is said about why it can't be dealt.
    expect(html).not.toContain('still to assign')
    expect(html).not.toContain('Every blocker needs lethal')
  })

  it('says what is missing when the division cannot be confirmed', () => {
    const short: DamageAssignment = {
      attacker: 5,
      power: 6,
      shares: [
        { target: lions, amount: 1, lethal: 1 },
        { target: spider, amount: 3, lethal: 4 },
        { target: opponent, amount: 1, lethal: 0 },
      ],
      confirmed: false,
    }
    const html = render(short, [
      { type: 'assign_damage', target: lions },
      { type: 'assign_damage', target: spider },
      { type: 'unassign_damage', target: lions },
      { type: 'unassign_damage', target: spider },
      { type: 'unassign_damage', target: opponent },
    ])
    expect(html).toContain('1 damage still to assign.')
    expect(html).toContain('not lethal (needs 4)')
    // No more may go to the player while the Spider is short of lethal.
    expect(buttons(html)).toEqual([
      [false, false],
      [false, false],
      [false, true],
    ])

    const overThePlayer: DamageAssignment = {
      ...short,
      shares: [
        { target: lions, amount: 2, lethal: 1 },
        { target: spider, amount: 3, lethal: 4 },
        { target: opponent, amount: 1, lethal: 0 },
      ],
    }
    expect(render(overThePlayer, [{ type: 'unassign_damage', target: opponent }])).toContain(
      'Every blocker needs lethal damage before any goes to the player.',
    )
  })
})
