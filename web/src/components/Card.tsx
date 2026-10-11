// The text-only placeholder card frame, used everywhere a card is shown.

import type { ReactNode } from 'react'
import type { CardDef } from '../engine/types/CardDef.ts'

/** A mana cost such as `{1}{B}{B}` as a row of symbols. */
export function ManaCost({ cost }: { cost: string }) {
  const symbols = [...cost.matchAll(/\{([^}]+)\}/g)].map((match) => match[1])
  if (symbols.length === 0) return null
  return (
    <span className="mana-cost" aria-label={`Mana cost ${cost}`}>
      {symbols.map((symbol, index) => (
        <span key={index} className={`mana mana-${/^[WUBRG]$/.test(symbol) ? symbol : 'generic'}`}>
          {symbol}
        </span>
      ))}
    </span>
  )
}

function typeLine(def: CardDef): string {
  // As a token's type line reads: "Token Creature — Soldier".
  const types = [...(def.token ? ['Token'] : []), ...def.supertypes, ...def.types].join(' ')
  return def.subtypes.length > 0 ? `${types} — ${def.subtypes.join(' ')}` : types
}

function frameClass(def: CardDef): string {
  if (def.colors.length > 1) return 'frame-gold'
  if (def.colors.length === 1) return `frame-${def.colors[0]}`
  return def.types.includes('Land') ? 'frame-land' : 'frame-colorless'
}

export type CardSize = 'small' | 'normal' | 'large'

interface CardFrameProps {
  def: CardDef
  size?: CardSize
  /** Current power and toughness, when they are known to differ from the card. */
  power?: number | null
  toughness?: number | null
  /** Extra markers drawn over the frame, e.g. "Tapped". */
  children?: ReactNode
}

export function CardFrame({ def, size = 'normal', power, toughness, children }: CardFrameProps) {
  const shownPower = power ?? def.power
  const shownToughness = toughness ?? def.toughness
  const modified = shownPower !== def.power || shownToughness !== def.toughness
  return (
    <div className={`card card-${size} ${frameClass(def)}`}>
      <div className="card-title">
        <span className="card-name">{def.name}</span>
        <ManaCost cost={def.mana_cost} />
      </div>
      <div className="card-type">{typeLine(def)}</div>
      <div className="card-text">{def.oracle_text}</div>
      {shownPower !== null && shownToughness !== null && (
        <div className={`card-pt${modified ? ' card-pt-modified' : ''}`}>
          {shownPower}/{shownToughness}
        </div>
      )}
      {children}
    </div>
  )
}

/** A face-down card. */
export function CardBack({ size = 'small' }: { size?: CardSize }) {
  return <div className={`card card-${size} card-back`} aria-label="Face-down card" />
}
