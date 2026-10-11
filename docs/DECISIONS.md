# Decision Log

Settled decisions and why. Don't relitigate these without the owner reopening them. Add new entries at the bottom with a date.

| # | Date | Decision | Why | Alternatives considered |
|---|---|---|---|---|
| 1 | 2026-10-11 | Build from scratch, not decompiled Magic 2014 / Slay the Spire, not a Forge fork. | Clean, maintainable codebase; limited card pool; nice UI; path to selling. Decomp is legally fragile and hard to build on. Forge's pool is too big to balance. | Decomp merge; Forge + roguelike layer; Slay the Spire mod. |
| 2 | 2026-10-11 | Rust rules engine compiled to WASM; TypeScript + React UI. | Rust's strictness catches AI mistakes in the hardest code; owner knows Rust; web UI is ideal for card games and easy to share. | Bevy (weak UI), Unity/C#, Godot, all-TypeScript. |
| 3 | 2026-10-11 | Browser first; Tauri desktop later if commercial. | Easy sharing with friends; same code becomes a small native app. | Native-only executable. |
| 4 | 2026-10-11 | Engine is pure and deterministic (seeded RNG). | Testable, replayable, enables AI search and simulation. | — |
| 5 | 2026-10-11 | Cards are data built from effect primitives. | Scales to hundreds of cards safely; easy swap to original cards later. | Per-card code. |
| 6 | 2026-10-11 | Curated cube of ~250–300 recent cards around 10 two-colour archetypes. | Manageable balance; clear archetypes. | Full Forge pool; single set. |
| 7 | 2026-10-11 | In-duel life (20) and run health (hearts) are separate. | Archetypes like lifegain / pay-life play as intended; variance doesn't end runs. | Life carries over between duels (Slay the Spire style). |
| 8 | 2026-10-11 | Hearts: 3 per act, refill each act; lose 1 per lost duel/elite; 0 = run over; boss loss = run over; remaining hearts buy pre-boss boons. | Losses matter without one bad draw ending a run; each act builds toward the boss. | Fixed per-heart bonus; hearts as boss retries. |
| 9 | 2026-10-11 | **Superseded by #12.** Lands in a separate land pile, one drop per turn. | No mana screw/flood ending runs; no land-ratio busywork. Colour fixing still matters. | Lands in deck + Arena-style hand smoothing (fallback). |
| 10 | 2026-10-11 | Real MTG priority/stack in engine; Arena-style auto-pass in UI. | Keeps Magic's depth without click fatigue. | Simplified no-stack rules. |
| 11 | 2026-10-11 | Real MTG cards for a free prototype only; replace with originals before any sale. | IP. | — |
| 12 | 2026-10-11 | Lands are in the library, as in normal Magic. 7-card opening hand with smoothing (deal several candidate hands, keep the one closest to the deck's land ratio) plus London mulligan. In a run, the game manages basic lands automatically and non-basic lands are cards the player adds. | Every real card works as printed; Magic intuition and card balance carry over; interesting lands become real deck-building choices; no special-case rules. | Land pile with free pick each turn (#9); separate land deck with a choice of which deck to draw from (kept as a future idea: removes screw and flood entirely, but scry, card draw, explore, landfall and mill all change in power or meaning). |
| 13 | 2026-10-11 | Fatigue replaces losing on an empty library: drawing from an empty library costs 1 life, then 2, then 3... | Small decks can run out in a long game; fatigue is a clock, not a cliff. | Normal Magic loss on an empty library. |
| 14 | 2026-10-11 | Opponents follow exactly the same deck, smoothing and mulligan rules as the player. | Symmetry keeps the engine simple and the game fair. | Separate enemy rule (guaranteed 2 lands in hand). |
| 15 | 2026-10-11 | Starting player is a seeded coin flip. | Simple; revisit if it feels bad in a run. | Player always first. |
| 16 | 2026-10-11 | Combat damage follows current Magic rules (no damage assignment order); the engine assigns automatically for now. | The real rule is already the simple one; auto-assignment gets to playable sooner. | Pre-2024 ordering step; manual division from the start. |
| 17 | 2026-10-11 | JSON strings across the wasm boundary; TypeScript types generated with `ts-rs`; duel state held inside wasm. | Debuggable, no hand-mirrored types to drift, no state round-trips. | `serde-wasm-bindgen`; hand-written types. |
| 18 | 2026-10-11 | `sim --games N` plays N games for every pairing of the decks, mirror matches included. Turn cap of 100 turns. It measures land screw (2 or fewer lands at the end of a player's turn 4), flood (60% or more of drawn cards are lands at the end of turn 6) and unspent lands per turn. Thresholds live in `balance.json`. | Steadier win rates; a mirror should be 50/50 between seats, so it shows seat-dependent bugs; screw and flood check that smoothing and the land ratio work. | N games in total; cross pairings only. |
