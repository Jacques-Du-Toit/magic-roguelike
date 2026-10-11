# MTG Rules Scope

What subset of the Magic: The Gathering comprehensive rules this game implements. Cards may only be added if every rule they need is **In**. When unsure, ask before expanding scope.

Status key: **In** = implement properly · **Simplified** = implement with stated simplification · **Later** = planned, not yet · **Out** = not planned.

**"In" means allowed, not "build it now".** A rule marked In is built in the first milestone whose cards or acceptance criteria need it (see `ROADMAP.md`). For example tokens, the legend rule, X costs, artifacts and emblems are all In, and none of them is needed for M1.

## Game structure

| Rule | Status | Notes |
|---|---|---|
| Two-player duel, 20 starting life | In | Default; per-player in `DuelConfig`. |
| Starting player chosen at random | In | Seeded RNG. Starting player skips first draw. |
| Lands in the library, one land drop per turn | In | As in normal Magic. |
| 7-card opening hand with smoothing | In | Project rule. Deal several candidate hands, keep the one closest to the deck's land ratio. Numbers in `balance.json`. |
| London mulligan | In | |
| Lose by 0 life | In | |
| Fatigue instead of losing on an empty library | In | Project rule. Drawing from an empty library costs 1 life, then 2, then 3... |
| Lose by 10 poison | Later | |
| Drawn game (both players lose at once) | In | |
| Max hand size 7, discard at cleanup | In | |
| Multiplayer, sideboards, best-of-three | Out | |

## Turn structure

| Rule | Status | Notes |
|---|---|---|
| Untap, upkeep, draw | In | Starting player skips first draw. |
| Main phase 1 / 2 | In | |
| Combat: beginning, declare attackers, declare blockers, combat damage, end | In | First-strike damage step when relevant. |
| Combat damage among multiple blockers | Simplified | Current rules (no damage assignment order): attacker divides freely, lethal to every blocker before trampling over. Engine assigns automatically; manual division later. |
| End step, cleanup | In | "Until end of turn" effects end at cleanup. |
| Extra turns / extra combats | Later | |

## Priority and the stack

| Rule | Status | Notes |
|---|---|---|
| Priority passing, stack LIFO | In | Real priority in engine. |
| Arena-style auto-pass | In | Engine reports whether the player has a meaningful action; the UI auto-passes on that. Hold-priority toggle in the UI. |
| Instants, flash | In | |
| Activated abilities | In | Mana, tap, sacrifice, pay life costs. |
| Mana abilities don't use the stack | In | |
| Triggered abilities | In | |
| Ordering simultaneous triggers (APNAP; player orders own) | Simplified | Auto-order: APNAP, then by source `ObjectId`. Manual ordering later. |
| Countering spells | In | |
| Split second, mana burn | Out | |

## Mana

| Rule | Status | Notes |
|---|---|---|
| Coloured, generic, colourless mana | In | |
| Mana pool empties between steps | In | |
| Auto-tapping for payment | In | Done by the engine, deterministically. Manual tapping later. |
| X costs | In | |
| Hybrid mana | Later | |
| Phyrexian mana | Later | |
| Kicker / additional costs | Later | |
| Alternative costs, cost reduction | Simplified | Flat reductions only (needed for relics). |
| Snow, convoke, delve, affinity | Out (for now) | |

## Permanents and card types

| Type | Status | Notes |
|---|---|---|
| Creature | In | |
| Instant, sorcery | In | |
| Land (basic + simple duals) | In | Duals: enter untapped / tapped variants only. |
| Enchantment, Aura | In | Auras: "enchant creature" first. |
| Artifact | In | |
| Equipment | Later | |
| Planeswalker cards | Later | Open question. |
| Vehicles, Sagas, Battles, Classes, Rooms | Out (for now) | |
| Tokens | In | Creature tokens, Treasure, Food (later), Clue (later). |
| Double-faced / split / adventure / MDFC | Out | |

## Keywords

**In (v1):** flying, reach, first strike, double strike, deathtouch, lifelink, trample, haste, vigilance, menace, defender, hexproof, flash, indestructible.

**Later:** ward, prowess, surveil, fight, kicker, flashback, cycling, landfall, cascade.

**Out:** protection (complex), banding, phasing, morph/disguise, mutate, partner, companion.

Scry is an effect primitive rather than a keyword here, and is **In**.

## State-based actions

| Rule | Status |
|---|---|
| 0 life loss | In |
| Creature with toughness ≤ 0 dies | In |
| Lethal damage / deathtouch damage destroys | In |
| Legend rule | In |
| Aura with illegal/no object goes to graveyard | In |
| +1/+1 and −1/−1 counters annihilate | Later |
| Token leaves battlefield → ceases to exist | In |

## Continuous effects

| Rule | Status | Notes |
|---|---|---|
| P/T modification until EOT | In | |
| Anthems (static +N/+N) | In | |
| Counters | In | +1/+1 first; −1/−1 later. |
| Keyword grants | In | |
| "Can't attack / can't block" restrictions | In | Needed for Pacifism. |
| Full layer system (7 layers, timestamps, dependencies) | Simplified | Fixed order: base → set P/T → modify P/T & counters → ability grants. |
| Control-changing effects | Out | |
| Copy effects | Out | |
| Type-changing effects | Out | |

## Replacement and prevention effects

| Rule | Status | Notes |
|---|---|---|
| Simple damage prevention ("prevent the next N") | Later | |
| "Enters tapped" | In | Treated as a property, not a general replacement. |
| "If X would die, exile instead" and general replacements | Out (for now) | |

## Misc

| Rule | Status |
|---|---|
| Targeting, illegal targets on resolution (fizzle) | In |
| Hexproof / shroud | In / Out |
| Emblems (boss emblems) | In |
| Alternate win/lose conditions | Out |
| Hidden information (hand, library order) | In |
| Random discard, look at top N, reveal | In |

## Vision examples that don't fit this scope yet

`GAME_DESIGN.md` gives examples that need rules marked Later or Out above. They are illustrations, not commitments; each needs a scope decision before it is built (M3 onwards).

| Example | Needs |
|---|---|
| Boss emblem: "creatures have ward {1}" | Ward (Later) |
| Relic: "+1/+1 counters, that many plus one instead" | General replacement effects (Out for now) |
| Passive: "your tokens enter with a +1/+1 counter" | Replacement effect (Out for now) |
| Boss relic: "you can't gain life" | Replacement/restriction on life gain |
| RW archetype: equipment | Equipment (Later) |
| Upgrade: "kicker waived" | Kicker (Later) |
| UB archetype: mill | Fatigue makes an empty library far less deadly than in Magic; mill needs its own payoff |
