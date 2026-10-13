# Roadmap

What is built, what comes next, and what is further off. The record of each milestone and slice, with its checks and sim numbers, is in `HISTORY.md`.

**Active milestone: M4** (content expansion). One character, the Warden, and one act, until a whole playthrough of it is interesting (`DECISIONS.md` #56, #71). Three acts come last.

## Built so far

| Milestone | What it gave | State |
|---|---|---|
| M0 | The pipeline from Rust to the browser | Done |
| M1 | The rules engine, the heuristic AI, the sim | Done |
| M2 | A duel playable in the browser | Done, played |
| M3 | One act: map, duels, elites, boss, rests, shops, relics | Done, played |
| M4a | The Scryfall tool, the rules a cube needs, 114 spells | Done, played |
| M4b | The searching AI | Done, played |
| Commanders | A legendary creature in the command zone for characters, elites and the boss | Done, played, kept (#55) |
| The green-white cube | Five packages (go wide, +1/+1 counters, lifegain, big creatures, Auras), 104 cards offered | Done, played |
| The longer act | 11 rows and the boss; Draft, Treasure and Event nodes; 15 relics; 5 events | Done, played |
| Wounds and feats | In place of hearts and boons (#68, #69) | Done, numbers untuned |
| The balance pass | A weaker start, five enemy tiers, a boss at 50 life (#70, `BALANCE_REPORT.md`) | Done, played; some questions open |
| Card upgrades | Every Warden card has an upgraded side; a rest site upgrades one (#72) | Done, to be tuned |
| The player starts; stronger smoothing | The player takes the first turn of a run's duels; hands are dealt until the lands fit and every colour has a source, up to 5 (#74) | Done, to be played |
| Healing all; five wounds | A rest site and the Moneylender heal every wound; a lost duel gives one of five kinds, never one already carried (#75) | Done, to be played; numbers untuned |
| Seeing what is ahead | The boss is on the map from the start, with a panel beside it; each elite node names its commander, and no two are the same elite (#76) | Done, to be played |

Where the numbers stand, with the AI on both sides (`sim --runs --games 600 --decks warden`): the random picker wins 39.2% of acts (+/- 3.9), reaches the boss in 88% and beats it 44% of the time; it wins 78% of its ordinary duels and 23% of its elites. That command's 600 seeds are a lucky sample since the enemies moved onto the map (#76), which changed what every seed rolls: the same code wins 36.3% on seeds 601 to 1200, and the code before it won 34.3% and 37.7% on the two ranges, so the floor is about 37% and did not move (`HISTORY.md`, slice 15). The sim's player heals at a rest when one loss from the end, and it never meets the boss wounded. The floor was 29.3% before healing removed every wound (`HISTORY.md`, slice 14): the field has not been retuned for that. Each package's ideal deck won 83 to 91% of acts before upgrades (not measured since).

## Next: the backlog

From the owner's playtest of 2026-10-13. Each item is meant to be done on its own, in a fresh session, and says what was asked, what exists now, what I think, and what has to be settled first. They are the owner's suggestions and none is decided unless it says so; an item marked **propose first** needs a short proposal before any code. The order is my suggestion: the cheap ones that change how every duel feels come first, because they move every number measured after them.

For every item: logic in `crates/engine`, numbers in `data/balance.json`, a test for every rule, the docs in the same change, and `sim --runs --games 600 --decks warden` before and after (the baseline is the table above; say what moved, with the margin, and don't retune the field without asking).

### 1. Elites are very hard

**Asked:** "elites are really hard: what do the stats show?"

**What they show** (`BALANCE_REPORT.md` §7, 100 duels a cell, and this session's runs):

| | Bare starting deck | Random picker, in a run | A package's ideal deck |
|---|---|---|---|
| Town Militia (Kongming) | 0% | | 60 to 80% |
| Angelic Host (Radiant) | 14% | | 52 to 75% |
| Plague Court (Elas il-Kor) | 1% | | 77 to 89% |
| Wild Hunt (Lady of the Mountain) | 1% | | 74 to 94% |
| All elites | | 19% won, 0.8 fought a run | 65 to 78% in whole acts |

So they are hard for everyone and close to unwinnable for an unfinished deck. The cause is mostly where they sit: an elite can be met on row 5, after two easy duels, a draft and a treasure or an event, with the starting deck and about four more cards; the same four elites are also the pool for row 9. An elite is always optional, and losing one leaves a wound and no relic.

**Options:** (a) the player picks the fight: built, the map names each elite by its commander (#76), which helps a player and not the sim's, who doesn't choose by it; (b) two elite tiers, an early pool for row 5 built like the normal tier with a commander, and the present four for row 9; (c) keep one pool and move the first elite row later; (d) pay more for an elite, so the risk is worth it. I would do (b). With two tiers, `place_enemies` deals each elite row from its own pool; whether the hard duels of rows 9 and 10 are shown as well was left for this item (#76).

### 2. The Pyre, and more bosses

**Asked:** the Pyre is fine as it stands (and so is the boss at 50 life: `DECISIONS.md` #73). Perhaps make its emblem more conditional, or have it attack with a 1/1 each turn in place of the drain.

**Now:** the Pyre Tyrant (`data/run/enemies.json`): Borborygmos, 50 life, and an emblem that makes the player lose 1 life at each of its upkeeps. The finding of the balance pass is that the drain makes lifegain every package's answer (three of five ideal lists hold two Healer's Hawks). Earlier the owner asked for several act-one bosses with different abilities, not one retuned.

**Notes:**
- A 1/1 each upkeep is buildable today (an emblem can create a token). It asks "can you hold the ground and still race?" instead of "can you gain life?", which every creature deck answers in its own way, so it should spread the packages better. What it may lose is the gap between a focused deck and a random one, which the drain is what makes (report §10): that is the thing to measure.
- A conditional drain to try: only while the boss controls a creature with power 4 or greater, or only while you have more life than it.
- Both can be measured without a rebuild: copy `data/`, edit the emblem, and run `sim --act --games 300 --data DIR` for the five ideal decks and `sim --runs` for the floor.

- A second boss needs no new code to be met or shown: `place_enemies` picks one of the `boss` tier when the run starts, and the map and its side panel show whichever it is (#76).

**To settle:** after the measurements, which version the Pyre keeps, and what the second and third bosses ask (proposals wanted: one that punishes going wide, one that punishes one big threat, for instance).

### 3. More life as the act goes on

**Asked:** later enemies should have more life, and the player's life might rise at points in the run too.

**Now:** everyone starts at 20 (`duel.starting_life`); a modifier can change either side's (`starting_life`), which is how the boss has 50 and Traveller's Rations gives 2.

**Notes:**
- Life is the one lever that hurts a random deck far more than a focused one (report §1), so more life on the late rows should widen the gap in the right direction and make the hard tier less of a race.
- The player's life is strong: 2 more life held for a whole act measured +6 points of act win rate (+/- 5). So small steps, and it is a natural reward (after an elite, or a choice at a rest) more than a schedule.
- Things that read life as a number need a look when it moves: the Unbroken feat (never below 12 / 15 / 18). Twinblade Paladin and Speaker of the Heavens already count from the starting total.

**To settle:** by row or by tier, and how much (suggest a `life` number per map row or per tier in `balance.json`); where the player's life comes from.

### 4. Potions (propose first)

**Asked:** single-use potions as in Slay the Spire, common to rare: "destroy target creature with flying", "destroy all creatures", "gain 5 life". They are attrition too, in that one spent is gone.

**Now:** nothing is built. `GAME_DESIGN.md` §3.2 has them planned as consumables from events, treasures and shops. Most effects exist as primitives (destroy with a filter, gain life, draw, counters, tokens); "destroy all creatures" would be a new one, close to `deal_damage_to_each_creature`.

**Notes:**
- A potion is a spell outside the deck, so the questions are Magic's: when can it be used (any time you have priority, or only in your main phase), does it use the stack, can it be countered, does it cost mana? Free and at instant speed is the strongest and the least like Magic.
- "Destroy all creatures" with no cost would change how a duel is played more than anything else in the game. The owner's answer (2026-10-13): it is a very rare potion, and may cost mana, perhaps {5}. So a potion can have a mana cost, and the strong ones do.
- Potions can't be countered, by default (the owner, 2026-10-13). `cant_be_countered` exists for cards.
- The enemy AI doesn't know the player holds one, and the sim's player needs a rule for using them, or runs can't measure them.

**To settle (the proposal):** how many can be carried, where they come from, the timing rule, the first ten or so with rarities, and what the sim does with them.

### 5. Lands apart from card rewards (propose first)

**Asked:** separate lands from the normal rewards, and let the player adjust their lands, for example to pivot into one colour.

**Now:** basic lands are automatic: 40% of the deck (`run.lands.land_percent`), split by the coloured symbols of the spells, at least 2 a colour used (`lands.rs`). Non-basic lands (the Guildgates) are cards of the deck that each replace a basic; a card reward has a 15% chance of holding one (`run.rewards.land_percent`) and a shop sells one. The roadmap has long had "a land collection with adjustable basics".

**Notes:** two things are being asked and they can be built apart. Taking lands out of rewards is small (they stay in shops, or come some other way). Adjusting lands is a screen and a rule: the engine suggests the split as now, and the player may move basics between colours and choose which owned non-basics to play, within a total the engine sets.

**To settle (the proposal):** where non-basic lands come from once rewards don't hold them, whether the number of lands can change or only their colours, and whether the choice is made on the deck screen at any time or at certain nodes.

### 6. Quality of life

Small and separate; any of them fits beside another item.

- **See the cards left in your library.** A list sorted by name or cost, not in order, like the graveyard viewer. Check what `PlayerView` gives of the player's own library: it must show the contents without the order.
- **See what a card's upgrade would be, from the deck.** The deck screen shows upgraded copies already; this is the other side of a card that isn't upgraded yet (a toggle, or beside the hover preview). The data is there: `CardDef.upgrade`.
- **A red outline on cards in hand when discarding,** to show that the click is a loss.
- **Reordering cards in hand.** The order is the UI's own: the engine's hand order means nothing to the rules.

### 7. Left from earlier work

- **Tune the upgraded sides.** All 112 are first guesses (`GAME_DESIGN.md` §6.1). To look at first: Lightwalker+ (a 3/2 flier for two), Pacifism+ ({W}), Glorious Anthem+ ({W}{W}), Gigantosaurus+ (trample).
- **An event that removes a card,** now that a rest doesn't (#72).
- **After healing all (#75).** The floor rose 5 points (to 34.3%) and nobody reaches the boss wounded: if that is too kind, the last rest could heal one wound only. The four new wounds' numbers are first guesses and none was measured on its own. A wound from the Moneylender (100 gold) or the Devil's Bargain (a relic) is cheaper than it was, since the next rest wipes it: watch both.
- **Feat numbers.** Tempered and Adorned are met in about 6% of wins; Dominant, Decisive and Legion nearly always at their first tier.
- **The balance report's open questions** (§11, numbers 3 to 13): the starting deck, Hardened Scales, the three altered rares, the 22 cards held back and which of them an event might give, Glorious Anthem or Benalish Marshal, relic tiers, the rares passed over.
- **`ARCHITECTURE.md` §4** has grown a paragraph per package. It wants to be reference tables (triggers, effects, statics, conditions, counts, card properties), checked against the Rust enums.

## After the backlog

- [ ] Three acts and three bosses, about 20 relics (15 so far, 13 of them offered).
- [ ] More characters, each with a pool of its own. The Firebrand is parked (#71): its cards have no upgraded sides and it wins about 3% of acts against the present field.
- [ ] The cube toward 250 to 300 cards across the ten colour pairs. Green-white is the only pair with packages.
- [ ] Balance passes with the sim once a pool is real. Carried over from M1: Deck A is a little overtuned and the Deck B and C mirrors run to fatigue.

## M5: polish and sharing

- [ ] Card art (where permitted), map art, sound, better animations.
- [ ] Save and resume a run (serialise `RunState`).
- [ ] Settings, tutorial hints.
- [ ] Deploy to Render or itch.io; share with friends; collect feedback.

## Later, maybe

- Meta progression, ascension levels.
- A separate land deck (`GAME_DESIGN.md` §4.1).
- A Tauri desktop build.
- An original card set in place of Magic's cards, which any commercial release needs.
- The AI getting more competent as the run goes on.
