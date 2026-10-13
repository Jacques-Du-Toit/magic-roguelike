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
 │  run/    roguelike: map, wounds, feats, rewards, relics     │
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
pub fn ai_choose_action_with(state: &DuelState, player: PlayerId, kind: AiKind)
    -> Result<Action, ActionError>; // AiKind::Search is the AI the game uses
pub fn ai_choose_action(state: &DuelState, player: PlayerId) -> Result<Action, ActionError>; // v1

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
| Aim a triggered ability | `ChooseTargets { targets }`, once per legal combination, like a spell's; for a "you may" ability also with no targets, which declines it |
| Scry, surveil | `ScryKeep`, and `ScryBottom` (scry) or `ScryGraveyard` (surveil), once per card looked at |
| Discard for an effect | `Discard { card }`, as at cleanup |
| Later: order triggers | one step action each |

A `Confirm…` action is only legal when the declaration so far is legal (e.g. `ConfirmBlockers` is absent while a menace attacker has exactly one blocker, and `ConfirmDamage` while damage is unassigned or tramples over a blocker that has less than lethal).

Other actions: `PlayLand { card }`, `PassPriority`.

**Triggered abilities with targets.** A triggered ability is aimed when it goes on the stack, which is the next time a player would get priority. With no legal target it never goes on the stack; with exactly one way to aim it, that way is taken without asking; otherwise its controller gets the `ChooseTargets` decision and any other waiting abilities queue behind it. A "you may" ability (`"optional": true`) with a legal target is always asked about, because it can be declined: choosing no targets means it never goes on the stack.

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

`DuelConfig` is per-player from the start, because relics, wounds, bosses and planeswalker passives all need asymmetric setups:

```rust
pub struct DuelConfig {
    pub players: [PlayerConfig; 2],
    pub rules: RulesConfig,
    pub starting_player: Option<PlayerId>, // None: the seeded coin flip
}
pub struct PlayerConfig {
    pub library: Vec<CardInstance>,     // lands and spells together
    pub starting_life: i32,
    pub opening_hand_size: u8,
    pub starting_permanents: Vec<CardInstance>, // on the battlefield from the start
    pub emblems: Vec<EmblemDef>,                // in the command zone for the whole duel
    pub commander: Option<CardInstance>,        // starts in the command zone, outside the library
}
```

### Commanders (a prototype, `DECISIONS.md` #37 and #45)

A player may have a commander: a legendary creature that is not in their library. It is an ordinary card object in `Zone::Command`, listed in `PlayerState::command`, and `PlayerState::commander` remembers which physical card it is.

- It is cast with the same `CastSpell { card, targets }` action as a card in hand, naming its object in the command zone, so the AI and the UI need nothing new. `DuelState::casting_cost` is what a card costs from where it is: a commander in the command zone pays `RulesConfig::commander_tax` (from `duel.commander_tax` in `balance.json`) for each earlier cast from there.
- `DuelState::move_object` sends a commander that is headed for a graveyard to its owner's command zone instead. That one place covers dying, being countered and being discarded; the event that says what happened to it is followed by `CommanderReturned`. Abilities that trigger on a creature dying see it die as usual. Moved to a hand, it stays there and is cast at its printed cost.
- `PlayerSummary::commander` shows both players the card, its current cost and, while it is in the command zone, the object to cast. `PermanentView::commander` marks it on the battlefield.
- The legend rule is not built: a legendary card is never in a deck, a reward or a shop.

An `EmblemDef` is a name, a text and a list of abilities in the card ability format. Emblems may have triggered abilities and static ones (`anthem` and `grant_keyword`, affecting `creatures_you_control`). An emblem is not a permanent, so its triggers are the ones about its controller: `beginning_of_upkeep`, `gain_life`, `cast_spell`, and `enters_battlefield` or `dies` with subject `creature_you_control`; and its effects can't have chosen targets yet, though they may refer to `triggering`. Their triggers use the stack like any other, take `once_each_turn` and `min_power` as a card's do, and may stop for a choice (a relic's scry). The run builds them from modifiers (§6).

`RulesConfig` holds duel-wide numbers (max hand size, opening-hand smoothing, fatigue progression). Defaults for all of these come from `data/balance.json`; nothing is hard-coded.

A **`CardInstance`** is `{ card_id, upgraded }`, not a bare card id: a card of a deck, and whether this copy has been upgraded (`DECISIONS.md` #72). The duel never sees the flag. `new_duel` asks `CardDatabase::played_as(instance)` for the definition each copy is played as, the card or its upgraded side (§4), and creates the object from that; an upgraded copy of a card with no upgraded side is a `ConfigError`.

## 3. Determinism

- One seeded RNG (`ChaCha8Rng`) lives in the state and is the only randomness source.
- Same seed + same action sequence = identical game. This enables replays, bug reports ("seed 1234, these actions"), and reliable tests.
- No `HashMap` iteration in gameplay paths; use ordered collections.
- Two kinds of id:
  - **`ObjectId(u32)`**, assigned in creation order. A card changing zones becomes a new object (per MTG rules), so rules references ("target creature") correctly stop pointing at it.
  - **`CardId(u32)`**, one per physical card or token for the whole duel. It never changes. Events and `PlayerView` carry both, so the UI can animate one card moving between zones.
- `DuelState` and everything in it derive `Serialize`/`Deserialize` from the start. Card definitions are referenced by id, never embedded in state. This keeps save/resume and replays cheap to add.
- The starting player is `DuelConfig::starting_player` when it names one, as the run's `duel_config` does (the player, P1); otherwise it is chosen by the seeded RNG.

## 4. Card data format

Cards are JSON in `data/cards/`, one file per card, embedded at compile time with the `include_dir` crate (so the same loader works natively and in wasm; the engine does no file IO). Each card = Scryfall-derived fields + our ability definitions built from **effect primitives**.

Fields: `id`, `name`, `mana_cost`, `colors`, `supertypes` (`Basic` or `Legendary`), `types`, `subtypes`, `power`/`toughness` (creatures), `rarity`, `keywords`, `oracle_text`, `enchant` (Auras), `enters_tapped` (lands), `abilities`. Omitted optional fields default to empty. The examples below cover every shape the M1 starter set needs.

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
- `triggered` — `{ "trigger": {...}, "effects": [...] }`. Built so far: `enters_battlefield` and `dies` (each with `"subject": "self"`, `"creature_you_control"`, which includes the permanent itself, or `"another_creature_you_control"`), `attacks`, `deals_damage_to_opponent`, `gain_life`, `sacrifice` (any player sacrifices a permanent), `cast_spell` (with `"kind": "instant_or_sorcery"` or `"noncreature"`), `beginning_of_upkeep`, `beginning_of_combat` (on its controller's turn) and `beginning_of_end_step` (its controller's). An `enters_battlefield` subject can also be `"token_you_control"`. `"once_each_turn": true` makes it "the first time each turn". Prowess is a keyword, not an ability in the data: the engine supplies its trigger.
- `activated` — `{ "cost": {...}, "effects": [...] }`. The cost is any of `"mana": "{3}{U}"`, `"tap": true` and `"sacrifice": "creature"` (any creature of the controller's, the one with the ability included), and at least one of them. It can be used whenever its controller has priority, it uses the stack, and its controller keeps priority afterwards. A creature can't pay a tap cost while summoning sick. Paying life is not built yet.
- `static` — continuous effects: `{ "type": "anthem", "affects": ..., "power": 1, "toughness": 1 }` (optionally `"with_keyword": "flying"`), `{ "type": "grant_keyword", "affects": ..., "keyword": ... }` (optionally `"with_counter": true` for "with a +1/+1 counter on it") and `{ "type": "restriction", "affects": "enchanted", "cant": [...] }`. `affects` is `enchanted` (an Aura's creature), `creatures_you_control`, `other_creatures_you_control`, `creature_tokens_you_control` or `this` (the creature itself, for "has flying as long as it has a +1/+1 counter on it").
- `mana` — mana abilities (don't use the stack).

### Effect primitives (initial set; grow carefully)
`deal_damage`, `gain_life`, `lose_life`, `draw_cards`, `discard`, `destroy`, `exile`, `counter_spell`, `create_token`, `add_counters`, `modify_pt_until_eot`, `grant_keyword_until_eot`, `tap`, `untap`, `return_to_hand`, `add_mana`, `scry`, `search_library` (if needed), `mill`, `sacrifice`.

Built so far: `add_counters` (+1/+1 counters on a creature; they last as long as it stays on the battlefield), `create_token` (see Tokens below), `deal_damage`, `deal_power_damage` (a creature deals damage equal to its power: two targets), `destroy`, `counter_spell`, `discard`, `draw_cards`, `gain_life`, `lose_life`, `grant_keyword_until_eot`, `mill`, `modify_pt_until_eot`, `return_to_hand` (a creature on the battlefield, or a card in a graveyard), `scry`, `surveil`, `tap` (with `skip_next_untap` for "doesn't untap during its controller's next untap step"), `your_creatures_get_until_eot` ("creatures you control get +2/+1 until end of turn", with an optional `keyword` they gain; it reaches the creatures its controller has when it resolves).

More built for the green-white pool: `add_counters_to_your_creatures` (with `which`: `all`, `others` or `with_counter`), `double_counters` (one creature) and `double_counters_on_your_creatures`. Every counter an effect adds goes through one function (`put_counters` in `effects.rs`), which also keeps each player's count of counters put on their own creatures this turn. A creature card may have `"enters_with_counters": 4`, a property like `enters_tapped`.

Built with the lifegain package: `gain_life_equal_to` (`"stat": "power"` or `"toughness"`, `"of": "this"` or `"triggering"`), which reads the creature as it is, or as it was when the ability triggered if it has since left the battlefield (the number is remembered on the waiting ability and the stack item). A triggered ability may have a `condition` (`{ "type": "life_gained_this_turn", "at_least": 3 }` or `{ "type": "life_above_starting", "at_least": 5 }`), checked when it would trigger and again when it resolves; so may an activated ability, which can also be `"sorcery_speed": true`; and so may a `grant_keyword`. `beginning_of_end_step` takes `"each_player": true`. `{ "kind": "static", "type": "extra_counters", "amount": 1 }` is Conclave Mentor's "that many plus one instead": the one replacement effect, applied in `put_counters` and to "enters with" counters, and nowhere else.

Built with the big-creatures package: an `enters_battlefield` trigger may have `"min_power": 4` (the creature that enters, as it enters); `leaves_battlefield` (the permanent itself, for anywhere: `DuelState::move_object` reports it before the permanent goes); the condition `{ "type": "control_creature_with_power", "at_least": 4 }`, with `"another": true` to leave out the ability's own permanent (not allowed on a keyword grant, because power can depend on keywords); `{ "kind": "static", "type": "cant_be_blocked_by_power", "at_most": 2 }`; `"min_toughness"` on `your_creatures_get_until_eot`; a card's `"cant_be_countered": true`; and `fetch_basic_land`, which puts a basic land from the library onto the battlefield tapped and shuffles. The game picks that land (a colour the player has fewest lands for, the first in WUBRG order on a tie): there is no screen for choosing a card from a library yet.

Built with the Auras and enchantments package: `cast_spell` with `"kind": "enchantment"`; the `enters_battlefield` subject `"enchantment_you_control"` (the ability's own permanent included, if it is an enchantment: "constellation"); `put_into_graveyard_from_battlefield` (the permanent itself; Auras use it), with `return_this_to_hand` for "return it to its owner's hand"; and `for_each` on an `anthem`, with the counts `enchantment_you_control` and `other_enchantment`. A count never depends on a creature's power, so an anthem may use any of them.

A card file may say `"altered": true`: its text is ours, not the printed one, and `tools/scryfall.mjs check` then compares everything but the oracle text.

Built for wounds (`DECISIONS.md` #68): the card type `Artifact`, which so far only the Festering Wound token has; the effect `sacrifice_this` ("sacrifice this", for an ability of a permanent, never a spell or an emblem: its controller sacrifices the permanent if it is still on the battlefield); the trigger `beginning_of_combat` with `"opponents_turn": true`, which fires in each opponent's beginning of combat and not in its controller's (Open Gash, `DECISIONS.md` #75); the target spec `random_opponent_creature`, which is not a target: when the effect resolves, one of the creatures the controller's opponent controls is picked with the duel's RNG, from them in the order they entered, and with none the effect does nothing and the RNG is not drawn; and the card property `"burden": true`, which changes no rule and tells the AI that the permanent counts against whoever controls it (§7).

Built in the balance pass of the Warden's first act (a proposal on its branch, `docs/BALANCE_REPORT.md`): the effect `deal_damage_to_each_creature` (every creature, both sides); the counts `other_creature_with_flying` (either side's, the ability's own permanent left out) and `other_creature_you_control_with_counter`; the creature filter `"with_counter": true` ("with a +1/+1 counter on it"); and the static ability `weaker_creatures_cant_block` ("creatures with power less than this creature's power can't block creatures you control", checked where a block is declared, beside `cant_be_blocked_by_power`). A card may be `"reserved": true`: no reward, draft, shop or event ever offers it (an enemy's card, a starting-deck card, or one kept for an event). A relic may be `"reserved": true` in the same way.

`draw_cards` and `create_token` take `for_each` too (`creature_you_control`, `creature_you_control_with_counter`, `counter_put_this_turn`). `modify_pt_until_eot` takes `"for_each": "creature_you_control"` for "+1/+1 for each creature you control", counted when it resolves.

Effects on a player name one: `controller`, `opponent`, or a targeted one, `target_player` or `target_opponent`.

Two effects on one target ("target creature gets +3/+0 and gains first strike") are written as two effects, the second with `"target": { "same": {} }`, which reuses the target before it instead of asking for another. Three more entries stand for something known rather than chosen. `{ "you": {} }` is the controller ("deals 2 damage to you"). In abilities only, `{ "this": {} }` is the permanent the ability is on ("this creature gets +2/+0"), and in triggered abilities `{ "triggering": {} }` is the creature that set the ability off ("that creature gains haste"); these two do nothing if that permanent has left the battlefield.

`discard` takes `"random": true` for a discard at random, which uses the duel's seeded RNG.

### Upgraded sides
A card file may have `"upgraded"`: what the card becomes when a copy of it is upgraded in a run (`DECISIONS.md` #72). It lists only what differs, and a field that is there replaces the card's whole field: `mana_cost`, `power`, `toughness`, `keywords`, `enchant`, `enters_with_counters`, `abilities` (all of them, restated), and `oracle_text` whenever what the card does has changed.

```json
{
  "id": "raise_the_alarm",
  "name": "Raise the Alarm",
  "mana_cost": "{1}{W}",
  "...": "...",
  "upgraded": {
    "oracle_text": "Create three 1/1 white Soldier creature tokens.",
    "abilities": [
      { "kind": "spell", "effects": [ { "type": "create_token", "token": "soldier_token", "count": 3 } ] }
    ]
  }
}
```

The loader makes a card of its own from it: id `raise_the_alarm+`, name "Raise the Alarm+", `upgrade_of` naming the card, and the card's `upgrade` naming it back (both are set by the loader, never written in a file). It is validated like any card, must differ from its card in something other than text, and may change only the generic mana of the cost, because the deck's basic lands are split by the coloured symbols of the cards as printed. Lands and tokens have none. `CardDatabase::get` finds an upgraded side by its id, so the duel, the AI and the UI treat it as one more card; `CardDatabase::iter` leaves the sides out (`upgraded_sides` lists them), so nothing offers one, counts one or guesses one into an opponent's hand. A decklist may name one (`"card": "raise_the_alarm+"`). The Scryfall check doesn't look inside `upgraded`.

Every card the Warden starts with or can be offered has one, 112 in all, and a test (`upgrade_tests.rs`) holds the table of them: a new card for the Warden's pool fails it until it has an upgraded side and a row. The other colours' cards have none yet.

### Tokens
A token is defined like a card, in `data/tokens/` (one file per token, e.g. `soldier_token.json`): a creature with a name, colours, subtypes and power and toughness, and no mana cost. (One token is an artifact instead: `festering_wound.json`, which a wound of the run starts a duel with.) The loader marks everything in that folder as a token (`CardDef::token`), whatever the file says, and puts it in the same card database, so the engine and the UI treat a token on the battlefield like any other permanent. `{ "type": "create_token", "token": "soldier_token", "count": 2 }` creates them under the controller's control; a token named by an effect must exist. Two tokens may share a name (a Soldier with lifelink and one without); cards may not. Tokens are never in a deck, a reward or a shop, and `tools/scryfall.mjs check` doesn't look at them. A token that leaves the battlefield goes where it was sent and then ceases to exist the next time state-based actions are checked, as in Magic, so "when this dies" abilities see it die.

Targets are structured data: `any_target`, `creature`, `player`, `opponent`, `spell` and `card_in_your_graveyard` (with `"kind": "creature"` or `"permanent"`), each written with braces that hold its conditions. Built so far: `{ "creature": { "not_color": "B" } }` (nonblack), `{ "creature": { "min_power": 4 } }`, `{ "creature": { "keyword": "flying" } }`, `{ "creature": { "controller": "you" } }` or `"opponent"`, and `{ "spell": { "kind": "creature" } }` or `"noncreature"`. Conditions are checked when the target is chosen and again on resolution. Separately from the conditions, a player can't aim at an opponent's permanent with hexproof (`is_legal_target`); an Aura already attached only needs its creature to still fit `enchant` (`fits_target_spec`).

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
`data/decks/*.json`: `{ "id", "name", "cards": [{ "card": "grizzly_bears", "count": 2 }, { "card": "forest", "count": 5 }] }`. Lands and spells share one list. Validated on load against the card database. A, B and C are from M1. D (blue-black tempo), E (white-black drain), F (green-white tokens), G (black-red sacrifice) and H (black-green graveyard) were added in M4a so that its cards can be played in a practice duel before the run's decks are rebuilt on the cube.

## 5. Duel internals

- **Zones**: library, hand, battlefield, graveyard, exile, stack, command (emblems, and a commander waiting to be cast). Lands are in the library, as in normal Magic.
- **Opening-hand smoothing** (`setup.rs`, `deal_opening_hand`): the engine shuffles the library with the seeded RNG and looks at the top cards as a candidate hand, again and again until a hand is **right** or `smoothing_candidates` hands (5, from `duel.smoothing_candidates`) have been dealt. Both players are smoothed alike, and so is every mulligan hand, which goes through the same function. The rest of the library is then shuffled normally. Setting the number to 1 turns smoothing off.
  - A hand is right when it misses nothing (`HandMiss`). Its **land count fits**: the deck's land ratio × hand size, rounded down or up, both fit (2.8 wanted: 2 or 3; exactly 3 wanted: only 3). Its **colours have sources**: every colour in the mana costs of its non-land cards is made by a tap ability of at least one land in the hand (`CardDef::mana_colors`, so a Guildgate is a source of each of its two colours). Only lands count as sources, and the commander's cost is not looked at: it is not in the hand.
  - When no hand is right, the one that misses least is kept: one point for each land too few or too many, one for each colour without a source, the fewest points winning. Ties go to the land count closest to the exact ratio, then to the first dealt.
  - How many hands a deal takes depends on the cards, so the RNG is used a different number of times from one deal to the next. It is still only the duel's RNG: the same seed and actions give the same hands.
- **Starting player**: `DuelConfig::starting_player`. The run's `duel_config` names P1, the player, for every duel of a run (and for the sim's duels built from loadouts: `--enemies` and the balance lab). `None`, as `DuelConfig::from_decks` gives for practice duels and the sim's deck pairings, is the seeded coin flip. Whoever starts skips the draw of turn 1.
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

- **`RunState`**: seed, planeswalker, the starting emblem chosen, wounds, gold, deck, relics, the map (with who waits at the boss's node and each elite's), the path walked, the phase, the duel in progress (if any) with its stats and the feats picked for it, the enemies and events already met, what an event has left for the next duel, and the RNG. The deck is the player's spells and non-basic lands as `CardInstance`s; basic lands are not stored.
- **Phases**: `Map` (choose a node), `Feats` (a duel is ahead: pick feats for it), `Duel`, `Reward` (pick a card or none), `Rest`, `Shop`, `Draft` (a pack is open), `Treasure` (relics or gold are on offer), `Event` (an event has been met), `Over`. `run_legal_actions` lists what can be done in each, and is empty only when the run is over.
- **Run actions**: `Travel`, `ChooseFeats`, `Concede`, `FinishDuel`, `TakeReward`, `SkipReward`, `HealWounds`, `UpgradeCard`, `RemoveCard`, `Buy`, `TakeDraft`, `TakeTreasure`, `TakeGold`, `TakeEvent`, `Leave`.
- **Rest sites and upgrades** (`DECISIONS.md` #72): a rest site allows one thing, `HealWounds` or `UpgradeCard { card }`, or `Leave`. `HealWounds` removes every wound carried (`DECISIONS.md` #75) and is a legal action only while there is one. `UpgradeCard` is a legal action for each card of the deck that has an upgraded side and is not upgraded yet; it sets the copy's `upgraded` flag and reports `CardUpgraded`. The copy keeps its `card_id`, so its rarity, its place in the deck, and what the Wandering Smith would make of it are the card's. `RemoveCard` is a shop's service only.
- **The duel inside a run** is created by the run and kept in `RunState::duel`, so one state holds everything and the same seed and actions replay the run, duels included. Duel actions go through `apply_duel_action`. While the duel is being played the run offers only `Concede`; once it has a result, only `FinishDuel`, which applies the consequences (gold, the feats' score, a wound, a relic, a card reward, the end of the run). The human is always P1. Arriving at the boss starts its duel at once.
- **Wounds** (`data/run/wounds.json`, `DECISIONS.md` #68): a lost duel adds a wound to `RunState::wounds`, and the run ends when there are `run.max_wounds`. The wound is one of the file's, picked with the run's RNG among the kinds not carried already (among all of them if every kind is carried), so two wounds are two different ones (`DECISIONS.md` #75); an event's `take_wound` picks the same way. A wound is a `ModifierDef` like a relic, and joins the player's modifiers in every duel. There are five, each one effect: the Festering Wound (`starting_token`), Open Gash (an emblem: at the beginning of combat on the enemy's turn a random creature the enemy controls gets +1/+1 until end of turn), Concussion (`opening_hand` -1), Shaken (`starting_life` +2 for the opponent) and Hunted (`starting_token`, a Soldier, for the opponent). `HealWounds` at a rest site and an event's `heal_wounds` take them all off, reporting a `WoundHealed` for each. After a loss that doesn't end the run the phase is `Reward` with `run.rewards.loss_choices` cards.
- **Feats** (`feats.rs`, `data/run/feats.json`, `DECISIONS.md` #69): a `FeatDef` is a name, a text with `{n}` where a tier's number goes, a `measure`, up to three `tiers`, an optional `by_turn` and the `planeswalkers` it is offered to (empty for all). `Travel` to a duel picks the enemy (an elite's is on its node already), draws `run.feats.offered` feats from the character's pool with the run's RNG and stops in `Phase::Feats`; `ChooseFeats { feats }` names `run.feats.picks` of them by their places, in rising order (every such set is a legal action), and starts the duel. With no feats in the data the duel starts at once. While a duel is played, `RunState::duel_stats` (`DuelStats`) is brought up to date inside `apply_duel_action` from each action's events and the state after it: the player's own turns begun, their lowest life, their creatures that died, and for each stretch from one of their turns to the next the high points reached (greatest power, power lead over the enemy, creatures, +1/+1 counters, enchantments, and the most damage the enemy player was dealt in one turn, which is added up from the events as they come). The stats don't depend on which feats were picked, and the position is looked at once per action, so a high point that came and went inside one action is not seen. A feat's score is the number of its tiers the stats meet (`FeatDef::score`); for the measures where fewer is better (turns taken, creatures lost) a tier is "at most". On a win `FinishDuel` reports `FeatsScored`, and the points pick a row of `run.feats.rewards`: how many cards to choose from, how many steps up the ladder of rarity weights (`rewards.duel`, `elite`, `top`; an elite starts one step up) and how much gold. `feat_scores` says what the feats picked would score if the duel were won now; `RunView::feats` and the wasm handle's `duel_feats()` carry it to the screen.
- **Map generation** (`map.rs`): a layered graph. One path starts in each column of the first row and wanders at most one column sideways per row, never crossing another; the cells the paths visit are the nodes, and the boss follows the last row. Each node's kind is drawn from its row's weights. A row with a choice of kinds never shows only one, and a row with a single node is never an elite.
- **Who is ahead** (`place_enemies` in `map.rs`, `DECISIONS.md` #76): `new_run` draws the map and then, from the same RNG, says who waits at the boss's node and at each elite's, in `MapNode::enemy`. So the boss and every elite are fixed from the first moment of the run and the map can show them; a seed's map is what it was before, since they are drawn after it. The elites are dealt without one being used twice: an elite node gets an elite that is nowhere on the map yet. A map has at most four elite nodes (two rows can hold them, and neither is ever all elites) and there are four elites, so today every elite node of a map is a different elite. Should there ever be more nodes than elites, a node gets an elite that is on no elite node leading to it, so that a path doesn't meet one twice, and failing that any. `Travel` to an elite or the boss meets the enemy on the node and draws nothing. `MapNode::enemy` is `None` for every other node: the enemy of an ordinary duel is picked on arriving, with the run's RNG, among those of the row's tier not in `RunState::fought` (among all of the tier once all are), so nothing in the state or the view says who it will be.
- **Modifiers** (`defs.rs`, `loadout.rs`): relics, wounds, planeswalker passives and boss emblems are all `ModifierDef`s: an id, a name, a text and a list of effects. An effect changes a player's starting life, changes the opening hand size, adds a starting basic land (`starting_basic_land`), starts its owner, or with `"who": "opponent"` the other player, with tokens on the battlefield (`starting_token`, with a `token` and a `count`), or gives its owner abilities (which become an emblem named after the modifier). Two more act on the run and never reach a duel, and only a relic may have them: `gold_per_win`, and `shop_discount`, a `percent` off everything a shop charges. `duel_config(player, enemy)` turns two `Loadout`s (a library, its modifiers and its commander, if any) into a `DuelConfig`; it is the only place modifiers reach the duel. A land or a token a modifier starts a duel with is a starting permanent: it is there before the opening hands, it is not summoning sick, and nothing triggers on it entering. Every token a modifier names, to start with or in an ability's `create_token`, is checked against `data/tokens/` when the data loads.
- **Starting emblems** (`emblems.json`): what used to be a character's passive is now a modifier the player picks when the run starts (`RunConfig::emblem`; `None` takes the character's usual one, `PlaneswalkerDef::emblem`). Any emblem goes with any character. A character need not have a usual one, and the file may be empty: then a run starts with no emblem and the menu doesn't ask (the balance pass empties it).
- **Enemy tiers**: `easy`, `normal`, `hard`, `elite`, `boss`. A map row names the tier of its duels (`duel_tier`); with no hard enemy in the data a hard row meets a normal one.
- **Commanders**: a character and an enemy may each name a `commander`, a legendary creature card. The loader checks that it is one and that its colours are among the character's (for an enemy, among those of the cards in its deck), and that no deck holds a legendary card.
- **Automatic basic lands** (`lands.rs`): lands are a share of the finished deck (`land_percent`), split between colours by the coloured mana symbols of the spells using largest remainders, with a minimum per colour used. A non-basic land replaces a basic of one of its colours. They are worked out whenever the deck is used or shown.
- **Rewards and shops** (`rewards.rs`): a card is picked by first choosing a rarity by weight, among the rarities on offer, then a card of it. The first `own_color_choices` cards of an offer come from the planeswalker's colours; since `DECISIONS.md` #35 that is all of them.
- **Drafts**: a draft node opens a pack of `run.draft.cards` different spells in the planeswalker's colours, picked by `run.draft.weights` like a reward's. `TakeDraft { choice }` adds one to the deck and takes it out of the pack; after `run.draft.picks` of them, or on `Leave`, the run is back on the map (`DECISIONS.md` #57).
- **Treasures**: a treasure node offers `run.treasure.relics` different relics the player doesn't own (fewer if fewer are left, none if they own them all) and `run.treasure.gold` gold. `TakeTreasure { choice }` takes a relic and `TakeGold` the gold; either goes back to the map. The gold is always on offer, so the node always has a way on, and there is no leaving with nothing (`DECISIONS.md` #64).
- **Events** (`data/run/events.json`, `DECISIONS.md` #66): an event is a name, a text and a few options, each a list of effects that all happen: `take_wound`, `heal_wounds` (every wound carried), `gain_gold`, `pay_gold`, `gain_relic` (one of the `relics` the event shows, the player's choice), `gain_cards` (all the `cards` it shows), `transmute` (a card of the deck, the player's choice, for a random card of the next rarity up in their colours) and `next_duel` (a modifier for the next duel only). An event node meets an event not met before in the run while there are any, and rolls what it shows into `Phase::Event`. `TakeEvent { option, pick }` takes an option; `pick` is the relic or the deck card for an option that needs one chosen, and is `None` otherwise. An option is a legal action only while every cost can be paid and every gain had (`event_picks` in `flow.rs`), `take_wound` is never the wound that would end the run, and `Leave` is always legal, so an event can't end a run or block one. One option is taken and the run is back on the map. A `next_duel` modifier waits in `RunState::next_duel` (shown in `RunView::next_duel`), joins the player's modifiers when the next duel of any kind starts, and is then gone.
- **Shop prices**: a `ShopItem`'s `price` is what the player pays. `shop_stock` sets list prices, and `price_shop` in `flow.rs` takes the player's `shop_discount`s off them and off the removal price, on arriving and again when a relic is bought there, so a discount bought in a shop lowers what is still for sale in it.
- **Content** is data: `data/run/planeswalkers.json`, `emblems.json`, `enemies.json` (hand-built decks with lands, a tier, and any modifiers; the tier follows what `sim --enemies` measures) `relics.json`, `wounds.json`, `events.json` and `feats.json`. Every number (the wound that ends a run, gold, prices, weights, what the feats' points pay, map shape, land share, what a treasure holds) is in `data/balance.json` under `run`. The map is 11 rows and the boss (`DECISIONS.md` #67). A map row's weights are `duel`, `elite`, `rest`, `shop`, `draft`, `treasure` and `event`. An event's own numbers are beside its text in `events.json`, as a relic's are.
- **`RunView`** is what the UI gets: the run without its RNG and its duel, plus the basic lands that go with the deck, how the feats of the duel in progress stand, and the legal run actions. Its map carries `MapNode::enemy` for the elites and the boss, and `RunView::boss` (`BossView`) names the boss with the life it starts on by its own modifiers (`enemy_starting_life` in `loadout.rs`, which builds the duel's config to read it, so the screen adds nothing up; a wound of the player's that gives the enemy life is not counted). The screen looks up names, commanders and emblems by id in `run_content()`, on the map as on the feats screen, so the two agree. `run_content()` gives the definitions for names and texts, and the rows of the feats' rewards.
## 7. AI

- Lives in `engine/ai/`. Uses only `legal_actions` + state evaluation; never cheats with hidden info unless explicitly configured.
- v1: heuristics (play lands, cast best castable spell, attack if favourable, block to avoid bad trades, remove biggest threat). Mulligan: keep any hand with 2–5 lands, and never go below 5 cards (numbers in `balance.json`).
- How v1 picks a spell or an activated ability: it tries each one it could cast or use on a copy of the game, lets the stack and any combat damage still to come play out with both players passing, scores the result (life, creatures with their lasting bonuses from anthems and Auras, enchantments, cards in hand), and casts the best one if it beats doing nothing. That one rule covers removal, burn to the face when it matters, combat tricks after blocks, and countering a spell when mana happens to be open. Passing needs no hidden information, so the AI still doesn't cheat.
- How v1 fights: it attacks with each creature that no single blocker beats, or with everything when that is lethal; it blocks to kill an attacker, to stop damage safely or to trade evenly, and chump-blocks only to survive.
- v1 is expected to pilot creature and burn decks acceptably and reactive decks badly (it won't hold mana for a counterspell, and it sees no gain in trading a card for a card, so it never casts Raise Dead). That's accepted until M4b.
- **v2, the search** (`ai/search.rs`, `AiKind::Search`; built in M4b, `DECISIONS.md` #41 and #47 to #52). It is the opponent in every duel and the source of `duel_suggest`. For each thing it might do it fills in the cards it can't see with a guess (a "world"), does the thing on a copy, lets v1 play both sides from there to the start of the turn `turns` turns on, and scores the result with v1's evaluation on a scale that ends at `win_value`, so that one unlikely win can't outweigh everything else. Every candidate is tried in the same `worlds` guesses, and the best average wins; v1's own choice wins a tie. v1 stays in the code for good: it is the player inside the imagined games and the baseline to measure against.
  - What it searches so far: what to cast or use with priority (passing is always a candidate, with the `candidates` spells and abilities that v1's glance ranks highest, at most `targets_per_spell` ways of aiming each), and the targets of a triggered ability. It plays a land first, as v1 does, and it acts only where a human would be asked (`should_auto_pass`). Mulligans, scrying and discards are still v1's.
  - Attacks and blocks (`ai/battle.rs`) are whole plans, and there are too many to try them all. The search scores a few (v1's, everything or nothing, and for blocks v1's with two creatures ganging up on an attacker), then changes the best a creature at a time while the imagined games come out better, up to `attack_plans` or `block_plans` plans. Interchangeable creatures count once. The plan is worked out for the board as it was before anything was declared, so each step of a declaration heads for the same plan; the last plan is remembered between the steps, which only saves finding it again.
  - Dividing a double-blocked attacker's damage needs no imagined games, since nothing about it is hidden: each order of the blockers (lethal to each in turn) is tried on a copy and the best board wins.
  - Keeping mana up. In its own main phase, holding an instant or a card with flash, it also weighs "pass and hold": in the imagined games its player then casts nothing more in the main phases of that turn, so the counterspell or instant can still be paid for in the opponent's turn (where v1, playing its seat, uses it if that gains anything). Plain passing can't stand for that, because the imagined v1 would pass and then cast its creature a moment later.
  - Keeping mana up for combat. In its first main phase, able to cast an instant or a card with flash, it also weighs "pass, attack, and keep it for after blocks", once for each of up to `hold_attacks` attacks: v1's, v1's with one more creature, and everything. In those imagined games its player casts nothing until blocks are declared and makes that attack; from then on v1 plays its seat and casts the pump spell if the blocks make it worth it. Without this a pump spell was always cast before attacking, because v1 only attacks where no blocker beats the attacker, so no imagined game ever attacked a 2/2 into a 2/4 with the trick still in hand. The attack it then really declares is searched as any other.
  - What a creature is worth (`evaluate.rs`, both AIs): about twice its power plus its toughness, a little for each useful keyword, and a bonus for an ability that keeps working, set by how often it does something rather than by what it does. A trigger that fires most turns unaided (upkeep, combat, end step, another creature entering) or an extra-counters effect is worth 4; one that needs something else to go right (attacking, connecting, gaining life, a sacrifice, a spell, another creature dying) or an anthem for other creatures is worth 2; less 1 if it is once a turn or conditional. An enters trigger already used and a death trigger are worth nothing. So a 2/2 with an engine ability isn't expected to trade with a plain 3/3, by the AI or by the opponent it imagines.
  - A permanent marked `burden` (the Festering Wound) counts against whoever controls it, by a little less than a small creature is worth, in both AIs' judgement. So the AI casts its creatures first and pays a wound off with mana it has no other use for, and that is the only thing either AI knows about wounds. Neither knows about feats: an AI playing the player's side (the sim, the end-to-end tests' hints) meets them only by accident.
  - Scoring (`evaluate.rs`): v1's judgement of a position, except that an instant or sorcery in hand is worth more the more it costs (so Murder isn't spent on a 1/1 token) and a commander in the command zone is worth a third of what it is on the battlefield. The search's glance at candidates and its imagined players judge the same way, or they would spend the card a moment later and the search could never keep it. A premium on permanents in hand was tried and dropped: it made cheap creatures look not worth casting.
  - Timing of instants. When passing does as well in the imagined games as the best thing it could cast, and it will be asked again later in the turn (in combat, or in the opponent's turn before their end step), it passes: an instant or a flash creature held back shows the opponent nothing until they have committed. It doesn't wait in its own main phases or in the opponent's end step, where there is no later, and it never waits to win. One case in its own main phases is covered: when "pass and hold" scores the same as casting a creature with flash, which is what it would otherwise do, it holds, and the creature comes down in the opponent's turn. That is for flash creatures only; an instant that scores the same either way is still cast now, because the same rule for instants held back removal in positions where tests expect it cast (a Shock in the second main phase, a Murder on the one real threat).
  - `ai_explain(state, player, kind)` makes the same choice as `ai_choose_action_with` and returns it with everything that was weighed (each play and its score added up over the imagined games) and a note in words when nothing was weighed or a tie was settled. `sim --explain FILE` uses it: it replays a bug report copied from the game (a practice duel's, or a run's, which the duel screen of a run now copies too) and prints the AI's last decisions with the position and the scores. Because the engine and the AI are deterministic, and decide the same in WebAssembly as natively, the replay is the game that was played.
  - No enemy deck has a plan hint (aggro, midrange, control). The blue spell decks gain as much from the search as the others, so the numbers didn't call for one.
  - The imagined players act only where a human would be asked, like the search itself, and declare a whole attack or set of blocks in one go. v1 proper thinks again at every step of every turn and declares a creature at a time, and in imagined games that was most of the work.
  - The imagined players are v1 with two more differences, both settings: they look less deeply at chains of triggered-ability targets (`rollout_lookahead`), and they block an attacker with two creatures when that kills it for the loss of one at most (`rollout_gang_blocks`), without which the search could not see that danger when it attacks.
  - Hidden information: a world keeps everything its player can see. The opponent's unseen cards (hand and library, a commander excepted) are dealt out again among those same places, the player's own library is reordered, and the copy gets dice of its own. With `knows_decklist` off, which is how it is set (`DECISIONS.md` #49), each unseen card of the opponent's is instead a guess: a basic land of a colour they have shown, as often as lands are in a deck, or else any card of those colours. The AI's dice are seeded from a hash of its player's view (FNV-1a, written out so that it is the same on every machine), so the same position is always thought about the same way, and the duel's own dice are never touched. Tests check that rearranging everything unseen never changes its choice.
  - Every count is in `balance.json` under `ai.search`, never a time limit. `rollout_lookahead` makes the imagined players think less deeply than the real v1 about chains of triggered-ability targets, which is what made some boards (a sacrifice outlet beside Mayhem Devil) fifty times slower than the rest.
- Both AIs weigh choices that cannot differ only once (`ai/distinct.rs`): sacrificing or aiming at any one of several identical tokens is one choice. That changes no decision of v1's.
- `crates/sim` runs AI vs AI to find crashes and measure deck win rates. It plays every pairing of the decks in `data/decks/`, mirrors included, swapping seats each game, and reports win rates, game length, mulligans, land screw and flood, and how games ended. Its turn cap, its cap on permanents on the battlefield (a crowded stalemate is stopped like a game past the turn cap) and its thresholds are in `balance.json`. `--replay` prints one game move by move. `--enemies` plays each planeswalker's starting deck against every enemy deck, set up as the run sets a duel up. `--runs` plays whole runs, with the AI on both sides of each duel and a simple policy for the run's choices (the first two feats on offer, a wound healed at every rest), and reports how often the act is won, how many wounds are carried to the boss, and for each feat of the character's how often a duel that was won met each of its tiers, whether or not the feat was picked. The balance lab (`lab.rs`, the Warden only) adds `--lift` (single duels of random decks, and for each card and relic the win rate of the decks that held it less that of those that didn't), `--ideal` (the hand-built ideal decks of `data/sim/ideal_decks.json` against each enemy), `--leave-out` (one ideal deck with each card in turn swapped for a filler) and `--act` (ideal decks through whole acts, taking no card and no relic); `--runs` takes `--no-cards`, `--no-relics`, `--with-relic` and `--with-card`. `--data DIR` reads the game's data from a folder when the sim starts (`data::replace_embedded`; the engine itself still does no file IO), and `--threads N` caps how many games are played at once. `--ai old|new` picks the AI in every mode; `new`, the search, is the default. `--versus` plays one AI against another (`--ai` against `--against`) on the same pairings, each way round on separate seeds, and reports how often `--ai` wins with a margin of error, and how much each deck gains from being played by it; `--decks a,b,c` keeps a few decks for a quick sample. Every report ends with a timing table: decisions, average and worst time per decision, and time per game, for each AI, and where the slowest decision was. `tools/ai-timing.mjs` measures the same in WebAssembly, by playing whole runs through the wasm handle.
- `ai_choose_action_with(state, player, kind)` picks the AI (`AiKind`); `ai_choose_action` is v1. An AI's look-ahead applies actions with `apply_trusted`, which skips the legality check and the safety copy of `apply_action`: it is only for actions taken from `legal_actions`, on a copy that is thrown away.
- A `GameObject` remembers the card definition its `def` names, so the rules don't look it up by name each time. That is not part of the state: it is left out of saves and comparisons and found again by id when missing.
- The AI aims a triggered ability by trying each choice on a copy of the game and keeping the best, and it thinks the same choice through when it imagines casting the creature. Scrying, it bottoms a land it has no use for or a spell far from castable.
- Faced with a damage division, the AI confirms the automatic one. It only ever double-blocks to survive a menace attacker, so a human rarely gets to divide damage; better blocking is M4 work.

## 8. The wasm boundary

- `engine-wasm` owns the `DuelState` inside wasm memory and exposes a handle. State is never round-tripped through JavaScript.
- The handle is `Duel`, made with `new Duel(human_deck, ai_deck, seed)` from two deck ids. The human is always P1 and the AI P2. Its methods: `apply(action) -> DuelUpdate` for the human, `ai_step() -> DuelUpdate` for the opponent, plus `view()`, `legal_actions()`, `has_meaningful_action()`, `should_auto_pass(hold_priority)` and `seed()`. A `DuelUpdate` is `{ events, view }`, with the events already redacted for the human.
- A run has its own handle, `Run`, made with `new Run(planeswalker, emblem, seed)`; an empty `emblem` takes the character's usual starting emblem. It owns the `RunState` and with it the duel in progress. `view()` returns the `RunView` and `apply(action)` a `RunUpdate` (`{ events, view }`). The duel is reached through the same handle: `duel_view()`, `duel_legal_actions()`, `duel_should_auto_pass(hold_priority)`, `duel_apply(action)` and `duel_ai_step()`, which mirror `Duel`'s methods, and `duel_feats()`, how the feats picked for it stand. TypeScript never builds a duel or reports its result; it sends `finish_duel` and the engine reads the result from its own state.
- `duel_suggest()` returns what the AI would do in the human's place. The end-to-end tests use it (see §9); the player is not shown it.
- The AI behind `ai_step()`, `duel_ai_step()` and `duel_suggest()` is `session::GAME_AI`, the search. A decision that takes thought costs about 2 ms in WebAssembly and the slowest seen was 28 ms, so it runs on the page's own thread with no "thinking" indicator.
- `card_defs()`, `decks()` and `run_content()` return every card definition, decklist, and planeswalker, relic, wound, enemy, event and feat, so the UI can show names, costs and text.
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
  - `interaction.ts` maps clicks onto the list from `legal_actions`: which actions use a hand card, which targets can be picked next, which creature can attack or block, which permanent has an ability to activate and what it still needs picked (the creature to sacrifice, then the targets). A target that is a card in a graveyard is shown in the prompt bar to be clicked.
  - `prompt.ts` holds the wording: what the game is waiting for and what the buttons say.
- `web/src/run/` — wording for the run: what just happened, and the names of nodes.
- `web/src/screens/` — Menu (start a run, or a practice duel), Run and Duel. The run screen shows the map, the feats on offer before a duel, rewards, drafts, treasures, events, rest sites, shops and the end of the run from the `RunView`, offers the legal run actions and sends one back. The feats are ticked on the screen and sent as one `ChooseFeats` once the ticks are one of the legal sets. While the run is in a duel it shows the duel screen, with a controller driving the run handle's duel and a panel that asks the engine how the feats stand at every change.
- `web/src/components/` — Card, Battlefield, Hand, the stack and log panels, the overlays (mulligan, graveyard, result), the hover preview, the damage-division panel, the run's map and its deck view.
- `web/e2e/` — Playwright tests that click through the real app: whole duels, and whole acts. With `?hints` in the address the page exposes the engine's suggested duel action, and the act tests follow it by clicking the same things a player would, which lets them reach and beat the boss.
- State: the engine owns game state; React renders the controller's snapshot (the view on screen, the legal actions, the log and the effects being animated).
- Every legal action is reachable from the screen: card actions through the hand, declarations and targets through the battlefield, the stack and the player panels, and the rest (pass, confirm, reset, keep, mulligan) as buttons built from the legal-action list.
- The menu asks for a commander (the character) and then a starting emblem. In a duel the human's commander sits at the left end of the hand while it is in the command zone, with what it costs now, and is clicked like a hand card; each player's panel names their commander and says where it is.
- A run can be started from the address, e.g. `?run=warden&seed=7&speed=instant` (add `&emblem=emblem_fury` for a starting emblem other than the character's usual one); "Copy bug report" on the run screen copies the planeswalker, the seed and everything the player did.
- A practice duel can be started from the address, e.g. `?seed=7&you=deck_b&ai=deck_a&speed=instant`. The seed and the two decks are enough to replay a game; "Copy bug report" on the duel screen copies them with the human's actions.
- Placeholder card frames (text-based) until art is added.
