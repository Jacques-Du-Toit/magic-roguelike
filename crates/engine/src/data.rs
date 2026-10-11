//! Loads and validates everything under `data/`: cards, decklists and
//! balance numbers.
//!
//! The files are embedded at compile time, so the same loader works natively
//! and in wasm and the engine does no file IO.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use include_dir::{Dir, include_dir};
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::balance::{self, Balance, BalanceError};
use crate::cards::{self, CardDef, CardDefId, CardError};
use crate::decks::{self, DeckDef, DeckError, DeckId};

static DATA_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../data");

static EMBEDDED: LazyLock<Result<GameData, DataError>> =
    LazyLock::new(|| GameData::load(&embedded_files()?));

/// The game data embedded in this build.
pub fn embedded() -> Result<&'static GameData, DataError> {
    EMBEDDED.as_ref().map_err(Clone::clone)
}

/// All card definitions, by id.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardDatabase {
    cards: BTreeMap<CardDefId, CardDef>,
}

impl CardDatabase {
    pub fn get(&self, id: &CardDefId) -> Option<&CardDef> {
        self.cards.get(id)
    }

    /// Every card, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &CardDef> {
        self.cards.values()
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameData {
    pub cards: CardDatabase,
    pub decks: BTreeMap<DeckId, DeckDef>,
    pub balance: Balance,
}

/// One JSON file: its name without the extension, and its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataFile<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

/// The raw contents of `data/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFiles<'a> {
    pub cards: Vec<DataFile<'a>>,
    pub decks: Vec<DataFile<'a>>,
    pub balance: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DataError {
    #[error("{0} is missing or is not UTF-8 text")]
    Missing(String),
    #[error("{file}: {message}")]
    Parse { file: String, message: String },
    #[error("{file}: id `{id}` must match the file name")]
    IdMismatch { file: String, id: String },
    #[error("{file}: {source}")]
    Card { file: String, source: CardError },
    #[error("{file}: the name `{name}` is already used by card `{other}`")]
    DuplicateName {
        file: String,
        name: String,
        other: CardDefId,
    },
    #[error("{file}: {source}")]
    Deck { file: String, source: DeckError },
    #[error("balance.json: {0}")]
    Balance(#[from] BalanceError),
}

impl GameData {
    /// Parses and validates a full set of data files.
    pub fn load(files: &DataFiles<'_>) -> Result<GameData, DataError> {
        let mut cards = CardDatabase::default();
        let mut names: BTreeMap<String, CardDefId> = BTreeMap::new();
        for source in &files.cards {
            let file = format!("cards/{}.json", source.name);
            let card: CardDef = parse(&file, source.text)?;
            if card.id.as_str() != source.name {
                return Err(DataError::IdMismatch {
                    file,
                    id: card.id.0,
                });
            }
            if let Err(source) = cards::validate(&card) {
                return Err(DataError::Card { file, source });
            }
            if let Some(other) = names.insert(card.name.clone(), card.id.clone()) {
                return Err(DataError::DuplicateName {
                    file,
                    name: card.name,
                    other,
                });
            }
            cards.cards.insert(card.id.clone(), card);
        }

        let mut decks = BTreeMap::new();
        for source in &files.decks {
            let file = format!("decks/{}.json", source.name);
            let deck: DeckDef = parse(&file, source.text)?;
            if deck.id.0 != source.name {
                return Err(DataError::IdMismatch {
                    file,
                    id: deck.id.0,
                });
            }
            if let Err(source) = decks::validate(&deck, &cards) {
                return Err(DataError::Deck { file, source });
            }
            decks.insert(deck.id.clone(), deck);
        }

        let balance: Balance = parse("balance.json", files.balance)?;
        balance::validate(&balance)?;

        Ok(GameData {
            cards,
            decks,
            balance,
        })
    }
}

fn parse<T: DeserializeOwned>(file: &str, text: &str) -> Result<T, DataError> {
    serde_json::from_str(text).map_err(|error| DataError::Parse {
        file: file.to_string(),
        message: error.to_string(),
    })
}

fn embedded_files() -> Result<DataFiles<'static>, DataError> {
    let balance = DATA_DIR
        .get_file("balance.json")
        .and_then(|file| file.contents_utf8())
        .ok_or_else(|| DataError::Missing("balance.json".to_string()))?;
    Ok(DataFiles {
        cards: embedded_json_files("cards")?,
        decks: embedded_json_files("decks")?,
        balance,
    })
}

/// The `.json` files directly inside one folder of `data/`, in name order.
fn embedded_json_files(folder: &str) -> Result<Vec<DataFile<'static>>, DataError> {
    let dir = DATA_DIR
        .get_dir(folder)
        .ok_or_else(|| DataError::Missing(format!("{folder}/")))?;
    let mut files = Vec::new();
    for file in dir.files() {
        let path = file.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let missing = || DataError::Missing(path.display().to_string());
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(missing)?;
        let text = file.contents_utf8().ok_or_else(missing)?;
        files.push(DataFile { name, text });
    }
    files.sort_by_key(|file| file.name);
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{AbilityDef, CardType, Effect, Keyword, PlayerRef, TargetSpec};

    const BEARS: &str = r#"{ "id": "grizzly_bears", "name": "Grizzly Bears", "mana_cost": "{1}{G}",
        "colors": ["G"], "types": ["Creature"], "power": 2, "toughness": 2, "rarity": "common" }"#;
    const FOREST: &str = r#"{ "id": "forest", "name": "Forest", "mana_cost": "", "types": ["Land"],
        "rarity": "common",
        "abilities": [{ "kind": "mana", "cost": { "tap": true }, "produces": "{G}" }] }"#;
    const DECK: &str = r#"{ "id": "bears", "name": "Bears",
        "cards": [{ "card": "grizzly_bears", "count": 2 }, { "card": "forest", "count": 1 }] }"#;
    const BALANCE: &str = r#"{ "duel": { "starting_life": 20, "opening_hand_size": 7,
        "max_hand_size": 7, "smoothing_candidates": 3,
        "fatigue": { "first_life_loss": 1, "increase": 1 } } }"#;

    fn files<'a>(cards: &[(&'a str, &'a str)], deck: &'a str, balance: &'a str) -> DataFiles<'a> {
        DataFiles {
            cards: cards
                .iter()
                .map(|&(name, text)| DataFile { name, text })
                .collect(),
            decks: vec![DataFile {
                name: "bears",
                text: deck,
            }],
            balance,
        }
    }

    fn load_with(cards: &[(&str, &str)], deck: &str, balance: &str) -> Result<GameData, DataError> {
        GameData::load(&files(cards, deck, balance))
    }

    const CARDS: [(&str, &str); 2] = [("grizzly_bears", BEARS), ("forest", FOREST)];

    // ---- Loader behaviour, on small hand-made data ----

    #[test]
    fn loads_valid_data() {
        let data = load_with(&CARDS, DECK, BALANCE).unwrap();
        assert_eq!(data.cards.len(), 2);
        let bears = &data.decks[&DeckId("bears".to_string())];
        assert_eq!(bears.size(), 3);
        assert_eq!(bears.count_of("grizzly_bears"), 2);
        assert_eq!(data.balance.duel.starting_life, 20);
    }

    #[test]
    fn reports_the_file_of_a_json_error() {
        let error = load_with(&[("grizzly_bears", "{ not json")], DECK, BALANCE).unwrap_err();
        assert!(
            matches!(&error, DataError::Parse { file, .. } if file == "cards/grizzly_bears.json"),
            "{error}"
        );
    }

    #[test]
    fn card_id_must_match_its_file_name() {
        assert_eq!(
            load_with(&[("bears", BEARS)], DECK, BALANCE),
            Err(DataError::IdMismatch {
                file: "cards/bears.json".to_string(),
                id: "grizzly_bears".to_string(),
            })
        );
    }

    #[test]
    fn invalid_card_is_reported_with_its_file() {
        let no_power = BEARS.replace("\"power\": 2,", "");
        assert_eq!(
            load_with(&[("grizzly_bears", &no_power)], DECK, BALANCE),
            Err(DataError::Card {
                file: "cards/grizzly_bears.json".to_string(),
                source: CardError::PowerToughness,
            })
        );
    }

    #[test]
    fn two_cards_cannot_share_a_name() {
        let copy = BEARS.replace("grizzly_bears", "other_bears");
        assert_eq!(
            load_with(
                &[("grizzly_bears", BEARS), ("other_bears", &copy)],
                DECK,
                BALANCE
            ),
            Err(DataError::DuplicateName {
                file: "cards/other_bears.json".to_string(),
                name: "Grizzly Bears".to_string(),
                other: CardDefId("grizzly_bears".to_string()),
            })
        );
    }

    #[test]
    fn rejects_bad_decklists() {
        let deck_error = |deck: &str| match load_with(&CARDS, deck, BALANCE) {
            Err(DataError::Deck { file, source }) => {
                assert_eq!(file, "decks/bears.json");
                source
            }
            other => panic!("expected a deck error, got {other:?}"),
        };
        let id = |card: &str| CardDefId(card.to_string());
        assert_eq!(
            deck_error(&DECK.replace("forest", "island")),
            DeckError::UnknownCard(id("island"))
        );
        assert_eq!(
            deck_error(&DECK.replace("\"count\": 1", "\"count\": 0")),
            DeckError::ZeroCount(id("forest"))
        );
        assert_eq!(
            deck_error(&DECK.replace("forest", "grizzly_bears")),
            DeckError::DuplicateEntry(id("grizzly_bears"))
        );
        assert_eq!(
            deck_error(r#"{ "id": "bears", "name": "Bears", "cards": [] }"#),
            DeckError::Empty
        );
        assert_eq!(
            deck_error(&DECK.replace("\"name\": \"Bears\"", "\"name\": \"\"")),
            DeckError::EmptyName
        );
    }

    #[test]
    fn deck_id_must_match_its_file_name() {
        assert_eq!(
            load_with(
                &CARDS,
                &DECK.replace("\"id\": \"bears\"", "\"id\": \"cubs\""),
                BALANCE
            ),
            Err(DataError::IdMismatch {
                file: "decks/bears.json".to_string(),
                id: "cubs".to_string(),
            })
        );
    }

    #[test]
    fn rejects_bad_balance() {
        assert_eq!(
            load_with(&CARDS, DECK, &BALANCE.replace("20", "0")),
            Err(DataError::Balance(BalanceError::StartingLife))
        );
        let error = load_with(&CARDS, DECK, "{}").unwrap_err();
        assert!(
            matches!(&error, DataError::Parse { file, .. } if file == "balance.json"),
            "{error}"
        );
    }

    // ---- The real files under data/ ----

    fn data() -> &'static GameData {
        embedded().unwrap()
    }

    fn card(id: &str) -> &'static CardDef {
        data()
            .cards
            .get(&CardDefId(id.to_string()))
            .unwrap_or_else(|| panic!("no card `{id}`"))
    }

    fn deck(id: &str) -> &'static DeckDef {
        &data().decks[&DeckId(id.to_string())]
    }

    #[test]
    fn all_embedded_data_loads_and_validates() {
        assert_eq!(embedded().map(|_| ()), Ok(()));
    }

    #[test]
    fn starter_set_is_complete() {
        let ids: Vec<&str> = data().cards.iter().map(|card| card.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "boggart_brute",
                "cancel",
                "centaur_courser",
                "colossal_dreadmaw",
                "divination",
                "elvish_visionary",
                "forest",
                "giant_growth",
                "giant_spider",
                "grizzly_bears",
                "healers_hawk",
                "hill_giant",
                "inspiring_overseer",
                "island",
                "lightning_strike",
                "llanowar_elves",
                "mountain",
                "murder",
                "pacifism",
                "plains",
                "raging_goblin",
                "savannah_lions",
                "serra_angel",
                "shock",
                "swamp",
                "typhoid_rats",
                "vampire_nighthawk",
                "wind_drake",
                "youthful_knight",
            ]
        );
    }

    /// Cost, power/toughness and keywords of every starter creature, as
    /// printed (checked against Scryfall).
    #[test]
    fn starter_creatures_match_their_printed_stats() {
        use Keyword::*;
        let expected: [(&str, &str, i32, i32, &[Keyword]); 17] = [
            ("savannah_lions", "{W}", 2, 1, &[]),
            ("healers_hawk", "{W}", 1, 1, &[Flying, Lifelink]),
            ("youthful_knight", "{1}{W}", 2, 1, &[FirstStrike]),
            ("inspiring_overseer", "{2}{W}", 2, 1, &[Flying]),
            ("serra_angel", "{3}{W}{W}", 4, 4, &[Flying, Vigilance]),
            ("wind_drake", "{2}{U}", 2, 2, &[Flying]),
            ("typhoid_rats", "{B}", 1, 1, &[Deathtouch]),
            (
                "vampire_nighthawk",
                "{1}{B}{B}",
                2,
                3,
                &[Flying, Deathtouch, Lifelink],
            ),
            ("raging_goblin", "{R}", 1, 1, &[Haste]),
            ("boggart_brute", "{2}{R}", 3, 2, &[Menace]),
            ("hill_giant", "{3}{R}", 3, 3, &[]),
            ("llanowar_elves", "{G}", 1, 1, &[]),
            ("elvish_visionary", "{1}{G}", 1, 1, &[]),
            ("grizzly_bears", "{1}{G}", 2, 2, &[]),
            ("centaur_courser", "{2}{G}", 3, 3, &[]),
            ("giant_spider", "{3}{G}", 2, 4, &[Reach]),
            ("colossal_dreadmaw", "{4}{G}{G}", 6, 6, &[Trample]),
        ];
        for (id, cost, power, toughness, keywords) in expected {
            let card = card(id);
            assert_eq!(card.types, [CardType::Creature], "{id}");
            assert_eq!(card.mana_cost.to_string(), cost, "{id}");
            assert_eq!(
                (card.power, card.toughness),
                (Some(power), Some(toughness)),
                "{id}"
            );
            assert_eq!(card.keywords, keywords, "{id}");
        }
        let creatures = data()
            .cards
            .iter()
            .filter(|card| card.is_creature())
            .count();
        assert_eq!(creatures, expected.len());
    }

    #[test]
    fn starter_noncreature_spells_have_their_printed_costs() {
        for (id, cost) in [
            ("pacifism", "{1}{W}"),
            ("divination", "{2}{U}"),
            ("cancel", "{1}{U}{U}"),
            ("murder", "{1}{B}{B}"),
            ("shock", "{R}"),
            ("lightning_strike", "{1}{R}"),
            ("giant_growth", "{G}"),
        ] {
            assert_eq!(card(id).mana_cost.to_string(), cost, "{id}");
        }
    }

    #[test]
    fn basic_lands_tap_for_their_colour() {
        for (id, mana) in [
            ("plains", "{W}"),
            ("island", "{U}"),
            ("swamp", "{B}"),
            ("mountain", "{R}"),
            ("forest", "{G}"),
        ] {
            let land = card(id);
            assert_eq!(land.types, [CardType::Land], "{id}");
            assert!(land.mana_cost.is_empty() && land.colors.is_empty(), "{id}");
            match land.abilities.as_slice() {
                [AbilityDef::Mana { cost, produces }] => {
                    assert!(cost.tap, "{id}");
                    assert_eq!(produces.to_string(), mana, "{id}");
                }
                other => panic!("{id}: unexpected abilities {other:?}"),
            }
        }
    }

    #[test]
    fn llanowar_elves_has_a_tap_mana_ability() {
        match card("llanowar_elves").abilities.as_slice() {
            [AbilityDef::Mana { cost, produces }] => {
                assert!(cost.tap);
                assert_eq!(produces.to_string(), "{G}");
            }
            other => panic!("unexpected abilities {other:?}"),
        }
    }

    fn spell_effects(id: &str) -> &'static [Effect] {
        match card(id).abilities.as_slice() {
            [AbilityDef::Spell { effects }] => effects,
            other => panic!("{id}: unexpected abilities {other:?}"),
        }
    }

    fn enters_effects(id: &str) -> &'static [Effect] {
        match card(id).abilities.as_slice() {
            [AbilityDef::Triggered { effects, .. }] => effects,
            other => panic!("{id}: unexpected abilities {other:?}"),
        }
    }

    #[test]
    fn starter_spells_have_their_printed_effects() {
        let any = TargetSpec::AnyTarget {};
        let creature = TargetSpec::Creature {};
        let you = PlayerRef::Controller;

        assert_eq!(card("shock").types, [CardType::Instant]);
        assert_eq!(
            spell_effects("shock"),
            [Effect::DealDamage {
                amount: 2,
                target: any
            }]
        );
        assert_eq!(card("lightning_strike").types, [CardType::Instant]);
        assert_eq!(
            spell_effects("lightning_strike"),
            [Effect::DealDamage {
                amount: 3,
                target: any
            }]
        );
        assert_eq!(card("murder").types, [CardType::Instant]);
        assert_eq!(
            spell_effects("murder"),
            [Effect::Destroy { target: creature }]
        );
        assert_eq!(card("giant_growth").types, [CardType::Instant]);
        assert_eq!(
            spell_effects("giant_growth"),
            [Effect::ModifyPtUntilEot {
                target: creature,
                power: 3,
                toughness: 3
            }]
        );
        assert_eq!(card("cancel").types, [CardType::Instant]);
        assert_eq!(
            spell_effects("cancel"),
            [Effect::CounterSpell {
                target: TargetSpec::Spell {}
            }]
        );
        assert_eq!(card("divination").types, [CardType::Sorcery]);
        assert_eq!(
            spell_effects("divination"),
            [Effect::DrawCards {
                player: you,
                amount: 2
            }]
        );
    }

    #[test]
    fn enters_triggers_have_their_printed_effects() {
        let you = PlayerRef::Controller;
        assert_eq!(
            enters_effects("elvish_visionary"),
            [Effect::DrawCards {
                player: you,
                amount: 1
            }]
        );
        // The printed card gains 1 life as well as drawing.
        assert_eq!(
            enters_effects("inspiring_overseer"),
            [
                Effect::GainLife {
                    player: you,
                    amount: 1
                },
                Effect::DrawCards {
                    player: you,
                    amount: 1
                }
            ]
        );
    }

    #[test]
    fn pacifism_is_an_aura_for_creatures() {
        let pacifism = card("pacifism");
        assert!(pacifism.is_aura());
        assert_eq!(pacifism.enchant, Some(TargetSpec::Creature {}));
    }

    #[test]
    fn test_decks_are_fifteen_spells_and_ten_lands() {
        assert_eq!(data().decks.len(), 3);
        for deck in data().decks.values() {
            let lands: u32 = deck
                .cards
                .iter()
                .filter(|entry| card(entry.card.as_str()).is_land())
                .map(|entry| entry.count)
                .sum();
            assert_eq!((deck.size(), lands), (25, 10), "{}", deck.id);
        }
    }

    #[test]
    fn test_decks_match_the_roadmap_lists() {
        let a = deck("deck_a");
        assert_eq!(a.count_of("llanowar_elves"), 2);
        assert_eq!(a.count_of("youthful_knight"), 2);
        assert_eq!((a.count_of("forest"), a.count_of("plains")), (5, 5));
        assert_eq!(a.cards.len(), 15);

        let b = deck("deck_b");
        for (id, count) in [
            ("typhoid_rats", 3),
            ("wind_drake", 3),
            ("murder", 3),
            ("vampire_nighthawk", 2),
            ("cancel", 2),
            ("divination", 2),
            ("island", 5),
            ("swamp", 5),
        ] {
            assert_eq!(b.count_of(id), count, "{id}");
        }

        let c = deck("deck_c");
        for id in [
            "raging_goblin",
            "boggart_brute",
            "hill_giant",
            "shock",
            "lightning_strike",
        ] {
            assert_eq!(c.count_of(id), 3, "{id}");
        }
        assert_eq!(c.count_of("mountain"), 10);
    }

    #[test]
    fn every_starter_card_is_in_a_test_deck() {
        for card in data().cards.iter() {
            let used = data()
                .decks
                .values()
                .any(|deck| deck.count_of(card.id.as_str()) > 0);
            assert!(used, "{} is in no deck", card.id);
        }
    }

    #[test]
    fn balance_has_the_duel_numbers() {
        let duel = &data().balance.duel;
        assert_eq!(duel.starting_life, 20);
        assert_eq!(duel.opening_hand_size, 7);
        assert_eq!(duel.max_hand_size, 7);
        assert_eq!(duel.smoothing_candidates, 3);
        assert_eq!(
            (duel.fatigue.first_life_loss, duel.fatigue.increase),
            (1, 1)
        );
    }
}
