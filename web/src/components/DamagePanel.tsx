// Dividing an attacker's combat damage among its blockers. The engine
// fills in the automatic division; the buttons here are its legal actions.

import { HUMAN } from '../engine/index.ts'
import type { Action } from '../engine/types/Action.ts'
import type { DamageAssignment } from '../engine/types/DamageAssignment.ts'
import type { Target } from '../engine/types/Target.ts'
import { targetKey } from '../duel/present.ts'

interface DamagePanelProps {
  assignment: DamageAssignment
  actions: Action[]
  attackerName: string
  targetName: (target: Target) => string
  onAction: (action: Action) => void
}

export function DamagePanel({
  assignment,
  actions,
  attackerName,
  targetName,
  onAction,
}: DamagePanelProps) {
  const find = (type: 'assign_damage' | 'unassign_damage', target: Target) =>
    actions.find(
      (action) => action.type === type && targetKey(action.target) === targetKey(target),
    )
  const confirm = actions.find((action) => action.type === 'confirm_damage')
  const assigned = assignment.shares.reduce((sum, share) => sum + share.amount, 0)
  const unassigned = assignment.power - assigned
  const why =
    unassigned > 0
      ? `${unassigned} damage still to assign.`
      : 'Every blocker needs lethal damage before any goes to the player.'

  return (
    <div className="damage-panel" role="group" aria-label="Divide combat damage" data-testid="damage-panel">
      <p>
        <strong>{attackerName}</strong> deals {assignment.power} damage. Divide it as you like; this
        is what the game would do.
      </p>
      <ul>
        {assignment.shares.map((share) => {
          const less = find('unassign_damage', share.target)
          const more = find('assign_damage', share.target)
          const name = targetName(share.target)
          const isPlayer = share.target.type === 'player'
          return (
            <li key={targetKey(share.target)} data-testid="damage-share">
              <span className="damage-name">
                {isPlayer && share.target.type === 'player'
                  ? share.target.player === HUMAN
                    ? 'You'
                    : 'The opponent (trample)'
                  : name}
              </span>
              <button
                type="button"
                disabled={!less}
                onClick={() => less && onAction(less)}
                aria-label={`One less damage to ${name}`}
              >
                −
              </button>
              <span className="damage-amount" data-testid="damage-amount">
                {share.amount}
              </span>
              <button
                type="button"
                disabled={!more}
                onClick={() => more && onAction(more)}
                aria-label={`One more damage to ${name}`}
              >
                +
              </button>
              <span className="muted">
                {isPlayer
                  ? 'only after lethal damage to every blocker'
                  : share.amount >= share.lethal
                    ? `lethal (needs ${share.lethal})`
                    : `not lethal (needs ${share.lethal})`}
              </span>
            </li>
          )
        })}
      </ul>
      {!confirm && <p className="muted">{why}</p>}
    </div>
  )
}
