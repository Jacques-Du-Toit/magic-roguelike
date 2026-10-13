---
name: add-cards
description: Add a batch of real Magic cards to the card pool of this game (a package for a colour pair, a few cards for an enemy deck). Use when the owner has approved a list of cards to add.
---

# Adding cards to the pool

The owner approves the list first: cards are game design. Look each candidate up on Scryfall before proposing it, so the text you show is exact, and say which package or deck each card is for and which rule it needs.

## 1. Rules first

Read each card's text against `docs/ARCHITECTURE.md` section 4 (what the data format can say) and `docs/RULES_SCOPE.md`.

- Everything expressible already: go to step 2.
- A small extension of something built (a new trigger, a new count, a new condition): add the enum variant in `crates/engine/src/cards/mod.rs`, its rule in `crates/engine/src/duel/`, and its validation. Unknown fields fail to load, so the variant comes before the card.
- A rule marked Later or Out in `RULES_SCOPE.md`: ask the owner before building it, with a recommendation. A narrow exception is written into `RULES_SCOPE.md` by name.
- A card that would behave differently from its printed text is left out or asked about. If the owner wants the text changed, mark the file `"altered": true` and write our own `oracle_text`.

Rules of thumb that have come up:

- No two cards of one rarity where one is simply the better.
- A package's commons are plain enablers; cards get more unusual as they get rarer.
- Our mythic weight is zero: a card that is mythic in print is entered as `"rarity": "rare"`.
- Tokens may share a name (two kinds of Soldier); cards may not.

## 2. The cards

Write a spec file (in the scratchpad, not the repo) and run the batch tool from the repo root:

```
node tools/add-cards.mjs spec.json
```

```json
{
  "cards": {
    "Centaur Healer": {
      "abilities": [
        { "kind": "triggered", "trigger": { "type": "enters_battlefield", "subject": "self" },
          "effects": [{ "type": "gain_life", "player": "controller", "amount": 3 }] }
      ]
    },
    "Armada Wurm": { "rarity": "rare", "abilities": [] }
  },
  "tokens": [
    { "id": "wurm_token", "name": "Wurm", "colors": ["G"], "power": 5, "toughness": 5, "keywords": ["trample"] }
  ]
}
```

The tool fetches each card's facts from Scryfall (never type a cost or a text from memory), sets the spec's fields on the file and writes the tokens. An Aura needs `"enchant"`.

## 3. A test for every card

One test a card, in the `*_tests.rs` file of its package under `crates/engine/src/duel/` (`token_tests.rs`, `counter_tests.rs`, `lifegain_tests.rs`, `big_creature_tests.rs`, `enchantment_tests.rs`), with the helpers of `test_support.rs`. Test what the card does, what it leaves alone (the opponent's creatures, a later turn), and any new rule at its edges. A new test file is declared in `crates/engine/src/duel/mod.rs`.

Then update the pool's size in `the_card_pool_has_the_expected_size` (`crates/engine/src/data.rs`): creatures, other spells, tokens.

A card the Warden can be offered (green or white, not `reserved`) also needs an **upgraded side** (`DECISIONS.md` #72): an `"upgraded"` block in its file with only what differs (`docs/ARCHITECTURE.md` section 4), a row in the `SIDES` table of `crates/engine/src/duel/upgrade_tests.rs`, and a test there if it changes what the card does. `every_card_the_warden_can_hold_has_an_upgraded_side_and_no_other_card_has` fails until it has one. Propose the upgrade to the owner with the card: a common's is large, a rare's small, and only generic mana may change.

## 4. Check, measure, write up

```
.\tools\check-all.ps1 -Sim
```

That runs fmt, clippy, every test suite, the Scryfall check, the web build, the end-to-end tests and 300 whole runs. Report the sim's act win rates with their margin (about +/- 6 at 300 runs) and say what was not verified: nothing but the sim will have played the new cards.

Before committing, update:

- `docs/ARCHITECTURE.md` section 4, for anything new the data format can say;
- `docs/DECISIONS.md`, one row for the package: the cards, any rule exception, the spares;
- `docs/ROADMAP.md`, a short note for the slice with the counts and the sim numbers.

Commit with a message written to a file (`git commit -F`), and push.
