// Turning a batch of engine events into something to show: log lines, and
// a short sequence of beats to animate before the new view replaces the old
// one. Display only: nothing here decides anything about the game.

import type { CardDef } from '../engine/types/CardDef.ts'
import type { GameEvent } from '../engine/types/GameEvent.ts'
import type { PlayerId } from '../engine/types/PlayerId.ts'
import type { Target } from '../engine/types/Target.ts'

/** A number floating over a permanent or a player, e.g. damage. */
export interface FloatEffect {
  /** `targetKey` of what it floats over. */
  key: string
  text: string
  tone: 'harm' | 'help'
}

/** What is being animated over the view currently shown. */
export interface Effects {
  floats: FloatEffect[]
  /** Permanents (by object id) on their way out. */
  leaving: number[]
}

export const NO_EFFECTS: Effects = { floats: [], leaving: [] }

/** One step of animation, shown over the old view for `ms`. */
export interface Beat {
  effects: Effects
  ms: number
}

export interface LogEntry {
  text: string
  kind: 'turn' | 'event' | 'result'
}

export interface Presentation {
  /** Played in order before the view changes. */
  beats: Beat[]
  log: LogEntry[]
  /** How long to hold the new view before anything else happens. */
  holdMs: number
}

/** Durations in milliseconds. These are UI pacing, not game balance. */
export const TIMING = {
  impactBeat: 650,
  leavingBeat: 450,
  hold: {
    turnBegan: 500,
    cardDrawn: 300,
    landPlayed: 500,
    spellCast: 900,
    abilityTriggered: 800,
    permanentEntered: 350,
    spellEnded: 350,
    attackersDeclared: 900,
    blockersDeclared: 900,
    own: 150,
  },
} as const

export function targetKey(target: Target): string {
  return target.type === 'player' ? `p:${target.player}` : `o:${target.object}`
}

/** What the presentation needs to know to name things. */
export interface NameContext {
  viewer: PlayerId
  /** Name of the card behind an object id, if it has been seen. */
  objectName: (object: number) => string | undefined
  cards: ReadonlyMap<string, CardDef>
}

function signed(amount: number): string {
  return amount > 0 ? `+${amount}` : `${amount}`
}

export function describeEvent(event: GameEvent, context: NameContext): LogEntry | null {
  const { viewer, cards } = context
  const you = (player: PlayerId) => player === viewer
  const who = (player: PlayerId) => (you(player) ? 'You' : 'Opponent')
  // "You draw" but "Opponent draws".
  const verb = (player: PlayerId, base: string) => (you(player) ? base : `${base}s`)
  const whose = (player: PlayerId) => (you(player) ? 'Your' : "Opponent's")
  const object = (id: number) => context.objectName(id) ?? 'a card'
  const face = (card: { face: { def: string } | null }) =>
    card.face ? (cards.get(card.face.def)?.name ?? card.face.def) : 'a card'
  const target = (t: Target) =>
    t.type === 'player' ? (you(t.player) ? 'you' : 'the opponent') : object(t.object)
  const line = (text: string): LogEntry => ({ text, kind: 'event' })

  switch (event.type) {
    case 'mulligan_taken':
      return line(`${who(event.player)} ${verb(event.player, 'mulligan')}`)
    case 'hand_kept':
      return line(`${who(event.player)} ${verb(event.player, 'keep')}`)
    case 'card_drawn':
      return line(`${who(event.player)} ${verb(event.player, 'draw')} ${face(event.card)}`)
    case 'card_put_on_bottom':
      return line(
        `${who(event.player)} ${verb(event.player, 'put')} ${face(event.card)} on the bottom`,
      )
    case 'turn_began':
      return {
        text: `Turn ${event.turn}: ${you(event.player) ? 'your turn' : "opponent's turn"}`,
        kind: 'turn',
      }
    case 'land_played':
      return line(`${who(event.player)} ${verb(event.player, 'play')} ${face(event.card)}`)
    case 'card_discarded':
      return line(`${who(event.player)} ${verb(event.player, 'discard')} ${face(event.card)}`)
    case 'fatigue':
      return line(`${who(event.player)} ${verb(event.player, 'draw')} from an empty library`)
    case 'life_changed': {
      const change =
        event.change < 0
          ? `${verb(event.player, 'lose')} ${-event.change}`
          : `${verb(event.player, 'gain')} ${event.change}`
      return line(`${who(event.player)} ${change} life (now ${event.life})`)
    }
    case 'game_over': {
      const result = event.result
      if (result.type === 'draw') return { text: 'The game is a draw', kind: 'result' }
      return {
        text: you(result.player) ? 'You win the game' : 'Opponent wins the game',
        kind: 'result',
      }
    }
    case 'spell_cast': {
      const at = event.targets.length > 0 ? ` targeting ${event.targets.map(target).join(', ')}` : ''
      return line(`${who(event.player)} ${verb(event.player, 'cast')} ${face(event.card)}${at}`)
    }
    case 'permanent_entered':
      return line(`${face(event.card)} enters the battlefield`)
    case 'spell_resolved':
      return line(`${face(event.card)} resolves`)
    case 'spell_fizzled':
      return line(`${face(event.card)} has no legal target and does nothing`)
    case 'spell_countered':
      return line(`${face(event.card)} is countered`)
    case 'ability_triggered':
      return line(`${object(event.source)}'s ability triggers`)
    case 'ability_activated': {
      const at = event.targets.length > 0 ? ` targeting ${event.targets.map(target).join(', ')}` : ''
      return line(
        `${who(event.player)} ${verb(event.player, 'use')} ${object(event.source)}'s ability${at}`,
      )
    }
    case 'permanent_sacrificed':
      return line(`${who(event.player)} ${verb(event.player, 'sacrifice')} ${object(event.permanent)}`)
    case 'power_toughness_changed':
      return line(
        `${object(event.permanent)} gets ${signed(event.power)}/${signed(event.toughness)} until end of turn`,
      )
    case 'aura_attached':
      return line(`${object(event.aura)} is attached to ${object(event.enchanted)}`)
    case 'permanent_put_into_graveyard':
      return line(`${whose(event.player)} ${face(event.card)} is put into the graveyard`)
    case 'attackers_declared':
      return line(
        `${who(event.player)} ${verb(event.player, 'attack')} with ${event.attackers.map(object).join(', ')}`,
      )
    case 'blockers_declared':
      if (event.blocks.length === 0) return line(`${who(event.player)} ${verb(event.player, 'declare')} no blocks`)
      return line(
        `${who(event.player)} ${verb(event.player, 'block')} ` +
          event.blocks
            .map((block) => `${object(block.attacker)} with ${block.blockers.map(object).join(' and ')}`)
            .join('; '),
      )
    case 'damage_dealt':
      return line(`${object(event.source)} deals ${event.amount} damage to ${target(event.target)}`)
    case 'creature_died':
      return line(`${whose(event.player)} ${face(event.card)} dies`)
    case 'counters_added': {
      const counters = event.amount === 1 ? 'a +1/+1 counter' : `${event.amount} +1/+1 counters`
      return line(`${object(event.permanent)} gets ${counters}`)
    }
    case 'token_created':
      return line(`${who(event.player)} ${verb(event.player, 'create')} a ${face(event.card)} token`)
    case 'keyword_granted':
      return line(
        `${object(event.permanent)} gains ${event.keyword.replaceAll('_', ' ')} until end of turn`,
      )
    case 'permanent_returned_to_hand':
      return line(`${whose(event.player)} ${face(event.card)} returns to its owner's hand`)
    case 'card_milled':
      return line(
        `${whose(event.player)} ${face(event.card)} goes from the library to the graveyard`,
      )
    case 'card_returned_to_hand':
      return line(
        `${who(event.player)} ${verb(event.player, 'return')} ${face(event.card)} from the graveyard to hand`,
      )
    case 'card_kept_on_top':
      return line(`${who(event.player)} ${verb(event.player, 'keep')} a card on top`)
    case 'step_began':
    case 'permanents_untapped':
    case 'permanent_tapped':
    case 'ability_resolved':
      return null
  }
}

type BeatKind = 'impact' | 'leaving'

function beatKind(event: GameEvent): BeatKind | null {
  switch (event.type) {
    case 'damage_dealt':
    case 'life_changed':
    case 'power_toughness_changed':
    case 'counters_added':
      return 'impact'
    case 'creature_died':
    case 'permanent_put_into_graveyard':
    case 'permanent_returned_to_hand':
      return 'leaving'
    default:
      return null
  }
}

function holdFor(event: GameEvent, viewer: PlayerId): number {
  const hold = TIMING.hold
  // The human knows what they just did; the opponent's moves need time to
  // be read.
  const theirs = (player: PlayerId, ms: number) => (player === viewer ? hold.own : ms)
  switch (event.type) {
    case 'turn_began':
      return hold.turnBegan
    case 'card_drawn':
      return hold.cardDrawn
    case 'land_played':
      return theirs(event.player, hold.landPlayed)
    case 'spell_cast':
      return theirs(event.player, hold.spellCast)
    case 'ability_triggered':
      return hold.abilityTriggered
    case 'ability_activated':
      return theirs(event.player, hold.abilityTriggered)
    case 'permanent_entered':
    case 'token_created':
      return hold.permanentEntered
    case 'spell_resolved':
    case 'spell_fizzled':
    case 'spell_countered':
      return hold.spellEnded
    case 'attackers_declared':
      return theirs(event.player, hold.attackersDeclared)
    case 'blockers_declared':
      return event.blocks.length > 0 ? theirs(event.player, hold.blockersDeclared) : 0
    default:
      return 0
  }
}

/**
 * Plans how to show one batch of events. Damage, life changes and deaths
 * are animated over the old view, in order, because what they happen to may
 * be gone from the new one; everything else simply appears with the new
 * view, which is then held for long enough to be read.
 */
export function planPresentation(events: GameEvent[], context: NameContext): Presentation {
  const beats: Beat[] = []
  const log: LogEntry[] = []
  let holdMs = 0
  let current = null as { kind: BeatKind; effects: Effects } | null

  for (const event of events) {
    const entry = describeEvent(event, context)
    if (entry) log.push(entry)
    holdMs = Math.max(holdMs, holdFor(event, context.viewer))

    const kind = beatKind(event)
    if (kind === null) continue
    if (current?.kind !== kind) {
      current = { kind, effects: { floats: [], leaving: [] } }
      beats.push({
        effects: current.effects,
        ms: kind === 'impact' ? TIMING.impactBeat : TIMING.leavingBeat,
      })
    }
    const effects = current.effects
    switch (event.type) {
      case 'damage_dealt':
        // Damage to a player shows as their life change instead.
        if (event.target.type === 'object') {
          effects.floats.push({
            key: targetKey(event.target),
            text: `-${event.amount}`,
            tone: 'harm',
          })
        }
        break
      case 'life_changed':
        effects.floats.push({
          key: targetKey({ type: 'player', player: event.player }),
          text: signed(event.change),
          tone: event.change < 0 ? 'harm' : 'help',
        })
        break
      case 'power_toughness_changed':
        effects.floats.push({
          key: targetKey({ type: 'object', object: event.permanent }),
          text: `${signed(event.power)}/${signed(event.toughness)}`,
          tone: event.power < 0 || event.toughness < 0 ? 'harm' : 'help',
        })
        break
      case 'counters_added':
        effects.floats.push({
          key: targetKey({ type: 'object', object: event.permanent }),
          text: `+${event.amount}/+${event.amount}`,
          tone: 'help',
        })
        break
      case 'creature_died':
      case 'permanent_put_into_graveyard':
      case 'permanent_returned_to_hand':
        effects.leaving.push(event.permanent)
        break
      default:
        break
    }
  }
  return { beats, log, holdMs }
}
