# CLAUDE.md

Guidance for AI assistants working in this repository. Read this first, every session.

## What this project is

A **Magic: The Gathering roguelike**: real MTG duels (stack, instants, combat, keywords) wrapped in a Slay the Spire–style run. You follow a branching map, win duels, pick cards and relics, and build a deck toward an archetype across three acts.

Current status: **early prototype**: one act, one character (the Warden). See `docs/ROADMAP.md` for the active milestone and the backlog. Only build what the task in hand needs.

Priorities: reach a playable version quickly, without closing off a maintainable, scalable, feature-rich game later. When the two pull apart, make the cheap structural choice now and defer the feature.

## Where things are documented

| Doc | Read it when |
|---|---|
| `docs/GAME_DESIGN.md` | You need the long-term vision. Most of it is NOT in scope for the prototype. |
| `docs/ROADMAP.md` | Deciding what to build next. It holds the backlog: one brief per task, each meant for a session of its own. |
| `docs/HISTORY.md` | You need what an earlier milestone or slice built, or a sim number measured then. A finished slice's note goes here. |
| `docs/BALANCE_REPORT.md` | Touching the Warden's pool, the enemies or the relics: what was measured, tried and left open. |
| `docs/ARCHITECTURE.md` | Touching code structure, the engine API, or the card data format. |
| `docs/RULES_SCOPE.md` | Implementing any MTG rule. Lists what's in, simplified, and out. |
| `docs/TECH_STACK.md` | Setting up tools, building, running, deploying. |
| `docs/DECISIONS.md` | Before proposing to change a major choice. Decisions there are settled unless the user reopens them. |

## Architecture in one paragraph

A **pure Rust rules engine** (`crates/engine`) holds all game logic and knows nothing about rendering. It is compiled to **WebAssembly** via a thin bindings crate (`crates/engine-wasm`). A **TypeScript + React** front end (`web/`) renders state and sends player actions. A headless **simulation CLI** (`crates/sim`) runs AI-vs-AI games for testing and balance. The engine has two layers of the same shape: the **duel** (`crates/engine/src/duel`) and the **run** around it (`crates/engine/src/run`), which owns the duel in progress. Cards are **data** (`data/cards/`, with tokens in `data/tokens/`), not code, and so is the run's content (`data/run/`).

## Commands

```bash
# Rust
cargo test                      # all engine tests (run before saying a task is done);
                                # also regenerates the ts-rs types in web/src/engine/types/
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo run -p sim -- --games 100 # headless AI-vs-AI simulation: 100 games for every pairing
                                # of the decks in data/decks, then a report (--seed S to vary)
cargo run -p sim -- --replay deck_a deck_c --seed 7   # one game, move by move
cargo run --release -p sim -- --enemies --games 400   # the run's starting decks against its enemy decks
cargo run --release -p sim -- --runs --games 400      # whole runs: how often the act is won, and how often
                                                      # each feat's tiers are met by an AI that isn't trying
cargo run --release -p sim -- --versus --games 100    # the new AI against the old on the same decks, both ways
                                                      # round (--ai, --against); add --enemies for the run's decks
cargo run --release -p sim -- --versus --games 50 --decks deck_a,deck_b,deck_c   # a quick sample: only these decks
cargo run --release -p sim -- --explain report.json --last 3   # replays a bug report copied from the game and says
                                # what the AI weighed in its last 3 decisions (Get-Clipboard | Set-Content report.json)
                                # --ai old|new picks who plays in any mode (new, the searching AI, is the default);
                                # every sim report ends with how long the AIs took to decide

# The balance lab (the Warden only; docs/BALANCE_REPORT.md says what each was used for)
cargo run --release -p sim -- --lift --games 12000 --weighted --relics 1   # single duels of random decks: each card's
                                # and relic's lift (win rate of the decks with it less that of those without)
cargo run --release -p sim -- --ideal --games 100           # the ideal decks of data/sim/ideal_decks.json against each enemy
cargo run --release -p sim -- --leave-out counters --games 40   # one ideal deck with each card in turn swapped for a filler
cargo run --release -p sim -- --act --games 300             # each ideal deck through whole acts, taking no cards or relics
cargo run --release -p sim -- --runs --games 600 --decks warden --no-relics   # whole runs of one character; also
                                # --no-cards, --with-relic ID, --with-card ID
                                # any mode: --field easy,normal,hard,elite,boss or enemy ids (the lab's enemies),
                                # --data DIR (read data/ from a folder at run time, no rebuild),
                                # --threads N (default: three quarters of the cores)

# Card data (from repo root; needs the network the first time, then uses its cache)
node tools/scryfall.mjs add "Card Name"   # writes data/cards/<id>.json from Scryfall, abilities left to write by hand
node tools/scryfall.mjs check             # every card's name, cost, types, stats, keywords and text against Scryfall
                                          # (a card marked "altered" keeps its own text)
node --test tools/scryfall.test.mjs       # the tool's own tests
node tools/add-cards.mjs spec.json        # a batch of cards: Scryfall's facts, the spec's abilities, and tokens
                                          # (.claude/skills/add-cards/SKILL.md has the whole routine)

# Every check before a commit (from repo root, Windows PowerShell)
.\tools\check-all.ps1                    # fmt, clippy, all tests, Scryfall, wasm, lint, build, end-to-end
.\tools\check-all.ps1 -Quick             # without the end-to-end tests
.\tools\check-all.ps1 -Sim               # and then 300 whole runs in the sim

# AI speed in WebAssembly (from repo root, after npm run wasm in web/)
node tools/ai-timing.mjs 4                # plays 4 whole runs through the wasm handle and times every AI decision

# WASM build (from repo root)
wasm-pack build crates/engine-wasm --target web --out-dir ../../web/src/wasm

# Web (run these from web/)
npm install
npm run wasm                    # same wasm-pack build as above; needed before dev, build and test
npm run dev                     # local dev server
npm run build                   # production build to web/dist
npm test                        # Vitest
npm run e2e                     # Playwright: clicks through whole games and whole acts in a headless browser
                                # (once: npx playwright install chromium)
npm run lint                    # oxlint
```

`web/src/wasm/` is gitignored, so on a fresh clone build the wasm before anything else in `web/`.

The owner works in Windows PowerShell 5.1, where `&&` is not available. Give commands one per line.

Update this section if commands change.

## Rules for working in this repo

1. **All game logic lives in `crates/engine`.** The UI never decides what is legal; it asks the engine (`legal_actions`, `run_legal_actions`) and sends actions. That goes for the run too: what a node gives, what a reward holds, what something costs. No rules logic in TypeScript, and that includes mana payment and deciding when to auto-pass.
2. **The engine is deterministic.** All randomness goes through the seeded RNG in game state. Never use `thread_rng`, system time, or HashMap iteration order for anything gameplay-relevant (use `BTreeMap`/`IndexMap` or sort).
3. **Cards are data.** New cards are added to `data/cards/*.json` with `tools/scryfall.mjs add`, then given abilities using existing effect primitives; `tools/scryfall.mjs check` must pass. Only add a new effect primitive when a card genuinely can't be expressed; add a test for the primitive. A card the Warden can be offered also needs an upgraded side and its row in `upgrade_tests.rs`. `Custom` effects implemented in Rust are a last resort and must be listed in `docs/ARCHITECTURE.md`.
4. **Tests are mandatory for rules.** Every card, keyword, and rules interaction gets a test. Every bug fixed gets a regression test that fails before the fix. Run `cargo test` before declaring work done.
5. **No magic numbers for balance.** Life totals, opening hand sizes, fatigue, wounds, rewards, what feats pay, prices, map shape, land share, etc. live in `data/balance.json`, never hard-coded. Content keeps its own numbers beside its text, as cards do: relics, wounds, feats, passives and enemy emblems in `data/run/`.
6. **Stay inside `docs/RULES_SCOPE.md`.** If a card needs a rule marked "out", don't add the card. Ask instead of expanding scope.
7. **Errors, not panics.** Engine functions return `Result`. Illegal actions are rejected with a clear error, never a panic. No `unwrap()` in engine code outside tests.
8. **Small, focused changes.** Prefer several small commits over one large one. Don't refactor unrelated code while doing a task.
9. **Keep docs current.** If you change architecture, commands, or a settled decision (with the user's agreement), update the relevant doc in the same change.
10. **Ask when the design is ambiguous.** Game design choices belong to the user. Implementation details are yours to decide; note notable ones in your summary.

## Code style

- Rust: stable toolchain, `cargo fmt`, clippy clean. Prefer enums and exhaustive `match` over strings and booleans. Use `serde` for anything crossing the WASM boundary or loaded from data.
- TypeScript: `strict: true`. No `any`. Types for engine state are generated from the Rust types with `ts-rs` into `web/src/engine/types/`; never write or edit them by hand.
- Names: use MTG terminology (`battlefield`, `graveyard`, `stack`, `priority`, `permanent`) so code matches the rules.

## Glossary (project-specific terms)

- **Run**: one full playthrough, three acts.
- **Act**: one map ending in a boss.
- **Wound**: run-level health, separate from in-duel life (20 by default). A lost duel leaves one; it follows the player into every duel until a rest site heals it, and the third ends the run. It replaced hearts (`DECISIONS.md` #68).
- **Feat**: something to do in a duel beyond winning it ("never fall below 12 / 15 / 18 life"). Two are picked before each duel; their points set the card reward (`DECISIONS.md` #69).
- **Smoothing**: the engine deals several candidate opening hands and keeps the one closest to the deck's land ratio. Lands are in the library, as in normal Magic.
- **Fatigue**: drawing from an empty library costs life (1, then 2, then 3...) instead of losing the game.
- **Upgraded side**: what a card becomes when a rest site upgrades a copy of it: a card of its own, written in the card's file under `"upgraded"` and named with a "+" ("Raise the Alarm+", id `raise_the_alarm+`). Every card the Warden can hold has one (`DECISIONS.md` #72).
- **Relic**: passive item that modifies rules for the whole run.
- **Modifier**: what relics, wounds, passives and boss emblems all are in the code (`ModifierDef`). It reaches a duel through `PlayerConfig`, often as an **emblem**.
- **Commander**: a legendary creature in the deck's colours that waits in the command zone and is cast from there, costing more each time (a prototype; `DECISIONS.md` #45). Characters, elites and the boss have one.
- **Starting emblem**: the modifier the player picks with their commander when a run starts (`data/run/emblems.json`). It used to be the character's fixed passive.
- **Planeswalker (character)**: the player's chosen starting identity (starting deck, colours, passive). Not the MTG card type.
