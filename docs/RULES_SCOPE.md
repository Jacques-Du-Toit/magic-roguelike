# MTG Rules Scope

What subset of the Magic: The Gathering comprehensive rules this game implements. Cards may only be added if every rule they need is **In**. When unsure, ask before expanding scope.

Status key: **In** = implement properly · **Simplified** = implement with stated simplification · **Later** = planned, not yet · **Out** = not planned.

**"In" means allowed, not "build it now".** A rule marked In is built in the first milestone whose cards or acceptance criteria need it (see `ROADMAP.md`). For example tokens, the legend rule, X costs, artifacts and emblems are all In, and none of them is needed for M1.

## Game structure

| Rule | Status | Notes |
|---|---|---|
| Two-player duel, 20 starting life | In | Default; per-player in `DuelConfig`. |
| Starting player | In | Project rule: in a run's duels the player always starts (`DECISIONS.md` #74). Elsewhere (practice duels, the sim's deck pairings) it is the seeded coin flip. Starting player skips first draw. |
| Lands in the library, one land drop per turn | In | As in normal Magic. |
| 7-card opening hand with smoothing | In | Project rule, for both players and for every mulligan hand. Deal hands until one is right (its land count fits the deck's land ratio, and every colour among its spells has a land in the hand that makes it; a dual land is a source of each of its colours), up to 5, then keep the one that misses least. The cap is in `balance.json`. |
| London mulligan | In | |
| Lose by 0 life | In | |
| Fatigue instead of losing on an empty library | In | Project rule. Drawing from an empty library costs 1 life, then 2, then 3... |
| Lose by 10 poison | Later | |
| Drawn game (both players lose at once) | In | |
| Max hand size 7, discard at cleanup | In | |
| Commander: a legendary creature cast from the command zone | Simplified | A prototype (`DECISIONS.md` #45). Tax of {2} per earlier cast from the command zone; it returns there automatically when it dies, is countered or is discarded; no commander damage, no choice of leaving it in the graveyard. |
| Multiplayer, sideboards, best-of-three | Out | |

## Turn structure

| Rule | Status | Notes |
|---|---|---|
| Untap, upkeep, draw | In | Starting player skips first draw. |
| Main phase 1 / 2 | In | |
| Combat: beginning, declare attackers, declare blockers, combat damage, end | In | First-strike damage step when relevant. |
| Combat damage among multiple blockers | In | Current rules (no damage assignment order): attacker divides freely, lethal to every blocker before trampling over. The attacking player is asked when an attacker has two or more blockers, starting from the automatic division; with one blocker the engine assigns automatically (lethal to the blocker, the rest tramples over). |
| End step, cleanup | In | "Until end of turn" effects end at cleanup. |
| Priority in every step | Simplified | Nobody gets priority in the untap or cleanup steps, or in the declare-attackers step when no creature attacks. (Real Magic gives priority in cleanup when something triggers there; no card needs that yet.) |
| Extra turns / extra combats | Later | |

## Priority and the stack

| Rule | Status | Notes |
|---|---|---|
| Priority passing, stack LIFO | In | Real priority in engine. |
| Arena-style auto-pass | In | Engine reports whether the player has a meaningful action; the UI auto-passes on that. Hold-priority toggle in the UI. |
| Instants, flash | In | |
| Activated abilities | In | Built in M4a with mana, tap and sacrifice-a-creature costs. Pay-life costs when a card needs one. "Activate only as a sorcery" and "activate only if..." are built. |
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
| Land (basic + simple duals) | In | Duals: enter untapped / tapped variants only. Built in M3 with the Guildgates: "enters tapped" is a card property, and the auto-payer matches coloured symbols to sources. |
| Enchantment, Aura | In | Auras: "enchant creature" first. |
| Artifact | In | The type is built for one permanent only: the Festering Wound, the token a wound of the run puts on the battlefield (`DECISIONS.md` #68). No card in the pool is an artifact yet, and nothing targets or counts artifacts. |
| Equipment | Later | |
| Planeswalker cards | Later | Open question. |
| Vehicles, Sagas, Battles, Classes, Rooms | Out (for now) | |
| Tokens | In | Creature tokens are built (M4a), and one artifact token, the Festering Wound. Treasure needs mana from an artifact; Food and Clue are later. |
| Double-faced / split / adventure / MDFC | Out | |

## Keywords

**In (v1):** flying, reach, first strike, double strike, deathtouch, lifelink, trample, haste, vigilance, menace, defender, hexproof, flash, indestructible. All are built.

**In, added in M4a:** prowess and surveil, both built. Surveil has scry's simplification: one card at a time, and those kept stay in their order.

**Later:** ward, fight, kicker, flashback, cycling, landfall, cascade.

**Out:** protection (complex), banding, phasing, morph/disguise, mutate, partner, companion.

Scry is an effect primitive rather than a keyword here, and is **In**, simplified: the cards are looked at one at a time, top first, and each is kept or put on the bottom, so the ones kept stay in their order (real Magic lets you reorder them). With scry 1, which is all the pool has, there is no difference.

## State-based actions

| Rule | Status |
|---|---|
| 0 life loss | In |
| Creature with toughness ≤ 0 dies | In |
| Lethal damage / deathtouch damage destroys | In |
| Legend rule | In (not built: the only legendary cards are commanders, one a side, never in a deck) |
| Aura with illegal/no object goes to graveyard | In |
| +1/+1 and −1/−1 counters annihilate | Later |
| Token leaves battlefield → ceases to exist | In |

## Continuous effects

| Rule | Status | Notes |
|---|---|---|
| P/T modification until EOT | In | |
| Anthems (static +N/+N) | In | |
| Counters | In | +1/+1 counters are built (M4a); −1/−1 later. |
| Keyword grants | In | |
| "Can't attack / can't block" restrictions | In | Needed for Pacifism. |
| "Doesn't untap during its controller's next untap step" | In | Added in M4a for Frost Lynx; part of the tap effect. |
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
| "If +1/+1 counters would be put on a creature you control, that many plus one instead" | In, as a named exception | Conclave Mentor (`DECISIONS.md` #61). One check where counters are put; several add up. No choice of order, and no other "instead" card comes with it: each is asked about on its own. |

## Misc

| Rule | Status |
|---|---|
| Targeting, illegal targets on resolution (fizzle) | In |
| Hexproof / shroud | In / Out |
| Emblems (boss emblems, relics, passives) | In |
| Alternate win/lose conditions | Out |
| Hidden information (hand, library order) | In |
| Random discard, look at top N, reveal | In |
| Mill (library to graveyard) | In |
| Targets in a graveyard; returning a card from a graveyard to hand | In |
| "You may" on a triggered ability | Simplified: decided when the ability would go on the stack (decline, or choose its targets), not when it resolves. |

## Vision examples that don't fit this scope yet

`GAME_DESIGN.md` gives examples that need rules marked Later or Out above. They are illustrations, not commitments; each needs a scope decision before it is built (M3 onwards).

| Example | Needs |
|---|---|
| Boss emblem: "creatures have ward {1}" | Ward (Later) |
| Boss emblem: "upkeep, create a 2/2 Zombie" | Nothing: tokens are built, and an emblem can create one |
| Relic: "the first instant each turn costs {1} less" | Cost reduction (Simplified, not built) and per-turn counting |
| Relic: "+1/+1 counters, that many plus one instead" | General replacement effects (Out for now) |
| Passive: "your tokens enter with a +1/+1 counter" | Replacement effect (Out for now) |
| Boss relic: "you can't gain life" | Replacement/restriction on life gain |
| RW archetype: equipment | Equipment (Later) |
| Upgrade: "kicker waived" | Kicker (Later) |
| UB archetype: mill | Fatigue makes an empty library far less deadly than in Magic; mill needs its own payoff |
