# Roadmap

Build in small milestones. Each has **acceptance criteria**; a milestone is done only when all are met and `cargo test` passes. Don't start the next milestone's features early.

**Active milestone: M1**

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

- [ ] Card data loader + validation for `data/cards/*.json`; decklist loader for `data/decks/*.json`.
- [ ] `data/balance.json` with the duel numbers (starting life, opening hand, max hand size, fatigue). Nothing hard-coded.
- [ ] Zones, objects, `ObjectId` + `CardId`, seeded RNG, serialisable `DuelState`.
- [ ] Full turn structure, priority, stack, state-based actions.
- [ ] Lands in the library, opening-hand smoothing, London mulligan, one land drop per turn, fatigue.
- [ ] Mana payment (coloured + generic) via the engine auto-payer.
- [ ] Combat incl. first strike, trample, deathtouch, lifelink, flying/reach, menace, vigilance, haste; automatic damage assignment.
- [ ] Spells, ETB triggers, simple activated mana abilities, auras (Pacifism), counterspells.
- [ ] `legal_actions`, `has_meaningful_action`, `apply_action`, `player_view`, events + redaction, `result`.
- [ ] Simple heuristic AI.
- [ ] `sim` CLI: run N AI-vs-AI games with seeds across all deck pairings; report win rates and any errors.
- [ ] Tests for every card and keyword; regression tests for bugs found.

Not in M1: manual mana tapping, manual damage division, and any rule in `RULES_SCOPE.md` that no starter card needs.

**Starter card set (real cards, simple rules):**

| Card | Cost | Notes / what it tests | Deck |
|---|---|---|---|
| Plains, Island, Swamp, Mountain, Forest | — | Land drops, mana | all |
| Savannah Lions | {W} | 2/1 vanilla | A |
| Healer's Hawk | {W} | 1/1 flying, lifelink | A |
| Youthful Knight | {1}{W} | 2/1 first strike | A |
| Inspiring Overseer | {2}{W} | 2/1 flying; ETB draw a card | A |
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

---

## M2 — Playable duel in the browser

Goal: a human can play a full duel against the AI in the browser, and it's fun. Default matchup: the human plays Deck B, the AI plays Deck A or C (the v1 AI can't pilot a reactive deck).

- [ ] Duel screen: battlefield, hands, library, life totals, stack, graveyard viewer.
- [ ] Mulligan screen (keep / mulligan / choose cards to bottom).
- [ ] Card component (text-based placeholder frame) with hover-to-enlarge.
- [ ] Click/drag to play cards; targeting UI; attack/block declaration UI.
- [ ] Arena-style auto-pass with hold-priority toggle.
- [ ] Event-driven animations (basic: draw, cast, damage, die).
- [ ] Game log panel.
- [ ] Deck picker, win/lose screen, restart with new seed.

**Done when:** a full game can be played start to finish with no softlocks, and the owner has played 5+ games.

---

## M3 — One-act roguelike loop

- [ ] Run state, seeded map generation (one act), node selection screen.
- [ ] Node types: Duel, Elite, Boss, Rest, Shop (basic).
- [ ] Hearts system + loss consequences; boons before boss.
- [ ] Card rewards (pick 1 of 3), gold, card removal.
- [ ] 2 planeswalkers with starting decks and passives.
- [ ] ~5 relics using the modifier system.
- [ ] Extend `data/balance.json` with all run-level numbers.
- [ ] 4–6 enemy decks + 1 boss with emblems.
- [ ] Automatic basic-land management as the deck changes; non-basic lands as rewards and shop items.
- [ ] Manual combat damage division where it matters.

**Done when:** a full act can be played start to finish, and win/loss feels fair.

---

## M4 — Content expansion

- [ ] Cube of ~100 cards → toward 250–300, built around the 10 archetypes.
- [ ] Scryfall import tool.
- [ ] Draft node, Event node, Treasure node, card upgrades.
- [ ] 3 acts, 3 bosses, ~20 relics, 4+ planeswalkers.
- [ ] Better AI (search/rollouts), able to play control decks properly.
- [ ] Balance passes using `sim`.

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
