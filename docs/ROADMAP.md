# Roadmap

Build in small milestones. Each has **acceptance criteria**; a milestone is done only when all are met and `cargo test` passes. Don't start the next milestone's features early.

**Active milestone: M4** (M4a, the commander prototype and M4b are done and played. Next: the green-white cube, before the rest of M4c; three acts come last. `DECISIONS.md` #55, #56)

---

## M0 — Project skeleton

Goal: an empty but working pipeline from Rust to browser.

- [x] `git init`, first commit, GitHub remote.
- [x] Cargo workspace with `crates/engine`, `crates/engine-wasm`, `crates/sim`.
- [x] `web/` Vite + React + TypeScript (strict) app.
- [x] Engine exposes one trivial function through wasm; web app calls it and displays the result.
- [x] One Rust type is exported to TypeScript with `ts-rs` and used by the web app.
- [x] npm script to build wasm; `npm run dev` works end to end.
- [x] `.gitignore`, `cargo fmt`, clippy, Vitest set up.
- [x] `CLAUDE.md` commands section verified accurate.

**Done when:** a fresh clone can follow `TECH_STACK.md` and see the page load data from Rust.

---

## M1 — Rules engine core (no UI)

Goal: two AI players can play a full, correct game of Magic with ~25 simple cards, headlessly.

- [x] Card data loader + validation for `data/cards/*.json`; decklist loader for `data/decks/*.json`.
- [x] `data/balance.json` with the duel numbers (starting life, opening hand, max hand size, fatigue). Nothing hard-coded.
- [x] Zones, objects, `ObjectId` + `CardId`, seeded RNG, serialisable `DuelState`.
- [x] Full turn structure, priority, stack, state-based actions.
- [x] Lands in the library, opening-hand smoothing, London mulligan, one land drop per turn, fatigue.
- [x] Mana payment (coloured + generic) via the engine auto-payer.
- [x] Combat incl. first strike, trample, deathtouch, lifelink, flying/reach, menace, vigilance, haste; automatic damage assignment.
- [x] Spells, ETB triggers, simple activated mana abilities, auras (Pacifism), counterspells.
- [x] `legal_actions`, `has_meaningful_action`, `apply_action`, `player_view`, events + redaction, `result`.
- [x] Simple heuristic AI.
- [x] `sim` CLI: run N AI-vs-AI games with seeds across all deck pairings; report win rates and any errors.
- [x] Tests for every card and keyword; regression tests for bugs found.

Not in M1: manual mana tapping, manual damage division, and any rule in `RULES_SCOPE.md` that no starter card needs.

**Starter card set (real cards, simple rules):**

| Card | Cost | Notes / what it tests | Deck |
|---|---|---|---|
| Plains, Island, Swamp, Mountain, Forest | — | Land drops, mana | all |
| Savannah Lions | {W} | 2/1 vanilla | A |
| Healer's Hawk | {W} | 1/1 flying, lifelink | A |
| Youthful Knight | {1}{W} | 2/1 first strike | A |
| Inspiring Overseer | {2}{W} | 2/1 flying; ETB gain 1 life and draw a card | A |
| Serra Angel | {3}{W}{W} | 4/4 flying, vigilance | A |
| Pacifism | {1}{W} | Aura: can't attack or block | A |
| Wind Drake | {2}{U} | 2/2 flying | B |
| Divination | {2}{U} | Sorcery: draw two | B |
| Cancel | {1}{U}{U} | Counter target spell | B |
| Typhoid Rats | {B} | 1/1 deathtouch | B |
| Vampire Nighthawk | {1}{B}{B} | 2/3 flying, deathtouch, lifelink | B |
| Murder | {1}{B}{B} | Instant: destroy target creature | B |
| Raging Goblin | {R} | 1/1 haste | C |
| Shock | {R} | Instant: 2 damage to any target | C |
| Lightning Strike | {1}{R} | Instant: 3 damage to any target | C |
| Boggart Brute | {2}{R} | 3/2 menace | C |
| Hill Giant | {3}{R} | 3/3 vanilla | C |
| Llanowar Elves | {G} | Mana creature, summoning sickness | A |
| Elvish Visionary | {1}{G} | 1/1; ETB draw a card | A |
| Grizzly Bears | {1}{G} | 2/2 vanilla | A |
| Giant Growth | {G} | Instant: +3/+3 until EOT (combat trick) | A |
| Centaur Courser | {2}{G} | 3/3 vanilla | A |
| Giant Spider | {3}{G} | 2/4 reach | A |
| Colossal Dreadmaw | {4}{G}{G} | 6/6 trample | A |

Verify each card's exact text against Scryfall when entering it.

**Three test decks** (25 cards: 15 spells + 10 lands, shuffled together). Starting lists; tune freely.

- **Deck A — Green-White creatures and tricks.** Llanowar Elves ×2, Youthful Knight ×2, and one each of Savannah Lions, Healer's Hawk, Grizzly Bears, Elvish Visionary, Inspiring Overseer, Centaur Courser, Giant Spider, Serra Angel, Colossal Dreadmaw, Giant Growth, Pacifism. Lands: 5 Forest, 5 Plains.
- **Deck B — Blue-Black fliers, removal and counterspells.** Typhoid Rats ×3, Wind Drake ×3, Murder ×3, Vampire Nighthawk ×2, Cancel ×2, Divination ×2. Lands: 5 Island, 5 Swamp.
- **Deck C — Mono-red aggro and burn.** Raging Goblin ×3, Boggart Brute ×3, Hill Giant ×3, Shock ×3, Lightning Strike ×3. Lands: 10 Mountain.

**Done when:**
- `cargo run -p sim -- --games 1000` completes across all deck pairings with zero errors/panics.
- Running the same seed twice produces an identical event log (tested).
- The sim reports, per pairing: win rate, play/draw win rate, average game length, mulligan rate, how often a player is stuck on two or fewer lands by turn 4, and how many games ended by fatigue or the turn cap. As a guide, no pairing should be worse than about 75/25 and fatigue or turn-cap endings should be rare. Deck B is expected to underperform in AI hands (see `ARCHITECTURE.md` §7).
- Every starter card has tests.

**Result (2026-10-11):** 6,000 games (1,000 for each of the six pairings, mirrors included) ran with zero errors. Cross pairings: A beats B 64/36, A beats C 78/22, C beats B 62/38, with fatigue endings under 0.5%. A against C is a little outside the 75/25 guide. The mirrors are slower: Deck B's mirror ends by fatigue in 57% of games and Deck C's in 11%, because both sides remove nearly every creature. Deck A looks a bit overtuned. The owner chose to leave the decklists as they are and balance them later (see M4), to get to playtesting sooner.

---

## M2 — Playable duel in the browser

Goal: a human can play a full duel against the AI in the browser, and it's fun. Default matchup: the human plays Deck B, the AI plays Deck A or C (the v1 AI can't pilot a reactive deck).

- [x] Duel screen: battlefield, hands, library, life totals, stack, graveyard viewer.
- [x] Mulligan screen (keep / mulligan / choose cards to bottom).
- [x] Card component (text-based placeholder frame) with hover-to-enlarge.
- [x] Click to play cards; targeting UI; attack/block declaration UI. (Click only: dragging was left out, see `DECISIONS.md` #20.)
- [x] Arena-style auto-pass with hold-priority toggle.
- [x] Event-driven animations (basic: draw, cast, damage, die).
- [x] Game log panel.
- [x] Deck picker, win/lose screen, restart with new seed.

**Done when:** a full game can be played start to finish with no softlocks, and the owner has played 5+ games.

**Status (2026-10-11):** the features are built. Automated tests play whole games through the wasm handle, the controller and the real screen in a headless browser without ever being left without a move. The owner playtested it, found it fairly good and moved on to M3 the same day.

---

## M3 — One-act roguelike loop

- [x] Run state, seeded map generation (one act), node selection screen.
- [x] Node types: Duel, Elite, Boss, Rest, Shop (basic).
- [x] Hearts system + loss consequences; boons before boss.
- [x] Card rewards (pick 1 of 3), gold, card removal.
- [x] 2 planeswalkers with starting decks and passives.
- [x] ~5 relics using the modifier system.
- [x] Extend `data/balance.json` with all run-level numbers.
- [x] 4–6 enemy decks + 1 boss with emblems. (Seven normal decks in two tiers, two elites, one boss.)
- [x] Automatic basic-land management as the deck changes; non-basic lands as rewards and shop items.
- [x] Manual combat damage division where it matters.

**Done when:** a full act can be played start to finish, and win/loss feels fair.

**Status (2026-10-11):** the features are built; the milestone is waiting on the owner playing a full act. Automated checks: a run with the same seed and actions is identical, duels included; random walks through runs never find a state with nothing to do; Playwright clicks two whole acts through to a boss win in a headless browser.

Numbers from the sim, with the AI playing both sides (a person should do better than the AI does with the player's deck):

- `sim --runs --games 400`: the Warden wins the act in 37% of runs and the Firebrand in 44%. They reach the boss in 84% and 71% of runs, with 2.7 and 2.6 hearts, and beat it 44% and 62% of the time.
- `sim --enemies --games 400`, starting decks only: 61–93% against the three easy decks, 37–70% against the four normal ones, 24–61% against the elites, 16–27% against the boss before any boons or new cards.

Known limits, left for M4: the AI only ever double-blocks to survive a menace attacker, so the damage-division screen is rarely seen; the rare tier of the card pool is one card (Gigantosaurus); elites and the boss are harder for the Warden than for the Firebrand.

---
## M4 — Content expansion

Built in four parts, each ending in something to play:

- **M4a — rules and the first cube.** The Scryfall tool, then the rules the cube needs, a group at a time (keywords, target predicates, targeted triggers and small effects, more triggers and statics, tokens, +1/+1 counters, activated abilities, the graveyard), each with its cards, to about 114 spells. One act, played with the bigger pool.
- **M4b — better AI.** Rollouts on copies of the game; double-blocks; holding mana for instants. Shown with the sim, old against new.
- **M4c — three acts.** Acts, Draft, Event and Treasure nodes, upgrades, relics, bosses, new characters, a land collection with adjustable basics, consumables. Before it starts: play a commander prototype and decide whether characters become commanders.
- **M4d — the cube to 250–300.** Archetype packages, the rules they need (asked for one at a time), balance passes.

**Reordered on 2026-10-12 (`DECISIONS.md` #55, #56).** Commanders stay. The cube now comes before the rest of M4c, one character at a time, starting with a green-white commander built around creatures and a pool of overlapping archetype packages for it. The first act is made better with it (the new nodes, upgrades and so on, as they are decided); three acts are built last. Balance waits for the card pool.

**The green-white cube, slice 1 (2026-10-12): the Draft node.** A pack of 5 cards, keep up to 2 (`DECISIONS.md` #57). The map is 8 rows: every path passes a draft after the two easy duels, and the second row of rests and shops may hold another. The Warden starts with 12 spells. Checks: 507 engine tests, 12 wasm-crate tests, 29 sim tests, 69 web tests and 22 end-to-end tests pass (the end-to-end tests take about 66 s, 55 s before). `sim --runs --games 300`, +/- 6: the Warden wins the act in 61.3% of runs (55.0% before) and the Firebrand in 40.7% (39.7%); about 7 cards are added per run. That table took 191 s where it took 94 s before; not yet looked into. Next: the five packages (`DECISIONS.md` #58), a package at a time.

**Slice 2 (2026-10-12): the go-wide package.** Eleven cards and four tokens (`DECISIONS.md` #59), each with a test, and three small rules: an until-end-of-turn effect on all your creatures, an amount counted from your creatures, a static bonus for creature tokens only. The new cards are in no enemy or starting deck yet: they come from rewards, drafts and shops.

**Slice 3 (2026-10-12): the counters package.** Thirteen cards and a token (`DECISIONS.md` #60), each with a test. New rules: a counter on each of your creatures, doubling counters, "enters with N counters", triggers at the beginning of combat and of the end step and on a token entering, a keyword a creature has only while it has a counter, and a count of the counters a player has put on their creatures this turn. The pool is 145 spells, 15 lands and 13 tokens; 160 cards match Scryfall. Checks: 533 engine tests, 12 wasm-crate tests, 29 sim tests, 69 web tests and 22 end-to-end tests pass. `sim --runs --games 300`, +/- 6: the Warden wins the act in 59.7% of runs and the Firebrand in 40.3%, no errors, in 113 s (the 191 s seen after slice 1 was the machine being busy).

**Slice 4 (2026-10-12): the lifegain package and Conclave Mentor.** Eleven cards and three tokens (`DECISIONS.md` #61), each with a test. New rules: life gained this turn; a condition on a triggered ability, an activated ability or a keyword grant; "activate only as a sorcery"; gaining life equal to a creature's power or toughness, remembered if it has left; an end-step trigger in each player's turn; and the Mentor's extra counter. The pool is 156 spells, 15 lands and 16 tokens; 171 cards match Scryfall (one, Twinblade Paladin, with a text of our own). Checks: 547 engine tests, 12 wasm-crate tests, 29 sim tests, 69 web tests and 22 end-to-end tests pass. `sim --runs --games 300`, +/- 6: the Warden wins the act in 60.3% of runs and the Firebrand in 38.7%, no errors, in 129 s. Still to come: big creatures, and Auras and enchantments.

**Slice 5 (2026-10-12): the big-creatures package.** Ten cards and two tokens (`DECISIONS.md` #62), each with a test. New rules: a trigger on a creature of a given power entering, a "leaves the battlefield" trigger, an "if you control a creature with power 4 or greater" condition, "can't be blocked by creatures with power 2 or less", "can't be countered", a pump for creatures of a given toughness, and fetching a basic land from the library (the game picks it). The pool is 166 spells, 15 lands and 18 tokens; 181 cards match Scryfall. Checks: 562 engine tests, 12 wasm-crate tests, 29 sim tests, 69 web tests and 22 end-to-end tests pass. `sim --runs --games 300`, +/- 6: the Warden wins the act in 61.3% of runs and the Firebrand in 38.0%, no errors, in 143 s. After the owner's playtest of the pool (good): the starting deck may shrink to 8 to 10 spells, and the act is to get longer, with more kinds of node, perhaps with early enemies at 10 life. Still to come: Auras and enchantments.

**Slice 6 (2026-10-12): Auras and enchantments, and a 10-spell starting deck.** Twelve cards and a token (`DECISIONS.md` #63), each with a test. New rules: triggers on casting an enchantment, on an enchantment entering and on a permanent going to the graveyard from the battlefield (with the card returning to hand), and an anthem that counts enchantments. The five packages are in: 102 green-white spells to be offered, of 178 spells in all, with 15 lands and 19 tokens; 193 cards match Scryfall. The Warden starts with 10 spells and 7 lands, and a deck can be trimmed to 8. Adding cards is now a routine: `tools/add-cards.mjs` for a batch, `tools/check-all.ps1` for every check, and `.claude/skills/add-cards` for the steps. Checks: 571 engine tests, 12 wasm-crate tests, 29 sim tests, 69 web tests and 22 end-to-end tests pass. `sim --runs --games 300`, +/- 6: the Warden wins the act in 58.7% of runs and the Firebrand in 36.0%, no errors, in 137 s. Next: a Treasure node with new relics, an Event node, then a longer map.

**Slice 7 (2026-10-12): the Treasure node and seven relics.** A treasure offers two relics the player doesn't own, or 40 gold instead (`DECISIONS.md` #64); it has a weight on the fourth row of the map, so about half of maps show one (54% of 2000) and the act is no longer. Seven relics, twelve in all (`DECISIONS.md` #65): Rally Horn, Whetstone, Pilgrim's Flask, Beast Tamer's Whistle, Votive Candle, Surveyor's Map and Merchant's Ledger, each with a test in a duel or a shop of the run's. Two modifier effects are new: `starting_token` and `shop_discount`. Every token a relic, a boon or an emblem names is now checked when the data loads, which closes a limit noted under M4a. Checks: 587 engine tests, 12 wasm-crate tests, 29 sim tests, 70 web tests and 22 end-to-end tests pass; `cargo test` takes 68 s and the end-to-end tests 55 s, as before. The end-to-end acts moved to seeds 14 (Warden) and 40 (Firebrand), which pass a treasure and take a relic from it. `sim --runs --games 300`, +/- 6: the Warden wins the act in 65.7% of runs (58.7% before) and the Firebrand in 42.3% (36.0%), no errors, in 142 s; a run ends with 0.5 relics on average. Both gains are about the size of the margin, so "a little easier" is all that can be said, and the sim takes the first relic offered instead of choosing. Not exercised by anything but the tests and the sim until the owner plays it. Next: the owner chooses the events for an Event node, then the map is stretched to about 11 rows.

After the owner's M3 playtest (2026-10-11): rewards are locked to the character's colours and the win-margin gold bonuses are gone (`DECISIONS.md` #35, #36).

**M4a status (2026-10-12):** done. The owner played a whole run and found the mechanics fine.

- Rules added, each with its cards and tests: flash, defender, hexproof, double strike, indestructible, prowess; targets with conditions; triggered abilities with targets; effects that stop for a choice (discard, scry, surveil); bounce, tap, keywords until end of turn; triggers on dying, attacking, damaging an opponent, life gained, a spell cast, a sacrifice and an upkeep; anthems and Auras that change power and toughness; creature tokens; +1/+1 counters; activated abilities with mana, tap and sacrifice costs; targets in a graveyard, mill, and "you may" abilities.
- 129 cards (114 spells, 5 basic lands, 10 Guildgates) and 7 tokens. Every card matches Scryfall (`node tools/scryfall.mjs check`).
- Five more practice decks (D to H), one per group of new rules, for trying them in a practice duel.
- 17 enemy decks (5 easy, 7 normal, 4 elite, the boss) rebuilt on the pool, so that every spell is met in a run. Their tiers follow what the sim measured.
- The Firebrand's passive is now haste for the first creature each turn (`DECISIONS.md` #38).
- Checks: 487 Rust tests, 67 web tests and 21 end-to-end tests pass. The end-to-end tests click through games with each new deck and check that the new screens appear: aiming an ability, scrying, choosing a sacrifice, picking a card from a graveyard, declining a "you may".

Numbers from the sim, with the current AI on both sides (`--games 400`):

- `sim --runs`: the Warden wins the act in 38% of runs and the Firebrand in 38.5% (M3: 37% and 44%). They reach the boss in 91% and 80% of runs and beat it 42% and 48% of the time. Elites are won 29% of the time.
- `sim --enemies`, starting decks only: 66–95% against the easy decks, 35–90% against the normal ones, 15–47% against the elites, 16% and 25% against the boss.

Known limits, left for later parts:

- The act is below the 50–60% the owner agreed as the target for the AI's win rate; the boss is the gate. Balance is M4d, after the AI changes in M4b move every number.
- The AI still plays reactive cards badly: it almost never counters a spell, so Storm Scholars and Tide Weavers measure as easy. It never casts Raise Dead (a card for a card looks like no gain to it). M4b.
- Decks that mill themselves (Rot Farmers, practice deck H) often lose to fatigue in 25-card decks. Self-mill needs a payoff or bigger decks before it is an archetype; a question for M4d.
- The elites range widely (15–47%) and the normal decks more so; the tiers are a first sorting, not a balance pass.
- Blue cards and the gold cards of other colour pairs are only met in enemy decks until there are characters of those colours (M4c).
- An emblem's `create_token` is not checked against the token files when the data loads (no relic uses one yet). (Checked since the Treasure slice.)

**Commander prototype (2026-10-12):** built, to be played and judged before M4b (`DECISIONS.md` #37 is still open; the prototype's rules are #45 and #46).

- A character now has a commander: a legendary creature in its colours that waits in the command zone, beside the hand, and is cast from there. When it dies or is countered it goes back, and each cast from the command zone costs {2} more than the last (`duel.commander_tax` in `balance.json`). Bounced, it goes to hand and is cast from there at its printed cost. There is no commander damage.
- The Warden's is Jasmine Boreal ({3}{G}{W}, 4/5) and the Firebrand's is Boris Devilboon ({3}{B}{R}, 2/2, makes 1/1 Demons). The four elites and the boss have one too, shown in their panel from the first turn: Kongming (Town Militia), Elas il-Kor (Plague Court), Sir Shandlar of Eberyn (Grove Wardens), Tobias Andrion (Angelic Host) and The Lady of the Mountain (the Pyre Tyrant). Easy and normal enemies have none. None of the seven cards needed a new rule.
- The old passives are now starting emblems (`data/run/emblems.json`: Warm Welcome and Battle Fury), chosen on the menu after the commander. Either goes with either character; each character has a usual one, which the sim uses.
- To take commanders out again: delete the `commander` lines in `data/run/planeswalkers.json` and `enemies.json`. Nothing else depends on them.
- Checks: 508 Rust tests, 69 web tests and 22 end-to-end tests pass; 136 cards match Scryfall. The end-to-end tests pick an emblem on the menu, see the commander in the command zone, cast it by clicking, and see an elite's and the boss's commander. A commander dying and coming back is covered by the Rust tests only.

What the commanders did to the sim, with the v1 AI on both sides (`--games 400`, before → after):

- `sim --runs`: the Warden wins the act in 38.0% → 44.8% of runs, the Firebrand in 38.5% → 36.3%. Elites: the Warden wins 29% → 31% of them, the Firebrand 29% → 25%. The boss, for those who reach it: 42% → 46% and 48% → 45%.
- `sim --enemies`, starting decks: Jasmine Boreal, a 4/5 always on hand, lifts the Warden a great deal against the normal decks (Wild Hunt 35% → 66%, Primal Stampede 47% → 76%, Fire Giants 67% → 94%). Boris Devilboon changes little for the Firebrand (within about 8 points either way).
- The elites with a commander that does something got much harder: Plague Court with Elas il-Kor (Warden 30% → 12%, Firebrand 41% → 33%) and Town Militia with Kongming (Warden 39% → 25%, Firebrand 15% → 8%). The two with vanilla commanders got easier for the Warden, whose own commander matters more than theirs. The boss: Warden 16% → 24%, Firebrand 25% → 24%.
- So the two characters' commanders are far from equal in strength, and neither are the enemies'. That is left alone until the owner has decided whether commanders stay; balance is M4d.

**For M4b, the owner wants these measured and reported** (asked 2026-10-12):

1. How much better the new AI is than the old one: new against old on the same decks with seats swapped, mirror matches included.
2. Which decks become much better or much worse when both sides play the new AI, compared with both sides playing the old one.

Later idea, not for M4b: the AI gets more competent as the run goes on (weaker search for early enemies). For now every enemy plays at one strength and difficulty comes from decks, commanders and emblems.

**M4b status (2026-10-12):** built, to be played by the owner. How it works is in `ARCHITECTURE.md` §7; what was settled is `DECISIONS.md` #47 to #52.

- The game's opponent is now the searching AI ("new"), everywhere: runs, practice duels and the end-to-end tests' hints. v1 ("old") stays as the player inside its imagined games and as the baseline.
- What it does that v1 doesn't, each with a test on a position where the right play is known: double-blocks; doesn't attack into creatures that would gang up on it; divides damage to kill the better blocker; keeps mana up for a counterspell and uses it; keeps removal for something better than a token; casts Raise Dead.
- Checks: 499 engine tests, 12 wasm-crate tests, 26 sim tests, 69 web tests and 22 end-to-end tests pass; 136 cards match Scryfall. `cargo test` takes about 63 s (22 s before M4b; 35 s of it is the wasm crate's tests playing whole runs against the new AI in an unoptimised build). The end-to-end tests take 53 s (48 s before).

Report 1: how much better the new AI is. New against old on the same decks, each pairing both ways round, mirrors included (`sim --versus --games 100`, and `--versus --enemies --games 60`).

| Decks | Games | New wins | 95% margin |
|---|---|---|---|
| The eight test decks, every pairing | 7200 | 70.2% | +/- 1.1 |
| The two starting decks against the 17 enemy decks | 4080 | 70.8% | +/- 1.4 |

The same test deck, piloted by each AI against the other (900 games a deck, +/- 3):

| Deck | With the new AI | With the old AI | Gain |
|---|---|---|---|
| A (green-white creatures) | 78.7% | 36.3% | +42 |
| B (blue-black control) | 74.1% | 31.2% | +43 |
| C (red burn) | 64.4% | 21.4% | +43 |
| D (blue-black tempo) | 76.1% | 35.0% | +41 |
| E (white-black drain) | 75.2% | 32.0% | +43 |
| F (green-white tokens) | 78.8% | 47.9% | +31 |
| G (black-red sacrifice) | 65.4% | 21.7% | +44 |
| H (black-green graveyard) | 49.2% | 12.4% | +37 |

By slice, on the test decks (3600 games each, +/- 1.5): spells and abilities only 56.7%; with attacks and blocks 67.5%; with holding mana and card values 68.1%. Holding mana barely moves the win rate, so it was counted directly: over 30 games with deck B the old AI cast 18 counterspells and the new one 37.

Report 2: which decks change when both sides play the new AI. The starting decks' win rate against each enemy, both sides old (400 games a cell, +/- 5) and both sides new (150 games a cell, +/- 8), from `sim --enemies`.

| Enemy (tier after M4b) | Warden: old | new | change | Firebrand: old | new | change |
|---|---|---|---|---|---|---|
| Forest Pack (easy) | 98 | 100 | +2 | 80 | 95 | +15 |
| Grave Robbers (easy) | 99 | 100 | +1 | 74 | 93 | +19 |
| Rot Farmers (easy, was normal) | 98 | 99 | +2 | 78 | 91 | +14 |
| Sky Patrol (easy) | 89 | 95 | +6 | 77 | 89 | +11 |
| Storm Scholars (easy) | 97 | 95 | -2 | 89 | 88 | -1 |
| Goblin Raid (normal) | 95 | 98 | +3 | 55 | 83 | +28 |
| Border Legion (normal) | 95 | 98 | +3 | 63 | 80 | +17 |
| Fire Giants (normal) | 94 | 97 | +4 | 69 | 77 | +9 |
| Tide Weavers (normal, was easy) | 95 | 85 | -10 | 77 | 82 | +5 |
| Blood Cult (normal) | 94 | 89 | -4 | 57 | 61 | +4 |
| Wild Hunt (normal) | 66 | 81 | +15 | 52 | 44 | -8 |
| Primal Stampede (normal) | 76 | 59 | -17 | 63 | 52 | -11 |
| Grove Wardens (elite) | 39 | 49 | +10 | 42 | 15 | -27 |
| Angelic Host (elite) | 31 | 20 | -11 | 40 | 45 | +4 |
| Plague Court (elite) | 12 | 10 | -2 | 33 | 18 | -15 |
| Town Militia (elite) | 25 | 11 | -14 | 8 | 5 | -3 |
| The Pyre Tyrant (boss) | 24 | 25 | 0 | 24 | 15 | -9 |

- Much better for the player with both sides new: the Firebrand against the weak creature decks (Goblin Raid +28, Grave Robbers +19, Border Legion +17, Forest Pack +15, Rot Farmers +14), and the Warden against Wild Hunt (+15). The Firebrand's deck needs its attacks and removal played well, and v1 played them badly.
- Much worse for the player: the elites and the decks with a plan. Grove Wardens against the Firebrand (-27), Primal Stampede (-17 and -11), Plague Court against the Firebrand (-15), Town Militia against the Warden (-14), Angelic Host against the Warden (-11), Tide Weavers against the Warden (-10). Those decks reward good blocking and timing, which the enemy now has.
- The blue spell decks: Tide Weavers got harder for the Warden; Storm Scholars did not. Both are still beaten most of the time, so they are weak decks, not badly played ones. That is for M4d.
- Re-tiered (`DECISIONS.md` #52): Rot Farmers to easy, Tide Weavers to normal. Nothing else moved by more than the margin across its tier's edge.

Whole runs (`sim --runs`, after the re-tiering; old 400 runs, +/- 5; new 300 runs, +/- 6):

| | Warden: old | new | Firebrand: old | new |
|---|---|---|---|---|
| Won the act | 43.5% | 55.0% | 32.3% | 39.7% |
| Reached the boss | 96.8% | 97.3% | 78.5% | 87.3% |
| Beat the boss, of those | 45.0% | 56.5% | 41.1% | 45.4% |
| Normal duels won | 88.4% | 91.2% | 66.6% | 77.5% |
| Elite duels won | 30.5% | 25.5% | 24.0% | 20.4% |

So the act got easier for an AI-piloted player, more for the Warden, and the elites got harder. The Warden is inside the 50 to 60% the owner set as the target; the Firebrand is below it. Balance is M4d.

Knowing the opponent's decklist against guessing it, head to head: 50.1% +/- 1.5 over 4320 games on the test decks, 50.7% +/- 1.9 over 2720 on the run's decks. It is off (`DECISIONS.md` #49). Reports 1 and the per-slice numbers were measured with it on; report 2 and the runs with it off.

Timing, native, release build, 16 threads:

| | Old AI | New AI |
|---|---|---|
| Average time per decision | 0.03 ms | 0.75 ms |
| Slowest decision | 67 to 119 ms | 292 to 468 ms, on a crowded board around turn 33 |
| Thinking per side of a game | 6 to 7 ms | 190 to 250 ms |
| `sim --enemies`, per game on one core | 16 ms | 382 ms |
| `sim --enemies` | 400 games a cell in 18 s | 150 games a cell in 162 s |
| `sim --runs` | 400 runs each in 7 s | 300 runs each in 94 s |
| `sim --versus --games 100` (7200 games, new against old) | | 132 s |

Most decisions are forced passes, which is why the averages are so low. v1 itself is about four times faster than before M4b, with every move unchanged (the sim's v1 reports were compared byte for byte): before, `sim --enemies --games 400` took 67 s.

In WebAssembly (`node tools/ai-timing.mjs 6`: six whole runs, 31 duels, the new AI on both sides):

| | Count | Average | Median | 90% | 99% | Slowest |
|---|---|---|---|---|---|---|
| Every decision | 16386 | 0.5 ms | 0.0 ms | 0.8 ms | 9.8 ms | 49.5 ms |
| Decisions that take thought | 3194 | 2.5 ms | 1.0 ms | 7.3 ms | 19.0 ms | 49.5 ms |

That is far under the 0.1 to 0.3 s allowed per move. Thinking harder bought no strength (`DECISIONS.md` #51), so it stays quick. Not measured: the rare crowded late-game board in a browser; natively the slowest such decision was about 0.3 to 0.5 s, so a pause of up to a second is possible there.

What the new AI still plays badly:

- It looks one turn cycle ahead and no further, so it has no long plan: it doesn't race, set up a lethal turn two turns away, or hold a creature back as a blocker for later.
- It only finds plays that differ from v1's by one step, since v1 plays the rest of each imagined game. Two-step plays (bounce a blocker, then attack) are found only when v1 would make the second step.
- It keeps mana up only for instants and flash creatures in its hand, and only decides that in its own main phases. It doesn't bluff, and it acts only where a human is asked, so never in an upkeep or at the beginning of combat.
- It doesn't remember what it has seen: a card it scried to the top, or one bounced to the opponent's hand, is forgotten in its guesses.
- Mulligans, scrying and discards are still v1's rules of thumb.
- Its imagined opponent is v1, which never bluffs or holds mana on purpose, so it can walk into a counterspell or a trick more often than a person would.
- Self-mill decks (Rot Farmers, deck H) still deck themselves: it doesn't count its library.

**After the owner's M4b playtest (2026-10-12):** the new AI "seems a lot better". What he saw, and what was done:

- It cast Shock after attacking and before blocks, and Ambush Viper before attackers were declared, giving information away for nothing. Fixed: an instant or flash creature that does as well later is held back (`DECISIONS.md` #53), with a test for each case. New against old is unchanged by it: 67.8% +/- 1.5 over 3600 games.
- It used Sure Strike after blocks, which was right.
- A 2-power attacker blocked by a 4/5 and a 2/1 seemed to put all its damage on the 4/5. Not reproduced: in that plain shape the AI kills the 2/1, whichever was declared first, and a test pins that. Something else was different in the game played.
- It cast Titanic Growth on the owner's creature in his end step. Not understood, and not reproducible from memory. Open; a full hand about to discard is a guess.
- For cases like the last two there is now `sim --explain` (`DECISIONS.md` #54): press "Copy bug report" when the AI does something odd, save it to a file, and the sim replays the game and prints what the AI weighed. The duel screen of a run copies the whole run's report, which is what replays.

- [ ] Cube of ~100 cards → toward 250–300, built around the 10 archetypes. (M4a: 114 spells and all ten Guildgates, as rules vehicles and generic cards. The archetype packages and the rest are M4d.)
- [x] Scryfall import tool. (`tools/scryfall.mjs`: `add` and `check`.)
- [ ] Draft node (built), Treasure node (built), Event node, card upgrades.
- [ ] 3 acts, 3 bosses, ~20 relics (12 so far), 4+ planeswalkers.
- [x] Better AI (search/rollouts), able to play control decks properly. (M4b. It holds mana and counters spells; the blue decks are still weak decks.)
- [ ] Balance passes using `sim`. Carried over from M1: Deck A is a bit overtuned (78/22 against Deck C), and the Deck B and Deck C mirrors often run to fatigue.

## M5 — Polish and sharing

- [ ] Card art (where permitted), map art, sound, juicier animations.
- [ ] Save/resume runs (serialise RunState).
- [ ] Settings, tutorial hints.
- [ ] Deploy to Render / itch.io; share with friends; collect feedback.

## Later / maybe

- Meta progression, ascension levels.
- Separate land deck: lands in their own deck, choosing which deck to draw from each draw (see `GAME_DESIGN.md` §4.1).
- Tauri desktop build.
- Original card set replacing MTG cards (required before any commercial release).
