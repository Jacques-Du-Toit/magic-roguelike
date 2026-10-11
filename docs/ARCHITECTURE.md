# Architecture

## 1. Big picture

```
 ┌──────────────── web/ (TypeScript + React) ────────────────┐
 │  Screens: Map · Duel · Rewards · Shop · Deck view          │
 │  Renders PlayerView, animates Events, sends Actions        │
 └───────────────▲───────────────────────┬────────────────────┘
                 │ PlayerView, Events    │ Action
 ┌───────────────┴──── crates/engine-wasm ▼───────────────────┐
 │  Thin wasm-bindgen wrapper. Serialises in/out. No logic.   │
 └───────────────▲───────────────────────┬────────────────────┘
 ┌───────────────┴──── crates/engine (pure Rust) ▼────────────┐
 │  duel/   rules: turns, stack, combat, SBAs, effects        │
 │  run/    roguelike: map, hearts, rewards, relics, shop      │
 │  cards/  card definitions loaded from data                  │
 │  ai/     opponent decision-making                           │
 └─────────────────────────────────────────────────────────────┘
        ▲
 crates/sim — headless CLI using engine directly (AI vs AI)
```

**Golden rule:** the engine is the single source of truth. The UI never decides legality or outcomes.

## 2. Engine API (shape, not final)

```rust
// Duel level
pub fn new_duel(config: DuelConfig, seed: u64) -> Result<DuelState, ConfigError>;
pub fn legal_actions(state: &DuelState, player: PlayerId) -> Vec<Action>;
pub fn has_meaningful_action(state: &DuelState, player: PlayerId) -> bool;
pub fn apply_action(state: &mut DuelState, player: PlayerId, action: Action)
    -> Result<Vec<GameEvent>, ActionError>;
pub fn player_view(state: &DuelState, player: PlayerId) -> PlayerView; // hides opponent's hand/library
pub fn redact_events(events: &[GameEvent], viewer: PlayerId) -> Vec<GameEvent>;
pub fn result(state: &DuelState) -> Option<GameResult>; // Win { player, reason } | Draw
pub fn ai_choose_action(state: &DuelState, player: PlayerId) -> Action;

// Run level (later milestone)
pub fn new_run(config: RunConfig, seed: u64) -> RunState;
pub fn run_legal_actions(state: &RunState) -> Vec<RunAction>;
pub fn apply_run_action(state: &mut RunState, action: RunAction)
    -> Result<Vec<RunEvent>, ActionError>;
```

### Action model

Every `Action` returned by `legal_actions` is concrete and can be passed straight to `apply_action`. The AI and the UI both work only from this list. Two rules keep the list small:

- **Spells and abilities are fully specified.** `CastSpell { card, targets, mode }` and `ActivateAbility { source, ability, targets }` appear once per legal target combination. There is no separate "choose target" step. (X costs, when added, will be an exception: one action carrying the legal X range.)
- **Anything combinatorial is a stepwise pending decision.** The state records "waiting on player P for decision D", and `legal_actions` returns only the next step:

| Decision | Step actions |
|---|---|
| Declare attackers | `Attack { creature }`, `ResetDeclaration`, `ConfirmAttackers` |
| Declare blockers | `Block { blocker, attacker }`, `ResetDeclaration`, `ConfirmBlockers` |
| Mulligan | `Keep`, `Mulligan`, then `BottomCard { card }` once per card owed |
| Discard to hand size | `Discard { card }` |
| Later: order triggers, divide combat damage, "may" choices | one step action each |

A `Confirm…` action is only legal when the declaration so far is legal (e.g. `ConfirmBlockers` is absent while a menace attacker has exactly one blocker).

Other actions: `PlayLand { card }`, `PassPriority`.

### Mana payment

Payment is engine logic, not UI logic, because the AI and `sim` need it too.

- `CastSpell` / `ActivateAbility` is legal only if the engine's **auto-payer** can find a payment. Applying the action taps sources and pays in one go.
- The auto-payer is deterministic: lands before creatures, and among lands it prefers to leave untapped the colours the rest of the hand needs.
- Manual tapping (activating mana abilities one by one into the mana pool) is a later addition. Model the mana pool properly now so that it slots in.

### Auto-pass

Mana abilities are always available, so "the only legal action is pass" is never literally true. `has_meaningful_action` answers the real question: is there any legal action other than `PassPriority` and mana abilities? The UI auto-passes when it returns false (unless the player is holding priority). The rule lives in the engine so it can be refined (stops, "only on my turn") without touching TypeScript.

### Events

- **Events** describe what happened (`CardDrawn`, `SpellCast`, `DamageDealt`, `CreatureDied`, `LifeChanged`, `TokenCreated`, `GameOver` ...). The UI animates events in order rather than diffing state.
- `apply_action` returns unredacted events (the sim and tests want everything). `redact_events` strips hidden information for one viewer, e.g. the identity of a card the opponent drew. The wasm wrapper only ever returns redacted events.

### Configuration

`DuelConfig` is per-player from the start, because boons, relics, bosses and planeswalker passives all need asymmetric setups:

```rust
pub struct DuelConfig { pub players: [PlayerConfig; 2], pub rules: RulesConfig }
pub struct PlayerConfig {
    pub library: Vec<CardInstance>,     // lands and spells together
    pub starting_life: i32,
    pub opening_hand_size: u8,
    pub starting_permanents: Vec<CardInstance>, // unused until M3
    pub emblems: Vec<EmblemDef>,                // unused until M3
}
```

`RulesConfig` holds duel-wide numbers (max hand size, opening-hand smoothing, fatigue progression). Defaults for all of these come from `data/balance.json`; nothing is hard-coded.

A **`CardInstance`** is `{ card_id, upgrades }`, not a bare card id. `upgrades` is empty until card upgrades exist (M4), but decklists, rewards and saves are built on instances from day one.

## 3. Determinism

- One seeded RNG (`ChaCha8Rng`) lives in the state and is the only randomness source.
- Same seed + same action sequence = identical game. This enables replays, bug reports ("seed 1234, these actions"), and reliable tests.
- No `HashMap` iteration in gameplay paths; use ordered collections.
- Two kinds of id:
  - **`ObjectId(u32)`**, assigned in creation order. A card changing zones becomes a new object (per MTG rules), so rules references ("target creature") correctly stop pointing at it.
  - **`CardId(u32)`**, one per physical card or token for the whole duel. It never changes. Events and `PlayerView` carry both, so the UI can animate one card moving between zones.
- `DuelState` and everything in it derive `Serialize`/`Deserialize` from the start. Card definitions are referenced by id, never embedded in state. This keeps save/resume and replays cheap to add.
- The starting player is chosen by the seeded RNG.

## 4. Card data format

Cards are JSON in `data/cards/`, one file per card, embedded at compile time with the `include_dir` crate (so the same loader works natively and in wasm; the engine does no file IO). Each card = Scryfall-derived fields + our ability definitions built from **effect primitives**.

Fields: `id`, `name`, `mana_cost`, `colors`, `supertypes`, `types`, `subtypes`, `power`/`toughness` (creatures), `rarity`, `keywords`, `oracle_text`, `enchant` (Auras), `abilities`. Omitted optional fields default to empty. The examples below cover every shape the M1 starter set needs.

Instant with a target:

```json
{
  "id": "lightning_strike",
  "name": "Lightning Strike",
  "mana_cost": "{1}{R}",
  "colors": ["R"],
  "types": ["Instant"],
  "rarity": "common",
  "oracle_text": "Lightning Strike deals 3 damage to any target.",
  "abilities": [
    {
      "kind": "spell",
      "effects": [
        { "type": "deal_damage", "amount": 3, "target": { "any_target": {} } }
      ]
    }
  ]
}
```

Creature with keywords only:

```json
{
  "id": "vampire_nighthawk",
  "name": "Vampire Nighthawk",
  "mana_cost": "{1}{B}{B}",
  "colors": ["B"],
  "types": ["Creature"],
  "subtypes": ["Vampire", "Shaman"],
  "power": 2, "toughness": 3,
  "rarity": "uncommon",
  "keywords": ["flying", "deathtouch", "lifelink"],
  "oracle_text": "Flying, deathtouch, lifelink",
  "abilities": []
}
```

Basic land (the mana ability is explicit data, not implied by the subtype):

```json
{
  "id": "forest",
  "name": "Forest",
  "mana_cost": "",
  "supertypes": ["Basic"],
  "types": ["Land"],
  "subtypes": ["Forest"],
  "rarity": "common",
  "oracle_text": "({T}: Add {G}.)",
  "abilities": [
    { "kind": "mana", "cost": { "tap": true }, "produces": "{G}" }
  ]
}
```

Mana creature (Llanowar Elves) uses the same `mana` ability on a creature; summoning sickness applies because the cost includes `tap`.

Enters-the-battlefield trigger:

```json
{
  "id": "elvish_visionary",
  "name": "Elvish Visionary",
  "mana_cost": "{1}{G}",
  "colors": ["G"],
  "types": ["Creature"],
  "subtypes": ["Elf", "Shaman"],
  "power": 1, "toughness": 1,
  "rarity": "common",
  "oracle_text": "When this creature enters, draw a card.",
  "abilities": [
    {
      "kind": "triggered",
      "trigger": { "type": "enters_battlefield", "subject": "self" },
      "effects": [ { "type": "draw_cards", "player": "controller", "amount": 1 } ]
    }
  ]
}
```

Aura with a restriction:

```json
{
  "id": "pacifism",
  "name": "Pacifism",
  "mana_cost": "{1}{W}",
  "colors": ["W"],
  "types": ["Enchantment"],
  "subtypes": ["Aura"],
  "rarity": "common",
  "oracle_text": "Enchant creature\nEnchanted creature can't attack or block.",
  "enchant": { "creature": {} },
  "abilities": [
    { "kind": "static", "type": "restriction", "affects": "enchanted", "cant": ["attack", "block"] }
  ]
}
```

Counterspell:

```json
{
  "id": "cancel",
  "name": "Cancel",
  "mana_cost": "{1}{U}{U}",
  "colors": ["U"],
  "types": ["Instant"],
  "rarity": "common",
  "oracle_text": "Counter target spell.",
  "abilities": [
    { "kind": "spell", "effects": [ { "type": "counter_spell", "target": { "spell": {} } } ] }
  ]
}
```

### Ability kinds
- `spell` — effects when an instant/sorcery resolves.
- `triggered` — `{ "trigger": {...}, "effects": [...] }` e.g. `enters_battlefield`, `dies`, `attacks`, `beginning_of_upkeep`, `cast_spell { filter }`.
- `activated` — `{ "cost": {...}, "effects": [...] }` costs: mana, tap, sacrifice, pay life.
- `static` — continuous effects, e.g. `{ "type": "anthem", "filter": ..., "power": 1, "toughness": 1 }` or `{ "type": "restriction", "affects": ..., "cant": [...] }`.
- `mana` — mana abilities (don't use the stack).

### Effect primitives (initial set; grow carefully)
`deal_damage`, `gain_life`, `lose_life`, `draw_cards`, `discard`, `destroy`, `exile`, `counter_spell`, `create_token`, `add_counters`, `modify_pt_until_eot`, `grant_keyword_until_eot`, `tap`, `untap`, `return_to_hand`, `add_mana`, `scry`, `search_library` (if needed), `mill`, `sacrifice`.

Targets and filters are structured data (`creature`, `creature_you_control`, `any_target`, `player`, `spell`, with predicates like `power_lte`, `has_keyword`, `color`).

### Custom escape hatch
If a card truly can't be expressed, it may reference `{ "type": "custom", "id": "card_specific_fn" }` implemented in Rust. Keep a list here:

| Custom id | Card | Why |
|---|---|---|
| _(none yet)_ | | |

### Validation
On load, every card is validated (costs parse, targets exist, keywords known, Auras have `enchant`). A test loads all card files and fails on any error.

### Decklists
`data/decks/*.json`: `{ "id", "name", "cards": [{ "card": "grizzly_bears", "count": 2 }, { "card": "forest", "count": 5 }] }`. Lands and spells share one list. Validated on load against the card database.

## 5. Duel internals

- **Zones**: library, hand, battlefield, graveyard, exile, stack, command (emblems). Lands are in the library, as in normal Magic.
- **Opening-hand smoothing**: at the start of the duel the engine shuffles and deals `smoothing_candidates` opening hands (default 3) using the seeded RNG, and keeps the one whose land count is closest to the deck's land ratio × hand size (ties: the first dealt). The rest of the library is then shuffled normally. Setting the number to 1 turns smoothing off. Mulligan hands are smoothed the same way.
- **Mulligan**: London mulligan. Draw a fresh hand, then put one card on the bottom per mulligan taken.
- **Fatigue**: if a player would draw from an empty library, they instead lose life: 1 the first time, 2 the second, and so on (per-player counter). There is no lose-on-empty-library rule.
- **Turn structure**: full MTG steps (see `RULES_SCOPE.md`).
- **Priority**: real priority passing; the *UI* auto-passes for the human when `has_meaningful_action` is false (Arena-style), configurable.
- **Stack**: spells and abilities as stack objects; resolves LIFO when all players pass in succession.
- **State-based actions**: checked whenever a player would receive priority.
- **Simultaneous triggers**: ordered automatically, APNAP then by `ObjectId` of the source.
- **Continuous effects**: simplified layer handling — apply in order: base characteristics → P/T setting → P/T modifications (incl. counters) → ability grants. Full layer system is out of scope.
- **Combat**: declare attackers → declare blockers → (first strike damage) → damage → end of combat. Damage among multiple blockers follows the current rules (no damage assignment order: the attacker divides freely, and must assign lethal to every blocker before trampling over). The engine assigns automatically for now: lethal to each blocker in turn, the rest to the last blocker or, with trample, to the player.
- **Game end**: `result()` returns the winner and reason, or a draw (both players at 0 simultaneously). `sim` also enforces a turn cap and reports any game that hits it.

## 6. Run internals (later milestones)

- `RunState`: act number, map (graph of nodes), current node, hearts, gold, deck (as `CardInstance`s, lands included), relics, planeswalker, RNG.
- Map generation: seeded, layered DAG like Slay the Spire (~15 rows, path choices, constraints on node types per row).
- Relics hook into the duel through a **modifier system**: relics register triggers/static effects that are injected into the duel at setup (they're implemented using the same effect primitives where possible). Boons, planeswalker passives and boss emblems use the same route, via `PlayerConfig`.
- All tunable numbers from `data/balance.json`.

## 7. AI

- Lives in `engine/ai/`. Uses only `legal_actions` + state evaluation; never cheats with hidden info unless explicitly configured.
- v1: heuristics (play lands, cast best castable spell, attack if favourable, block to avoid bad trades, remove biggest threat). Mulligan: keep any hand with 2–5 lands, and never go below 5 cards.
- v1 is expected to pilot creature and burn decks acceptably and reactive decks badly (it won't hold mana for a counterspell). That's accepted for now; the human plays the control deck in M2.
- Later: shallow search / Monte Carlo rollouts using engine cloning (determinism makes this easy). This is the planned route to competent control play, rather than piling up heuristics.
- `crates/sim` runs AI vs AI to find crashes and measure deck win rates.

## 8. The wasm boundary

- `engine-wasm` owns the `DuelState` inside wasm memory and exposes a handle. State is never round-tripped through JavaScript.
- Handle methods: `apply(action) -> { events, view }` for the human, `ai_step() -> { events, view }` for the opponent, plus `legal_actions()` and `has_meaningful_action()`. Events are already redacted for the human.
- The UI drives the opponent by calling `ai_step()` while `view.waiting_on` is not the human, animating each batch of events. That loop is the only orchestration in TypeScript; it contains no rules.
- Data crosses as **JSON strings**. Simple, easy to log and paste into a bug report; the cost is irrelevant at this scale.
- Seeds are `u64` and cross as decimal strings (a JS number would lose precision).
- TypeScript types for `Action`, `PlayerView`, `GameEvent` etc. are generated from the Rust types with `ts-rs` into `web/src/engine/types/`. They are never written by hand.

## 9. Front end

- `web/src/engine/` — wasm loading + typed wrapper; the only place that talks to wasm.
- `web/src/screens/` — Duel, Map, Reward, Shop, DeckView.
- `web/src/components/` — Card, Battlefield, Hand, Stack, LifeCounter, RelicBar.
- State: the engine owns game state; React holds the latest `PlayerView` plus an animation queue of events.
- Placeholder card frames (text-based) until art is added.
