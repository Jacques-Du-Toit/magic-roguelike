# CLAUDE.md

Guidance for AI assistants working in this repository. Read this first, every session.

## What this project is

A **Magic: The Gathering roguelike**: real MTG duels (stack, instants, combat, keywords) wrapped in a Slay the Spire–style run. You follow a branching map, win duels, pick cards and relics, and build a deck toward an archetype across three acts.

Current status: **early prototype**. See `docs/ROADMAP.md` for the active milestone. Only build what the active milestone needs.

Priorities: reach a playable version quickly, without closing off a maintainable, scalable, feature-rich game later. When the two pull apart, make the cheap structural choice now and defer the feature.

## Where things are documented

| Doc | Read it when |
|---|---|
| `docs/GAME_DESIGN.md` | You need the long-term vision. Most of it is NOT in scope for the prototype. |
| `docs/ROADMAP.md` | Deciding what to build next. Milestones have acceptance criteria. |
| `docs/ARCHITECTURE.md` | Touching code structure, the engine API, or the card data format. |
| `docs/RULES_SCOPE.md` | Implementing any MTG rule. Lists what's in, simplified, and out. |
| `docs/TECH_STACK.md` | Setting up tools, building, running, deploying. |
| `docs/DECISIONS.md` | Before proposing to change a major choice. Decisions there are settled unless the user reopens them. |

## Architecture in one paragraph

A **pure Rust rules engine** (`crates/engine`) holds all game logic and knows nothing about rendering. It is compiled to **WebAssembly** via a thin bindings crate (`crates/engine-wasm`). A **TypeScript + React** front end (`web/`) renders state and sends player actions. A headless **simulation CLI** (`crates/sim`) runs AI-vs-AI games for testing and balance. The engine has two layers of the same shape: the **duel** (`crates/engine/src/duel`) and the **run** around it (`crates/engine/src/run`), which owns the duel in progress. Cards are **data** (`data/cards/`), not code, and so is the run's content (`data/run/`).

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
cargo run --release -p sim -- --runs --games 400      # whole runs: how often the act is won

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
3. **Cards are data.** New cards are added to `data/cards/*.json` using existing effect primitives. Only add a new effect primitive when a card genuinely can't be expressed; add a test for the primitive. `Custom` effects implemented in Rust are a last resort and must be listed in `docs/ARCHITECTURE.md`.
4. **Tests are mandatory for rules.** Every card, keyword, and rules interaction gets a test. Every bug fixed gets a regression test that fails before the fix. Run `cargo test` before declaring work done.
5. **No magic numbers for balance.** Life totals, opening hand sizes, fatigue, hearts, rewards, prices, boon values, map shape, land share, etc. live in `data/balance.json`, never hard-coded. Content keeps its own numbers beside its text, as cards do: relics, passives and enemy emblems in `data/run/`.
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
- **Hearts**: run-level health. Lost by losing duels; separate from in-duel life (20 by default).
- **Smoothing**: the engine deals several candidate opening hands and keeps the one closest to the deck's land ratio. Lands are in the library, as in normal Magic.
- **Fatigue**: drawing from an empty library costs life (1, then 2, then 3...) instead of losing the game.
- **Relic**: passive item that modifies rules for the whole run.
- **Modifier**: what relics, boons, passives and boss emblems all are in the code (`ModifierDef`). It reaches a duel through `PlayerConfig`, often as an **emblem**.
- **Boon**: pre-boss bonus bought with remaining hearts.
- **Planeswalker (character)**: the player's chosen starting identity (starting deck, colours, passive). Not the MTG card type.
