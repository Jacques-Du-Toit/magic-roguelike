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
pub fn should_auto_pass(state: &DuelState, player: PlayerId, hold_priority: bool) -> bool;
pub fn apply_action(state: &mut DuelState, player: PlayerId, action: Action)
    -> Result<Vec<GameEvent>, ActionError>;
pub fn player_view(state: &DuelState, player: PlayerId)
    -> Result<PlayerView, InternalError>; // hides opponent's hand/library
pub fn redact_events(events: &[GameEvent], viewer: PlayerId) -> Vec<GameEvent>;
pub fn result(state: &DuelState) -> Option<GameResult>; // Win { player, reason } | Draw
pub fn ai_choose_action(state: &DuelState, player: PlayerId) -> Result<Action, ActionError>;

// Run level
pub fn new_run(config: RunConfig, seed: u64) -> Result<RunState, RunError>;
pub fn run_legal_actions(state: &RunState) -> Vec<RunAction>;
pub fn apply_run_action(state: &mut RunState, action: RunAction)
    -> Result<Vec<RunEvent>, RunError>;
pub fn apply_duel_action(state: &mut RunState, player: PlayerId, action: Action)
    -> Result<Vec<GameEvent>, RunError>; // the duel inside the run
pub fn run_view(state: &RunState) -> Result<RunView, RunError>;
```

The duel functions above exist as of M1, in `crates/engine/src/duel/` (state, turn, stack, combat, mana, damage, effects, view) and `crates/engine/src/ai/`. `apply_action` works on a copy of the state and swaps it in on success, so an error always leaves the state untouched. An `InternalError` means the engine found its own state inconsistent: a bug, never something the caller did. The run functions exist as of M3, in `crates/engine/src/run/` (see §6), and work the same way.

### Action model

Every `Action` returned by `legal_actions` is concrete and can be passed straight to `apply_action`. The AI and the UI both work only from this list. Two rules keep the list small:

- **Spells and abilities are fully specified.** `CastSpell { card, targets }` and `ActivateAbility { source, ability, targets, sacrifice }` appear once per legal combination of targets and, for an ability whose cost includes a sacrifice, of the creature given up. There is no separate "choose target" step. (X costs, when added, will be an exception: one action carrying the legal X range.) No card has modes yet. Mana abilities are not actions: the auto-payer uses them.
- **Anything combinatorial is a stepwise pending decision.** The state records "waiting on player P for decision D", and `legal_actions` returns only the next step:

| Decision | Step actions |
|---|---|
| Declare attackers | `Attack { creature }`, `RemoveAttacker { creature }`, `ResetDeclaration`, `ConfirmAttackers` |
| Declare blockers | `Block { blocker, attacker }`, `RemoveBlocker { blocker }`, `ResetDeclaration`, `ConfirmBlockers` |
| Mulligan | `Keep`, `Mulligan`, then `BottomCard { card }` once per card owed |
| Discard to hand size | `Discard { card }` |
| Divide combat damage | `AssignDamage { target }`, `UnassignDamage { target }` (one point each), `ConfirmDamage` |
| Aim a triggered ability | `ChooseTargets { targets }`, once per legal combination, like a spell's |
| Scry | `ScryKeep`, `ScryBottom`, once per card looked at |
| Discard for an effect | `Discard { card }`, as at cleanup |
| Later: order triggers, "may" choices | one step action each |

A `Confirm…` action is only legal when the declaration so far is legal (e.g. `ConfirmBlockers` is absent while a menace attacker has exactly one blocker, and `ConfirmDamage` while damage is unassigned or tramples over a blocker that has less than lethal).

Other actions: `PlayLand { card }`, `PassPriority`.

**Triggered abilities with targets.** A triggered ability is aimed when it goes on the stack, which is the next time a player would get priority. With no legal target it never goes on the stack; with exactly one way to aim it, that way is taken without asking; otherwise its controller gets the `ChooseTargets` decision and any other waiting abilities queue behind it.

**Effects that stop for a choice.** A discard or a scry needs an answer in the middle of a resolution. The spell or ability being resolved is kept in `DuelState::resolving` with the effects it still has to carry out; the effect sets the decision, and when the player has answered, the resolution carries on from where it stopped. Targets are checked once, when the resolution begins. A discard or scry with no real choice in it (the whole hand goes, the library is empty) doesn't ask.

### Mana payment

Payment is engine logic, not UI logic, because the AI and `sim` need it too.

- `CastSpell` / `ActivateAbility` is legal only if the engine's **auto-payer** can find a payment. Applying the action taps sources and pays in one go.
- The auto-payer is deterministic: lands before creatures, and among lands it prefers to leave untapped the colours the rest of the hand needs. A source makes one mana per tap. A land with a choice of colours (a dual land has one mana ability per colour) is handled by matching the cost's coloured symbols to sources, so it never blocks a payment some other assignment allows; for generic mana it is the last land spent.
- Manual tapping (activating mana abilities one by one into the mana pool) is a later addition. Model the mana pool properly now so that it slots in.

### Auto-pass

Mana abilities are always available, so "the only legal action is pass" is never literally true. `has_meaningful_action` answers the real question: is there any legal action other than `PassPriority` and mana abilities?

`should_auto_pass(state, player, hold_priority)` is what the UI asks: it passes for the player whenever this returns true. The rule lives in the engine so it can be refined without touching TypeScript. As of M2:

- Only a priority decision is ever passed for. With `hold_priority` on (the player's toggle), nothing is.
- A player with no meaningful action is passed for.
- With something on the stack, the player is asked when the top item is the opponent's and passed for when it is their own.
- With an empty stack, the player is asked in their own main phases, in the declare-attackers and declare-blockers steps (once the declaration is made), and in the opponent's end step. Everywhere else they are passed for.

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
    pub starting_permanents: Vec<CardInstance>, // on the battlefield from the start
    pub emblems: Vec<EmblemDef>,                // in the command zone for the whole duel
}
```

An `EmblemDef` is a name, a text and a list of abilities in the card ability format. Emblems may have triggered abilities and static ones (`anthem` and `grant_keyword`, affecting `creatures_you_control`). An emblem is not a permanent, so its triggers are the ones about its controller: `beginning_of_upkeep`, `gain_life`, `cast_spell`, and `enters_battlefield` or `dies` with subject `creature_you_control`; and its effects can't have chosen targets yet, though they may refer to `triggering`. Their triggers use the stack like any other. The run builds them from modifiers (§6).

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

Fields: `id`, `name`, `mana_cost`, `colors`, `supertypes`, `types`, `subtypes`, `power`/`toughness` (creatures), `rarity`, `keywords`, `oracle_text`, `enchant` (Auras), `enters_tapped` (lands), `abilities`. Omitted optional fields default to empty. The examples below cover every shape the M1 starter set needs.

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

Dual land (one mana ability per colour; it still makes one mana per tap):

```json
{
  "id": "selesnya_guildgate",
  "name": "Selesnya Guildgate",
  "mana_cost": "",
  "types": ["Land"],
  "subtypes": ["Gate"],
  "rarity": "common",
  "oracle_text": "This land enters tapped.\n{T}: Add {G} or {W}.",
  "enters_tapped": true,
  "abilities": [
    { "kind": "mana", "cost": { "tap": true }, "produces": "{G}" },
    { "kind": "mana", "cost": { "tap": true }, "produces": "{W}" }
  ]
}
```

### Ability kinds
- `spell` — effects when an instant/sorcery resolves.
- `triggered` — `{ "trigger": {...}, "effects": [...] }`. Built so far: `enters_battlefield` and `dies` (each with `"subject": "self"`, `"creature_you_control"`, which includes the permanent itself, or `"another_creature_you_control"`), `attacks`, `deals_damage_to_opponent`, `gain_life`, `sacrifice` (any player sacrifices a permanent), `cast_spell` (with `"kind": "instant_or_sorcery"` or `"noncreature"`) and `beginning_of_upkeep`. `"once_each_turn": true` makes it "the first time each turn". Prowess is a keyword, not an ability in the data: the engine supplies its trigger.
- `activated` — `{ "cost": {...}, "effects": [...] }`. The cost is any of `"mana": "{3}{U}"`, `"tap": true` and `"sacrifice": "creature"` (any creature of the controller's, the one with the ability included), and at least one of them. It can be used whenever its controller has priority, it uses the stack, and its controller keeps priority afterwards. A creature can't pay a tap cost while summoning sick. Paying life is not built yet.
- `static` — continuous effects: `{ "type": "anthem", "affects": ..., "power": 1, "toughness": 1 }` (optionally `"with_keyword": "flying"`), `{ "type": "grant_keyword", "affects": ..., "keyword": ... }` (optionally `"with_counter": true` for "with a +1/+1 counter on it") and `{ "type": "restriction", "affects": "enchanted", "cant": [...] }`. `affects` is `enchanted` (an Aura's creature), `creatures_you_control` or `other_creatures_you_control`.
- `mana` — mana abilities (don't use the stack).

### Effect primitives (initial set; grow carefully)
`deal_damage`, `gain_life`, `lose_life`, `draw_cards`, `discard`, `destroy`, `exile`, `counter_spell`, `create_token`, `add_counters`, `modify_pt_until_eot`, `grant_keyword_until_eot`, `tap`, `untap`, `return_to_hand`, `add_mana`, `scry`, `search_library` (if needed), `mill`, `sacrifice`.

Built so far: `add_counters` (+1/+1 counters on a creature; they last as long as it stays on the battlefield), `create_token` (see Tokens below), `deal_damage`, `deal_power_damage` (a creature deals damage equal to its power: two targets), `destroy`, `counter_spell`, `discard`, `draw_cards`, `gain_life`, `lose_life`, `grant_keyword_until_eot`, `modify_pt_until_eot`, `return_to_hand`, `scry`, `tap` (with `skip_next_untap` for "doesn't untap during its controller's next untap step").

Effects on a player name one: `controller`, `opponent`, or a targeted one, `target_player` or `target_opponent`.

Two effects on one target ("target creature gets +3/+0 and gains first strike") are written as two effects, the second with `"target": { "same": {} }`, which reuses the target before it instead of asking for another. Two more entries stand for something known rather than chosen, in triggered abilities only: `{ "this": {} }` is the permanent the ability is on ("this creature gets +2/+0"), and `{ "triggering": {} }` is the creature that set the ability off ("that creature gains haste"). They do nothing if that permanent has left the battlefield.

`discard` takes `"random": true` for a discard at random, which uses the duel's seeded RNG.

### Tokens
A token is defined like a card, in `data/tokens/` (one file per token, e.g. `soldier_token.json`): a creature with a name, colours, subtypes and power and toughness, and no mana cost. The loader marks everything in that folder as a token (`CardDef::token`), whatever the file says, and puts it in the same card database, so the engine and the UI treat a token on the battlefield like any other permanent. `{ "type": "create_token", "token": "soldier_token", "count": 2 }` creates them under the controller's control; a token named by an effect must exist. Tokens are never in a deck, a reward or a shop, and `tools/scryfall.mjs check` doesn't look at them. A token that leaves the battlefield goes where it was sent and then ceases to exist the next time state-based actions are checked, as in Magic, so "when this dies" abilities see it die.

Targets are structured data: `any_target`, `creature`, `player`, `opponent` and `spell`, each written with braces that hold its conditions. Built so far: `{ "creature": { "not_color": "B" } }` (nonblack), `{ "creature": { "min_power": 4 } }`, `{ "creature": { "keyword": "flying" } }`, `{ "creature": { "controller": "you" } }` or `"opponent"`, and `{ "spell": { "kind": "creature" } }` or `"noncreature"`. Conditions are checked when the target is chosen and again on resolution. Separately from the conditions, a player can't aim at an opponent's permanent with hexproof (`is_legal_target`); an Aura already attached only needs its creature to still fit `enchant` (`fits_target_spec`).

Keywords built so far: deathtouch, defender, double strike, first strike, flash, flying, haste, hexproof, indestructible, lifelink, menace, reach, trample, vigilance.

### Custom escape hatch
If a card truly can't be expressed, it may reference `{ "type": "custom", "id": "card_specific_fn" }` implemented in Rust. Keep a list here:

| Custom id | Card | Why |
|---|---|---|
| _(none yet)_ | | |

### Validation
On load, every card is validated (costs parse, targets exist, keywords known, Auras have `enchant`). A test loads all card files and fails on any error.

The Rust types list only what a card in the pool uses, and unknown fields, keywords, effects and targets fail to load. Adding a card that needs something new therefore means adding the enum variant and its rules first. Two current limits are enforced the same way: a mana ability produces exactly one coloured mana, and an emblem's triggered ability can't have targets (a card's can). A restriction ("can't attack or block") is still for Auras only.

### Decklists
`data/decks/*.json`: `{ "id", "name", "cards": [{ "card": "grizzly_bears", "count": 2 }, { "card": "forest", "count": 5 }] }`. Lands and spells share one list. Validated on load against the card database. A, B and C are from M1. D (blue-black tempo), E (white-black drain), F (green-white tokens) and G (black-red sacrifice) were added in M4a so that its cards can be played in a practice duel before the run's decks are rebuilt on the cube.

## 5. Duel internals

- **Zones**: library, hand, battlefield, graveyard, exile, stack, command (emblems). Lands are in the library, as in normal Magic.
- **Opening-hand smoothing**: at the start of the duel the engine shuffles and deals `smoothing_candidates` opening hands (default 3) using the seeded RNG, and keeps the one whose land count is closest to the deck's land ratio × hand size (ties: the first dealt). The rest of the library is then shuffled normally. Setting the number to 1 turns smoothing off. Mulligan hands are smoothed the same way.
- **Mulligan**: London mulligan. Draw a fresh hand, then put one card on the bottom per mulligan taken. The starting player decides first and players who haven't kept take turns deciding; the bottoming happens when a player keeps.
- **Fatigue**: if a player would draw from an empty library, they instead lose life: 1 the first time, 2 the second, and so on (per-player counter). There is no lose-on-empty-library rule.
- **Turn structure**: full MTG steps (see `RULES_SCOPE.md`).
- **Priority**: real priority passing; the *UI* auto-passes for the human when `has_meaningful_action` is false (Arena-style), configurable.
- **Stack**: spells and abilities as stack objects; resolves LIFO when all players pass in succession.
- **State-based actions**: checked whenever a player would receive priority.
- **Triggers** (`triggers.rs`): whatever an ability could be waiting for (a permanent entering, a creature about to die, an attacker declared, damage dealt to an opponent, life gained, a spell cast, an upkeep beginning) is reported as a `Happening`; every permanent and emblem is asked whether an ability of its triggers, and those that do wait until a player would next get priority. Creatures that die together are all reported before any leaves, so each sees the others go. An ability whose source has gone by the time it resolves (a "when this dies" ability) still resolves; damage it deals has no deathtouch or lifelink.
- **Activated abilities** (`abilities.rs`): the ability goes on the stack, then its cost is paid: mana by the auto-payer, the tap, the sacrifice. A sacrificed creature dies, and what triggers on that goes on the stack above the ability.
- **Simultaneous triggers**: ordered automatically, APNAP then by `ObjectId` of the source.
- **A spell being cast** triggers abilities too. They go on the stack above the spell, and then the caster has priority again, whoever's turn it is.
- **Continuous effects**: simplified layer handling — apply in order: base characteristics → P/T setting → P/T modifications (incl. counters) → ability grants. Full layer system is out of scope. Anthems and keyword grants come from emblems and from permanents; `DuelState::static_sources` lists the permanents that have one, so that working out a creature's power doesn't read every card on the battlefield.
- **Combat**: declare attackers → declare blockers → (first strike damage) → damage → end of combat. Damage among multiple blockers follows the current rules (no damage assignment order: the attacker divides freely, and must assign lethal to every blocker before trampling over). The automatic division is lethal to each blocker in turn (in the order they were declared), the rest to the last blocker or, with trample, to the player. An attacker with two or more blockers gets a decision for its controller at the start of the damage step, before any damage is dealt: the automatic division is filled in and may be changed a point at a time. With fewer blockers the automatic division is used without asking. Declarations that have no choice in them are skipped: if nothing can attack, or no block is possible, the player isn't asked. With no attackers, combat goes from the declare-attackers step straight to the end-of-combat step, and nobody gets priority in between.
- **Game end**: `result()` returns the winner and reason, or a draw (both players at 0 simultaneously). `sim` also enforces a turn cap and reports any game that hits it.

## 6. Run internals

The run is `crates/engine/src/run/`, built in M3 for one act. It has the duel's shape: plain serialisable state, a seeded RNG inside it, legal actions, and actions that return events.

- **`RunState`**: seed, planeswalker, hearts, gold, deck, relics, boons, the map, the path walked, the phase, the duel in progress (if any) and the RNG. The deck is the player's spells and non-basic lands as `CardInstance`s; basic lands are not stored.
- **Phases**: `Map` (choose a node), `Duel`, `Reward` (pick a card or none), `Rest`, `Shop`, `Boons` (before the boss), `Over`. `run_legal_actions` lists what can be done in each, and is empty only when the run is over.
- **Run actions**: `Travel`, `Concede`, `FinishDuel`, `TakeReward`, `SkipReward`, `RestHeal`, `RemoveCard`, `Buy`, `Leave`, `TakeBoon`, `StartBoss`.
- **The duel inside a run** is created by the run and kept in `RunState::duel`, so one state holds everything and the same seed and actions replay the run, duels included. Duel actions go through `apply_duel_action`. While the duel is being played the run offers only `Concede`; once it has a result, only `FinishDuel`, which applies the consequences (gold, a heart, a relic, a card reward, the end of the run). The human is always P1.
- **Map generation** (`map.rs`): a layered graph. One path starts in each column of the first row and wanders at most one column sideways per row, never crossing another; the cells the paths visit are the nodes, and the boss follows the last row. Each node's kind is drawn from its row's weights. A row with a choice of kinds never shows only one, and a row with a single node is never an elite.
- **Modifiers** (`defs.rs`, `loadout.rs`): relics, boons, planeswalker passives and boss emblems are all `ModifierDef`s: an id, a name, a text and a list of effects. An effect changes a player's starting life, changes the opening hand size, adds a starting basic land, gives its owner abilities (which become an emblem named after the modifier), or pays gold per duel won (relics only; it acts on the run). `duel_config(player, enemy)` turns two `Loadout`s (a library and its modifiers) into a `DuelConfig`; it is the only place modifiers reach the duel.
- **Automatic basic lands** (`lands.rs`): lands are a share of the finished deck (`land_percent`), split between colours by the coloured mana symbols of the spells using largest remainders, with a minimum per colour used. A non-basic land replaces a basic of one of its colours. They are worked out whenever the deck is used or shown.
- **Rewards and shops** (`rewards.rs`): a card is picked by first choosing a rarity by weight, among the rarities on offer, then a card of it. The first `own_color_choices` cards of an offer come from the planeswalker's colours; since `DECISIONS.md` #35 that is all of them.
- **Content** is data: `data/run/planeswalkers.json`, `enemies.json` (hand-built decks with lands, a tier, and any modifiers) and `relics.json`. Boons and every number (hearts, gold, prices, weights, map shape, land share) are in `data/balance.json` under `run`.
- **`RunView`** is what the UI gets: the run without its RNG and its duel, plus the basic lands that go with the deck and the legal run actions. `run_content()` gives the definitions for names and texts.
## 7. AI

- Lives in `engine/ai/`. Uses only `legal_actions` + state evaluation; never cheats with hidden info unless explicitly configured.
- v1: heuristics (play lands, cast best castable spell, attack if favourable, block to avoid bad trades, remove biggest threat). Mulligan: keep any hand with 2–5 lands, and never go below 5 cards (numbers in `balance.json`).
- How v1 picks a spell or an activated ability: it tries each one it could cast or use on a copy of the game, lets the stack and any combat damage still to come play out with both players passing, scores the result (life, creatures with their lasting bonuses from anthems and Auras, enchantments, cards in hand), and casts the best one if it beats doing nothing. That one rule covers removal, burn to the face when it matters, combat tricks after blocks, and countering a spell when mana happens to be open. Passing needs no hidden information, so the AI still doesn't cheat.
- How v1 fights: it attacks with each creature that no single blocker beats, or with everything when that is lethal; it blocks to kill an attacker, to stop damage safely or to trade evenly, and chump-blocks only to survive.
- v1 is expected to pilot creature and burn decks acceptably and reactive decks badly (it won't hold mana for a counterspell). That's accepted for now; the human plays the control deck in M2.
- Later: shallow search / Monte Carlo rollouts using engine cloning (determinism makes this easy). This is the planned route to competent control play, rather than piling up heuristics.
- `crates/sim` runs AI vs AI to find crashes and measure deck win rates. It plays every pairing of the decks in `data/decks/`, mirrors included, swapping seats each game, and reports win rates, game length, mulligans, land screw and flood, and how games ended. Its turn cap and thresholds are in `balance.json`. `--replay` prints one game move by move. `--enemies` plays each planeswalker's starting deck against every enemy deck, set up as the run sets a duel up. `--runs` plays whole runs, with the AI on both sides of each duel and a simple policy for the run's choices, and reports how often the act is won.
- The AI aims a triggered ability by trying each choice on a copy of the game and keeping the best, and it thinks the same choice through when it imagines casting the creature. Scrying, it bottoms a land it has no use for or a spell far from castable.
- Faced with a damage division, the AI confirms the automatic one. It only ever double-blocks to survive a menace attacker, so a human rarely gets to divide damage; better blocking is M4 work.

## 8. The wasm boundary

- `engine-wasm` owns the `DuelState` inside wasm memory and exposes a handle. State is never round-tripped through JavaScript.
- The handle is `Duel`, made with `new Duel(human_deck, ai_deck, seed)` from two deck ids. The human is always P1 and the AI P2. Its methods: `apply(action) -> DuelUpdate` for the human, `ai_step() -> DuelUpdate` for the opponent, plus `view()`, `legal_actions()`, `has_meaningful_action()`, `should_auto_pass(hold_priority)` and `seed()`. A `DuelUpdate` is `{ events, view }`, with the events already redacted for the human.
- A run has its own handle, `Run`, made with `new Run(planeswalker, seed)`. It owns the `RunState` and with it the duel in progress. `view()` returns the `RunView` and `apply(action)` a `RunUpdate` (`{ events, view }`). The duel is reached through the same handle: `duel_view()`, `duel_legal_actions()`, `duel_should_auto_pass(hold_priority)`, `duel_apply(action)` and `duel_ai_step()`, which mirror `Duel`'s methods. TypeScript never builds a duel or reports its result; it sends `finish_duel` and the engine reads the result from its own state.
- `duel_suggest()` returns what the AI would do in the human's place. The end-to-end tests use it (see §9); the player is not shown it.
- `card_defs()`, `decks()` and `run_content()` return every card definition, decklist, and planeswalker, relic, boon and enemy, so the UI can show names, costs and text.
- The crate's logic is in `session.rs`, which is plain Rust and tested natively; `lib.rs` only wraps it for wasm-bindgen.
- The UI drives the opponent by calling `ai_step()` while the view's decision belongs to the AI, and passes for the human while `should_auto_pass` says so, playing each batch of events before the next step. That loop is the only orchestration in TypeScript; it contains no rules.
- Data crosses as **JSON strings**. Simple, easy to log and paste into a bug report; the cost is irrelevant at this scale.
- Seeds are `u64` and cross as decimal strings (a JS number would lose precision).
- TypeScript types for `Action`, `PlayerView`, `GameEvent` etc. are generated from the Rust types with `ts-rs` into `web/src/engine/types/`. They are never written by hand.

## 9. Front end

- `web/src/engine/` — wasm loading + typed wrapper; the only place that talks to wasm.
- `web/src/duel/` — the duel's logic on the TypeScript side, all of it free of rules and unit-tested:
  - `controller.ts` runs a duel for the screen. It applies the human's action, then lets the AI act and passes for the human until the human has a decision, one step at a time. Each step's events are played before the view that follows them is shown, and input is ignored meanwhile. An engine error is shown on screen rather than thrown.
  - `present.ts` turns a batch of events into log lines and animation beats. Damage, life changes and deaths are played over the old view, because what they happen to may be gone from the new one; everything else appears with the new view, which is then held long enough to read.
  - `interaction.ts` maps clicks onto the list from `legal_actions`: which actions use a hand card, which targets can be picked next, which creature can attack or block, which permanent has an ability to activate and what it still needs picked (the creature to sacrifice, then the targets).
  - `prompt.ts` holds the wording: what the game is waiting for and what the buttons say.
- `web/src/run/` — wording for the run: what just happened, and the names of nodes.
- `web/src/screens/` — Menu (start a run, or a practice duel), Run and Duel. The run screen shows the map, rewards, rest sites, shops, the boons and the end of the run from the `RunView`, offers the legal run actions and sends one back. While the run is in a duel it shows the duel screen, with a controller driving the run handle's duel.
- `web/src/components/` — Card, Battlefield, Hand, the stack and log panels, the overlays (mulligan, graveyard, result), the hover preview, the damage-division panel, the run's map and its deck view.
- `web/e2e/` — Playwright tests that click through the real app: whole duels, and whole acts. With `?hints` in the address the page exposes the engine's suggested duel action, and the act tests follow it by clicking the same things a player would, which lets them reach and beat the boss.
- State: the engine owns game state; React renders the controller's snapshot (the view on screen, the legal actions, the log and the effects being animated).
- Every legal action is reachable from the screen: card actions through the hand, declarations and targets through the battlefield, the stack and the player panels, and the rest (pass, confirm, reset, keep, mulligan) as buttons built from the legal-action list.
- A run can be started from the address, e.g. `?run=warden&seed=7&speed=instant`; "Copy bug report" on the run screen copies the planeswalker, the seed and everything the player did.
- A practice duel can be started from the address, e.g. `?seed=7&you=deck_b&ai=deck_a&speed=instant`. The seed and the two decks are enough to replay a game; "Copy bug report" on the duel screen copies them with the human's actions.
- Placeholder card frames (text-based) until art is added.
