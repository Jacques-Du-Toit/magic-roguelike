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

### 3.1 Wounds (run health)

In-duel life and run health are **separate**. (Hearts and boons were the first version of this; wounds replaced them, `DECISIONS.md` #68.)

- Every duel starts at **20 life** for both players, regardless of how the last one went.
- **Losing a duel or an elite** leaves a **wound**, and still pays a little: some gold and a pick from 2 cards. An elite lost gives no relic.
- A wound follows the player into **every duel** until a rest site heals it. **Healing removes every wound carried**, not one (`DECISIONS.md` #75).
- There are **five kinds**, each a small nuisance, and a lost duel gives one at random of a kind not carried already:
  - **Festering Wound**: an artifact on the player's battlefield that deals 1 damage to them at the beginning of their end step, until they pay {2} to be rid of it for that duel. The only one that can be paid off.
  - **Open Gash**: at the beginning of combat on the enemy's turn, a random creature the enemy controls gets +1/+1 until end of turn.
  - **Concussion**: you start each duel with one card fewer.
  - **Shaken**: the enemy starts each duel with 2 more life.
  - **Hunted**: the enemy starts each duel with a 1/1 Soldier token on the battlefield.
- **The third wound** ends the run.
- **Losing a boss duel**: run over, however few wounds are carried. The boss is fought with the deck, the relics and the wounds: nothing else.
- Later wounds may be harsher, and acts may differ in which they give.

### 3.2 Tying duels together

Because life doesn't carry over, each duel stands alone unless something else links them. Two things do (`DECISIONS.md` #68, #69):

- **Wounds** carry a loss forward into every duel after it.
- **Feats** give every duel something to play for beyond winning. Before a duel the player picks two of five feats on offer ("never fall below 12 / 15 / 18 life", "control 4 / 6 / 8 creatures at once"). Each scores a point for each tier met if the duel is won, and the points set how good the card reward is. Missing a feat costs nothing. A character has a pool of feats: some shared, some its own. The boss has none.

Gold bonuses for winning fast or at high life were tried in M3 and dropped as a standing rule, because they favour some archetypes over others (`DECISIONS.md` #36); as feats they are two choices among several. Still planned: **consumables**, single-use items earned at events, treasure and shops and spent in a duel of the player's choosing. Later maybes: feats that belong to an elite or an event, feats collected during the run like relics, and wagers, where the player gives the enemy an emblem for a better reward. The ideas weighed on the way are in `docs/ATTRITION_IDEAS.md`.

## 4. The duel

Real Magic rules within the subset in `RULES_SCOPE.md`, with these run-specific changes:

### 4.1 Lands

Lands are **in the deck**, as in normal Magic, so every card works as printed. Two things keep mana problems from deciding runs:
- **Opening-hand smoothing**: behind the scenes the game deals opening hands until one is right, and gives you that one. A hand is right when its land count fits your deck's share of lands (2 or 3 of seven, at 40%) and every colour among its spells has a land in the hand that makes it: white cards in hand mean at least one Plains, or a land that taps for white. A dual land counts as one source of each of its colours. After five hands it stops and gives you the one that misses least. Every mulligan hand is dealt the same way. Plus the London mulligan.
- **Automatic basic lands** (run layer, M3): the game keeps your basic land count at a target share of the deck (starting guess: 40%, split by the colours of your spells) and adjusts it as you add and remove cards. No land-ratio busywork.

**Interesting lands are deck-building choices.** Dual lands, utility lands and the like are cards you earn (shops, rewards, relics) and add to your deck. Each one takes the place of a basic, so it improves your mana rather than diluting your spells.

Opponents follow exactly the same rules, the stronger smoothing included.

Future idea, not planned: a **separate land deck**, where lands live in their own shuffled deck and you choose which deck to draw from on every draw. It removes screw and flood completely, but changes how scry, card draw, explore, landfall and mill behave, so it would need its own card pool review.

### 4.2 Deck size

Small decks so each pick shows up in games.
- Starting deck: the Warden's is 10 spells and 7 lands (it began at 15 and 10).
- No hard maximum, but adding cards dilutes; card removal is a valued service.
- **Fatigue**: running out of cards doesn't lose the game. If you would draw from an empty library, you lose 1 life instead, then 2 the next time, then 3, and so on.

### 4.3 Opening hand and mulligans

Opening hand of 7, smoothed as above. London mulligan: seven fresh cards, smoothed again, then one to the bottom for each mulligan taken.

In a run the player always takes the first turn, and skips their first draw as the starting player does in Magic (`DECISIONS.md` #74). A practice duel still flips a coin.

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
| **Duel** | Normal opponent, not known until you arrive, with two feats picked beforehand (§3.2). Win: gold and a pick from 2 to 4 cards, by how the feats went. |
| **Elite** | Stronger deck. Win: relic + card pick. Loss: a wound, no relic. The map shows which elite it is by its commander's name, with the card on hover, so the player picks the elites that suit their deck; no two elite nodes of a map are the same elite (`DECISIONS.md` #76). |
| **Boss** | Ends the act. Has emblems (see §8). Win: rare/mythic pick + relic. Loss: run over. Known from the start of the run: its name is on its node, and its commander, life and emblems are in a panel beside the map (`DECISIONS.md` #76). |
| **Draft** | Open a pack of five cards in your colours and keep up to two. Built (`DECISIONS.md` #57). |
| **Shop** | Buy cards, lands (incl. duals), relics. Pay to remove a card. |
| **Rest** | Choose: heal every wound you carry, or upgrade a card (§6.1). Removing a card is the shop's service. |
| **Event** | Short text event with a trade-off choice (e.g. "add a powerful card with a drawback", "sacrifice a relic for two cards"). Built with five events, each of which can be walked away from (`DECISIONS.md` #66). |
| **Treasure** | Choose one of two relics you don't own, or take gold instead. Built (`DECISIONS.md` #64). |

### 6.1 Card upgrades

MTG cards don't have upgrades, so an upgrade is the run's own. Built as in Slay the Spire (`DECISIONS.md` #72): **every card has one upgraded side of its own**, written by hand, and upgrading a copy turns it into that side for the rest of the run. "Raise the Alarm+" makes three Soldiers; "Grizzly Bears+" costs {G}; "Lightwalker+" enters with a +1/+1 counter.

- **Where**: a rest site, in place of healing your wounds. One card a rest, one upgrade a card. Nothing else gives one yet.
- **What a side changes**, in rough order of how common it is: generic mana off the cost, power and toughness, a number in the text (tokens made, life gained, counters put, a threshold), a counter to enter with, a keyword, or a line of text. Coloured mana never changes.
- **How much**: the weaker the card, the better its upgrade. Commons and the starting deck get the big ones (+1/+1 on a two-drop, a third token, a card drawn); uncommons a mana or a point of toughness; rares a small one (+0/+1, a cheaper ability). So the best card to upgrade is often not the rarest, and a starting card kept and upgraded is a real card.
- **By cost**: a cheap card's upgrade is small in absolute terms and large for the card; a six- or seven-drop gets a whole mana off or two points of size.
- Upgrades stay inside the rules that are built: no upgrade needs a new one. Kicker is out of scope.
- An upgraded card has a gold rim and a "+" after its name, in the deck and in a duel.

Every one of the 112 is a first guess, to be tuned by play. Not built: upgrades for the other colours' cards, an upgrade from an event, a shop or a feat, and anything for the commander, which may get something of its own (as in Monster Train).

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
- The first pool built this way is green-white, for a commander about creatures in general (`DECISIONS.md` #56, #58). Its five packages: go wide (tokens), +1/+1 counters, lifegain, big creatures (ramp), Auras and enchantments.
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

- **Commanders.** The character could be a commander: a legendary creature the player can cast from the command zone, with decks tied to it. An enemy's commander would also hint at what its deck does. A prototype was built after M4a (`DECISIONS.md` #45, #46): each character has a commander in its colours and picks a starting emblem to go with it, and elites and the boss have commanders too. Commanders stay (`DECISIONS.md` #55); the starting emblems were taken out again in the balance pass (#70).
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

Settled for the one-act prototype (see `DECISIONS.md`): rest sites heal every wound or upgrade a card (#72, #75); wounds are of five kinds (#75); elites are always optional; a drawn duel costs nothing; basic lands are 40% of the deck, split by mana symbols; the player takes the first turn of every duel in a run, and hands are smoothed for land count and for colours (#74).

Under discussion after the owner's playtest of 2026-10-13, each with a brief in `ROADMAP.md`: the boss and the elites' commanders shown on the map; easier early elites; a different question from the Pyre, and more bosses; more life for later enemies and for the player; potions; lands apart from card rewards, with a way to adjust them.
