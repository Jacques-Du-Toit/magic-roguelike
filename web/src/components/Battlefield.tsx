// The battlefield: each player's permanents, and the creatures in combat
// lined up against each other in the middle.

import { AI, HUMAN } from '../engine/index.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { PermanentView } from '../engine/types/PermanentView.ts'
import type { PlayerId } from '../engine/types/PlayerId.ts'
import type { PlayerView } from '../engine/types/PlayerView.ts'
import type { Effects } from '../duel/present.ts'
import { CardFrame } from './Card.tsx'
import { usePreview } from './usePreview.ts'

/** Why a permanent is lit up, if it is. */
export type Highlight = 'target' | 'action' | 'selected' | null

interface Shared {
  cards: ReadonlyMap<string, CardDef>
  effects: Effects
  highlight: (object: number) => Highlight
  /** Whether something on the stack targets the object. */
  targeted: (object: number) => boolean
  onPermanent: (permanent: PermanentView) => void
  /** Auras by the object they are attached to. */
  auras: ReadonlyMap<number, PermanentView[]>
}

function Floats({ effects, object }: { effects: Effects; object: number }) {
  const floats = effects.floats.filter((float) => float.key === `o:${object}`)
  return (
    <>
      {floats.map((float, index) => (
        <span key={index} className={`float float-${float.tone}`}>
          {float.text}
        </span>
      ))}
    </>
  )
}

function AuraChip({ aura, shared }: { aura: PermanentView; shared: Shared }) {
  const def = shared.cards.get(aura.def)
  const preview = usePreview(def)
  const highlight = shared.highlight(aura.object)
  return (
    <button
      type="button"
      className={`aura-chip${highlight ? ` lit-${highlight}` : ''}`}
      onClick={() => shared.onPermanent(aura)}
      {...preview}
    >
      {def?.name ?? aura.def}
    </button>
  )
}

function Permanent({
  permanent,
  shared,
  badge,
}: {
  permanent: PermanentView
  shared: Shared
  /** Its part in combat, in words. */
  badge?: string
}) {
  const def = shared.cards.get(permanent.def)
  const preview = usePreview(def)
  if (!def) return null
  const highlight = shared.highlight(permanent.object)
  const isCreature = permanent.power !== null
  const leaving = shared.effects.leaving.includes(permanent.object)
  const classes = [
    'permanent',
    isCreature ? 'permanent-creature' : 'permanent-compact',
    permanent.tapped ? 'tapped' : '',
    leaving ? 'leaving' : '',
    highlight ? `lit-${highlight}` : '',
  ]
  const auras = shared.auras.get(permanent.object) ?? []
  // Keywords it has from something other than its own text.
  const granted = permanent.keywords.filter((keyword) => !def.keywords.includes(keyword))
  return (
    <div className="permanent-slot" data-object={permanent.object} data-card={def.id}>
      <button
        type="button"
        className={classes.filter(Boolean).join(' ')}
        onClick={() => shared.onPermanent(permanent)}
        aria-label={`${def.name}${permanent.tapped ? ', tapped' : ''}${badge ? `, ${badge}` : ''}`}
        {...preview}
      >
        {isCreature ? (
          <CardFrame def={def} size="small" power={permanent.power} toughness={permanent.toughness}>
            <span className="badges">
              {permanent.commander && (
                <span className="badge badge-commander" data-testid="commander-badge">
                  Commander
                </span>
              )}
              {badge && <span className="badge badge-combat">{badge}</span>}
              {shared.targeted(permanent.object) && (
                <span className="badge badge-targeted">Targeted</span>
              )}
              {permanent.counters > 0 && (
                <span className="badge badge-granted" data-testid="counters">
                  +1/+1 ×{permanent.counters}
                </span>
              )}
              {granted.map((keyword) => (
                <span key={keyword} className="badge badge-granted">
                  {keyword.replace('_', ' ')}
                </span>
              ))}
              {permanent.tapped && <span className="badge">Tapped</span>}
              {permanent.summoning_sick && <span className="badge">Just arrived</span>}
              {permanent.damage > 0 && (
                <span className="badge badge-damage">{permanent.damage} damage</span>
              )}
            </span>
          </CardFrame>
        ) : (
          <span className="compact-name">
            {def.name}
            {permanent.tapped && <span className="badge">Tapped</span>}
          </span>
        )}
        <Floats effects={shared.effects} object={permanent.object} />
      </button>
      {auras.map((aura) => (
        <AuraChip key={aura.card} aura={aura} shared={shared} />
      ))}
    </div>
  )
}

function Row({
  label,
  permanents,
  shared,
}: {
  label: string
  permanents: PermanentView[]
  shared: Shared
}) {
  return (
    <div className="bf-row" aria-label={label}>
      {permanents.map((permanent) => (
        <Permanent key={permanent.card} permanent={permanent} shared={shared} />
      ))}
    </div>
  )
}

interface BattlefieldProps {
  view: PlayerView
  cards: ReadonlyMap<string, CardDef>
  effects: Effects
  highlight: (object: number) => Highlight
  targeted: (object: number) => boolean
  onPermanent: (permanent: PermanentView) => void
}

export function Battlefield({
  view,
  cards,
  effects,
  highlight,
  targeted,
  onPermanent,
}: BattlefieldProps) {
  const byObject = new Map(view.battlefield.map((permanent) => [permanent.object, permanent]))
  const auras = new Map<number, PermanentView[]>()
  for (const permanent of view.battlefield) {
    if (permanent.attached_to !== null && byObject.has(permanent.attached_to)) {
      auras.set(permanent.attached_to, [...(auras.get(permanent.attached_to) ?? []), permanent])
    }
  }
  const shared: Shared = { cards, effects, highlight, targeted, onPermanent, auras }

  // An attacker that has left the battlefield takes its fight with it; its
  // blockers go back to their row.
  const attackers = (view.combat?.attackers ?? []).filter((entry) => byObject.has(entry.attacker))
  const inCombat = new Set(attackers.flatMap((entry) => [entry.attacker, ...entry.blockers]))
  const attached = (permanent: PermanentView) =>
    permanent.attached_to !== null && byObject.has(permanent.attached_to)
  const side = (player: PlayerId, creatures: boolean) =>
    view.battlefield.filter(
      (permanent) =>
        permanent.controller === player &&
        (permanent.power !== null) === creatures &&
        !inCombat.has(permanent.object) &&
        !attached(permanent),
    )
  const humanAttacks = view.active_player === HUMAN

  return (
    <div className="battlefield">
      <Row label="Opponent's lands" permanents={side(AI, false)} shared={shared} />
      <Row label="Opponent's creatures" permanents={side(AI, true)} shared={shared} />
      {attackers.length > 0 && (
        <div
          className={`combat-lane${humanAttacks ? ' combat-lane-up' : ''}`}
          aria-label="Combat"
          data-testid="combat-lane"
        >
          {attackers.map((entry) => {
            const attacker = byObject.get(entry.attacker)
            if (!attacker) return null
            return (
              <div key={attacker.card} className="combat-pair">
                <Permanent permanent={attacker} shared={shared} badge="Attacking" />
                <div className="combat-blockers">
                  {entry.blockers.map((id) => {
                    const blocker = byObject.get(id)
                    return (
                      blocker && (
                        <Permanent
                          key={blocker.card}
                          permanent={blocker}
                          shared={shared}
                          badge="Blocking"
                        />
                      )
                    )
                  })}
                </div>
              </div>
            )
          })}
        </div>
      )}
      <Row label="Your creatures" permanents={side(HUMAN, true)} shared={shared} />
      <Row label="Your lands" permanents={side(HUMAN, false)} shared={shared} />
    </div>
  )
}
