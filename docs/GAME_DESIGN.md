# Game Design — Long-Term Vision

> This describes the **full game we're aiming for**. Most of it is **not** in scope for the prototype. Check `ROADMAP.md` for what to build now. Numbers here are starting points for playtesting; they live in config, not code.

## 1. Pitch

Real Magic: The Gathering duels inside a Slay the Spire–style roguelike. Start with a small deck and a chosen planeswalker, travel a branching map, win duels, and grow your deck toward a two-colour archetype, supported by relics that bend the rules in your favour.

## 2. Design pillars

1. **It plays like real Magic.** The stack, instant-speed interaction, combat tricks, and keywords matter. Depth over simplification, but with Arena-style auto-passing so it doesn't feel slow.
2. **Every pick matters.** Small decks, focused archetypes, meaningful relics. You should feel your deck take shape.
3. **Bad luck shouldn't end a run.** Magic has variance; the run structure absorbs it (separate run health, opening-hand smoothing, fatigue instead of decking).
4. **Archetypes play as intended.** Lifegain, pay-life, aggro, control: none is warped by run mechanics.
5. **Looks and feels great.** Visual polish inspired by MTG Arena / Magic 2014 and Slay the Spire.
6. **Short enough to play with friends.** A run should take roughly 45–75 minutes.

## 3. Run structure

- A run is **3 acts**. Each act is a **branching map** of nodes ending in a **boss**.
- The player sees the whole act map and chooses a path (Slay the Spire style). Paths trade risk for reward (more elites, fewer rest sites, etc.).
- Beating the act 3 boss wins the run.

### 3.1 Hearts (run health)

In-duel life and run health are **separate**.

- Every duel starts at **20 life** for both players, regardless of how the last one went.
- The player has **hearts** (start: **3 per act**). Hearts refill to full at the start of each act.
- **Losing a normal duel**: lose 1 heart, receive a reduced reward (e.g. gold only, or pick 1 of 2 commons).
- **Losing an elite**: lose 1 heart, no relic.
- **Hearts reach 0 before the boss**: run over.
- **Losing a boss duel**: run over, regardless of hearts.
- **Boons**: before the boss, each remaining heart buys **one boon** from a menu, for that boss duel only. Example boons:
  - Start with +5 life
  - +1 card in opening hand
  - Boss starts with −5 life
  - Free mulligan to 7
  - Start with an extra land on the battlefield
- Keep boons modest. A heart is an edge, not a win. Watch for snowballing.

### 3.2 Tying duels together

Because life doesn't carry over, each duel stands alone unless something else links them. Gold bonuses for winning fast or at high life were tried in M3 and dropped: they favour some archetypes over others (`DECISIONS.md` #36). The planned link is **consumables**: single-use items, each about as strong as a boon, earned at events, treasure and shops and spent in a duel of the player's choosing. A later maybe: wagers, where the player gives the enemy an emblem for a better reward.

## 4. The duel

Real Magic rules within the subset in `RULES_SCOPE.md`, with these run-specific changes:

### 4.1 Lands

Lands are **in the deck**, as in normal Magic, so every card works as printed. Two things keep mana problems from deciding runs:
- **Opening-hand smoothing**: the game deals a few candidate opening hands behind the scenes and gives you the one whose land count best matches your deck. Plus the London mulligan.
- **Automatic basic lands** (run layer, M3): the game keeps your basic land count at a target share of the deck (starting guess: 40%, split by the colours of your spells) and adjusts it as you add and remove cards. No land-ratio busywork.

**Interesting lands are deck-building choices.** Dual lands, utility lands and the like are cards you earn (shops, rewards, relics) and add to your deck. Each one takes the place of a basic, so it improves your mana rather than diluting your spells.

Opponents follow exactly the same rules.

Future idea, not planned: a **separate land deck**, where lands live in their own shuffled deck and you choose which deck to draw from on every draw. It removes screw and flood completely, but changes how scry, card draw, explore, landfall and mill behave, so it would need its own card pool review.

### 4.2 Deck size

Small decks so each pick shows up in games.
- Starting deck: ~15 spells + ~10 lands.
- No hard maximum, but adding cards dilutes; card removal is a valued service.
- **Fatigue**: running out of cards doesn't lose the game. If you would draw from an empty library, you lose 1 life instead, then 2 the next time, then 3, and so on.

### 4.3 Opening hand and mulligans

Opening hand of 7, smoothed as above. London mulligan.

The starting player is chosen by coin flip and skips their first draw.

## 5. Planeswalkers (player characters)

At run start, pick a planeswalker. Each provides:
- Starting colours (usually two, sometimes mono).
- A starting deck, lands included.
- A **passive** that nudges toward certain archetypes without locking you in. Examples:
  - "The first creature you cast each turn gets +1/+1 until end of turn."
  - "Whenever you cast your second spell in a turn, scry 1."
  - "Your tokens enter with a +1/+1 counter."
- Later: unlockable planeswalkers via meta progression.

These are original characters (or generic archetypes) — not MTG's named planeswalkers — to keep the IP path clean.

## 6. Map nodes

| Node | What happens |
|---|---|
| **Duel** | Normal opponent. Win: pick 1 of 3 cards (rarity-weighted) + gold. |
| **Elite** | Stronger deck. Win: relic + card pick. Loss: −1 heart, nothing. |
| **Boss** | Ends the act. Has emblems (see §8). Win: rare/mythic pick + relic. Loss: run over. |
| **Draft** | Open a booster-style pack from the cube, pick one card. Very Magic. |
| **Shop** | Buy cards, lands (incl. duals), relics. Pay to remove a card. |
| **Rest** | Choose: upgrade a card, or remove a card. (Healing isn't needed with hearts; possibly "restore 1 heart" as a third option — playtest.) |
| **Event** | Short text event with a trade-off choice (e.g. "add a powerful card with a drawback", "sacrifice a relic for two cards"). |
| **Treasure** | Free relic or gold. |

### 6.1 Card upgrades

MTG cards don't have upgrades, so upgrades are run-specific modifiers. Options to playtest:
- Permanent +1/+1 counter on a creature card.
- Reduce generic cost by 1 (min 1 total).
- Kicker / additional cost waived.
- Add a minor keyword from an approved list.
Upgraded cards show a visible foil/frame treatment.

## 7. Relics

Passive, run-long modifiers. Primary purpose: **reward archetypes** and make runs feel different. Rarity tiers: common, uncommon, rare, boss.

Examples:
- Whenever you put one or more +1/+1 counters on a creature, put that many plus one instead.
- The first instant you cast each turn costs {1} less.
- Start each duel with a Treasure token.
- At the start of each duel, create a 1/1 white Soldier token.
- Whenever you gain life for the first time each turn, scry 1.
- Your deck gains a dual land of your colours at the start of each act.
- Boss relics: strong but with a drawback ("+1 card each draw step; you can't gain life").

## 8. Opponents

- Opponents play **real Magic with real decks**, controlled by AI.
- Decks are hand-built per encounter, themed around an archetype and act-appropriate power level.
- **AI** is the hardest piece. Start simple (play best creature on curve, attack when favourable, remove the biggest threat) and improve gradually. Difficulty should come from deck strength and boss emblems more than AI brilliance.
- **Elites**: stronger decks, maybe one minor emblem.
- **Bosses**: themed decks plus **emblems** — rule-breaking passives, Archenemy-scheme style:
  - "At the beginning of the boss's upkeep, create a 2/2 black Zombie token."
  - "The boss's creatures have ward {1}."
  - "Whenever you cast your first spell each turn, the boss scries 1."
  - Multi-phase bosses (emblem changes at 10 life) as a later stretch.

## 9. Card pool (the cube)

- A curated **cube of ~250–300 cards** from roughly the last few years of sets (exact sets TBD).
- Built around **10 two-colour archetypes**, like a Limited set:

| Pair | Archetype (draft) |
|---|---|
| WU | Fliers / tempo |
| UB | Control / mill |
| BR | Sacrifice / aristocrats |
| RG | Big creatures / ramp-stomp |
| GW | Go-wide tokens |
| WB | Lifegain / drain |
| UR | Spells-matter |
| BG | Graveyard / recursion |
| RW | Aggro / equipment |
| GU | +1/+1 counters / ramp |

- **Colours are locked** (`DECISIONS.md` #35): a character is only ever offered cards in its colours. The variety is meant to come from inside the pool, as in Slay the Spire: each colour pair should hold three or four overlapping packages of 8–12 cards (for black-red, say: sacrifice, vampires, burn, discard), with cards that belong to two of them, so that the pick is rarely obvious. Starting decks stay generic.
- Archetypes the owner wants in time, beyond the ten above: discard, vampires, counterspells, spellslinger, walls and defenders, Eldrazi. Released Commander decks are a source of ideas. Walls (damage from toughness) and Eldrazi (colourless mana and their keywords) each need a rule that is not in scope yet.
- Every card must fit inside `RULES_SCOPE.md`.
- Rarity distribution and reward weighting tuned in playtesting.
- Card text/data sourced from Scryfall bulk data.

## 10. Meta progression (later, ideas only)

- Unlock new planeswalkers by winning with others.
- Unlock cards/relics into the pools.
- Ascension-style difficulty levels.
- Run history / stats.
Keep it light; avoid permanent power creep.

From the owner's M3 playtest (2026-10-11), the direction to explore once saving exists (M5):

- **Commanders.** The character could be a commander: a legendary creature the player can cast from the command zone, with decks tied to it. An enemy's commander would also hint at what its deck does. A prototype was built after M4a (`DECISIONS.md` #45, #46): each character has a commander in its colours and picks a starting emblem to go with it, and elites and the boss have commanders too. Whether it stays is decided before M4c (`DECISIONS.md` #37).
- **AI that grows.** Enemies could play more competently as the run goes on. For now they all play at one strength.
- **Unlocks by achievement.** New commanders unlocked by completing achievements; their alternative decks and skills by completing goals of their own.
- **A small skill tree per commander**, outside the run, that changes its abilities slightly: a little roguelite, not a power ladder.

## 11. Visual and UX direction

- Battlefield layout inspired by MTG Arena: hands at bottom, opponent at top, stack shown clearly, hover to enlarge cards.
- Run/map screens inspired by Slay the Spire: hand-drawn-feeling map, clear node icons, readable relic bar.
- Satisfying animations: card draw, cast, combat damage, death, token creation.
- Arena-style priority: auto-pass when the player has nothing relevant, with a toggle to hold priority.
- Readability over flash: every card's state (tapped, counters, summoning sick, modified P/T) is obvious.
- Accessibility: colour-blind safe indicators; don't rely on colour alone.

## 12. Platforms and distribution

- Primary: browser (static site), easy to share with friends.
- Later: desktop build via Tauri for Steam if the game goes commercial.

## 13. IP note

- The prototype uses real MTG cards for a **free, non-commercial** fan project shared with friends. Check Wizards of the Coast's Fan Content Policy before sharing publicly.
- **If the game is ever sold**, all MTG cards, names, and art must be replaced with original content. The engine is card-agnostic so this is a data swap, not a rewrite. Never build features that depend on specific MTG branding.

## 14. Open questions

- Exact sets for the cube.
- Planeswalker cards as a card type: in or out for v1?
- Run length vs map size: 45–75 minutes doesn't fit three acts of ~15 rows. Settle by playtesting.
- How does mill win, now that an empty library means fatigue rather than a loss?
- Several examples in §5, §7 and §8 need rules outside the current scope (see the last table in `RULES_SCOPE.md`).

- Self-mill in a 25-card deck runs into fatigue (seen in the M4a sim). What makes a graveyard deck worth it here?

Settled for the one-act prototype (see `DECISIONS.md` #22 to #31): rest sites restore a heart or remove a card; elites are always optional; a drawn duel costs nothing; basic lands are 40% of the deck, split by mana symbols.
