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
use crate::run::{self, EnemyDef, ModifierDef, PlaneswalkerDef, RunData, RunDataError};

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
    pub run: RunData,
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
    pub run: RunFiles<'a>,
}

/// The raw contents of `data/run/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunFiles<'a> {
    pub planeswalkers: &'a str,
    pub relics: &'a str,
    pub enemies: &'a str,
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
    #[error("run/: {0}")]
    Run(#[from] RunDataError),
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

        let planeswalkers: Vec<PlaneswalkerDef> =
            parse("run/planeswalkers.json", files.run.planeswalkers)?;
        let relics: Vec<ModifierDef> = parse("run/relics.json", files.run.relics)?;
        let enemies: Vec<EnemyDef> = parse("run/enemies.json", files.run.enemies)?;
        let run = RunData {
            planeswalkers,
            relics,
            enemies,
        };
        run::validate(&run, &cards)?;

        Ok(GameData {
            cards,
            decks,
            balance,
            run,
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
    let run_file = |name: &str| {
        DATA_DIR
            .get_file(format!("run/{name}.json"))
            .and_then(|file| file.contents_utf8())
            .ok_or_else(|| DataError::Missing(format!("run/{name}.json")))
    };
    Ok(DataFiles {
        cards: embedded_json_files("cards")?,
        decks: embedded_json_files("decks")?,
        balance,
        run: RunFiles {
            planeswalkers: run_file("planeswalkers")?,
            relics: run_file("relics")?,
            enemies: run_file("enemies")?,
        },
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
    const BALANCE: &str = include_str!("../../../data/balance.json");
    const PLANESWALKERS: &str = r#"[{ "id": "ranger", "name": "Ranger", "description": "Bears.",
        "colors": ["G"], "spells": [{ "card": "grizzly_bears", "count": 3 }],
        "passive": { "id": "ranger_passive", "name": "Vigour", "text": "More life.",
            "effects": [{ "type": "starting_life", "who": "owner", "change": 1 }] } }]"#;
    const RELICS: &str = "[]";
    const ENEMIES: &str = r#"[
        { "id": "cubs", "name": "Cubs", "tier": "easy", "cards": [{ "card": "forest", "count": 2 }] },
        { "id": "bears", "name": "Bears", "tier": "normal", "cards": [{ "card": "forest", "count": 2 }] },
        { "id": "big_bears", "name": "Big Bears", "tier": "elite", "cards": [{ "card": "forest", "count": 2 }] },
        { "id": "bear_king", "name": "Bear King", "tier": "boss", "cards": [{ "card": "forest", "count": 2 }] }
    ]"#;
    const RUN: RunFiles<'static> = RunFiles {
        planeswalkers: PLANESWALKERS,
        relics: RELICS,
        enemies: ENEMIES,
    };
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
            run: RUN,
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
            load_with(
                &CARDS,
                DECK,
                &BALANCE.replace("\"starting_life\": 20", "\"starting_life\": 0")
            ),
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
    fn the_card_pool_is_complete() {
        let ids: Vec<&str> = data().cards.iter().map(|card| card.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "aerial_responder",
                "air_elemental",
                "angel_of_mercy",
                "assault_griffin",
                "barony_vampire",
                "boggart_brute",
                "cancel",
                "centaur_courser",
                "charging_monstrosaur",
                "colossal_dreadmaw",
                "coral_merfolk",
                "divination",
                "elvish_visionary",
                "fire_elemental",
                "forest",
                "giant_growth",
                "giant_spider",
                "gigantosaurus",
                "glory_seeker",
                "goblin_piker",
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
                "rakdos_guildgate",
                "revitalize",
                "rumbling_baloth",
                "savannah_lions",
                "selesnya_guildgate",
                "sentinel_spider",
                "serra_angel",
                "shock",
                "siege_mastodon",
                "snapping_drake",
                "stampeding_rhino",
                "swamp",
                "thundering_giant",
                "typhoid_rats",
                "vampire_nighthawk",
                "vampire_of_the_dire_moon",
                "vampire_spawn",
                "volcanic_hammer",
                "walking_corpse",
                "wind_drake",
                "youthful_knight",
            ]
        );
    }

    /// Cost, power/toughness and keywords of every creature, as printed
    /// (checked against Scryfall).
    #[test]
    fn creatures_match_their_printed_stats() {
        use Keyword::*;
        let expected: [(&str, &str, i32, i32, &[Keyword]); 37] = [
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
            ("goblin_piker", "{1}{R}", 2, 1, &[]),
            ("walking_corpse", "{1}{B}", 2, 2, &[]),
            ("glory_seeker", "{1}{W}", 2, 2, &[]),
            ("rumbling_baloth", "{2}{G}{G}", 4, 4, &[]),
            ("air_elemental", "{3}{U}{U}", 4, 4, &[Flying]),
            ("thundering_giant", "{3}{R}{R}", 4, 3, &[Haste]),
            ("fire_elemental", "{3}{R}{R}", 5, 4, &[]),
            ("stampeding_rhino", "{4}{G}", 4, 4, &[Trample]),
            ("sentinel_spider", "{3}{G}{G}", 4, 4, &[Reach, Vigilance]),
            ("assault_griffin", "{3}{W}", 3, 2, &[Flying]),
            ("siege_mastodon", "{4}{W}", 3, 5, &[]),
            ("snapping_drake", "{3}{U}", 3, 2, &[Flying]),
            ("coral_merfolk", "{1}{U}", 2, 1, &[]),
            ("barony_vampire", "{2}{B}", 3, 2, &[]),
            (
                "vampire_of_the_dire_moon",
                "{B}",
                1,
                1,
                &[Deathtouch, Lifelink],
            ),
            ("gigantosaurus", "{G}{G}{G}{G}{G}", 10, 10, &[]),
            ("charging_monstrosaur", "{4}{R}", 5, 5, &[Trample, Haste]),
            (
                "aerial_responder",
                "{1}{W}{W}",
                2,
                3,
                &[Flying, Vigilance, Lifelink],
            ),
            ("angel_of_mercy", "{4}{W}", 3, 3, &[Flying]),
            ("vampire_spawn", "{2}{B}", 2, 3, &[]),
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
            ("revitalize", "{1}{W}"),
            ("volcanic_hammer", "{1}{R}"),
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
        assert_eq!(card("volcanic_hammer").types, [CardType::Sorcery]);
        assert_eq!(
            spell_effects("volcanic_hammer"),
            [Effect::DealDamage {
                amount: 3,
                target: any
            }]
        );
        assert_eq!(card("revitalize").types, [CardType::Instant]);
        assert_eq!(
            spell_effects("revitalize"),
            [
                Effect::GainLife {
                    player: you,
                    amount: 3
                },
                Effect::DrawCards {
                    player: you,
                    amount: 1
                }
            ]
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
        assert_eq!(
            enters_effects("angel_of_mercy"),
            [Effect::GainLife {
                player: you,
                amount: 3
            }]
        );
        assert_eq!(
            enters_effects("vampire_spawn"),
            [
                Effect::LoseLife {
                    player: PlayerRef::Opponent,
                    amount: 2
                },
                Effect::GainLife {
                    player: you,
                    amount: 2
                }
            ]
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
    fn guildgates_enter_tapped_and_tap_for_either_colour() {
        use crate::cards::Color;
        for (id, colors) in [
            ("selesnya_guildgate", [Color::G, Color::W]),
            ("rakdos_guildgate", [Color::B, Color::R]),
        ] {
            let gate = card(id);
            assert_eq!(gate.types, [CardType::Land], "{id}");
            assert_eq!(gate.subtypes, ["Gate"], "{id}");
            assert!(gate.supertypes.is_empty(), "{id}");
            assert!(gate.enters_tapped, "{id}");
            assert_eq!(gate.mana_colors(), colors, "{id}");
        }
        assert!(!card("forest").enters_tapped);
    }

    /// A card nobody can ever meet is dead weight: every card is in a test
    /// deck, a starting deck or an enemy deck. Non-basic lands are the
    /// exception; they come from rewards and shops.
    #[test]
    fn every_card_is_in_some_deck() {
        let run = &data().run;
        for card in data().cards.iter() {
            let id = card.id.as_str();
            let in_entries = |entries: &[crate::decks::DeckEntry]| {
                entries.iter().any(|entry| entry.card.as_str() == id)
            };
            let used = data().decks.values().any(|deck| deck.count_of(id) > 0)
                || run.planeswalkers.iter().any(|def| in_entries(&def.spells))
                || run.enemies.iter().any(|def| in_entries(&def.cards));
            let reward_land = card.is_land() && card.supertypes.is_empty();
            assert!(used || reward_land, "{id} is in no deck");
        }
    }

    #[test]
    fn the_run_content_is_what_the_roadmap_asks_for() {
        use crate::run::EnemyTier;
        let run = &data().run;
        assert_eq!(run.planeswalkers.len(), 2);
        assert_eq!(run.relics.len(), 5);
        assert_eq!(data().balance.run.boons.len(), 4);
        let count = |tier| run.enemies_of(tier).count();
        assert_eq!(count(EnemyTier::Easy) + count(EnemyTier::Normal), 7);
        assert_eq!(count(EnemyTier::Elite), 2);
        assert_eq!(count(EnemyTier::Boss), 1);
        for planeswalker in &run.planeswalkers {
            let spells: u32 = planeswalker.spells.iter().map(|entry| entry.count).sum();
            assert_eq!(spells, 15, "{}", planeswalker.id);
            assert_eq!(planeswalker.colors.len(), 2, "{}", planeswalker.id);
        }
        for enemy in &run.enemies {
            let size: u32 = enemy.cards.iter().map(|entry| entry.count).sum();
            let lands: u32 = enemy
                .cards
                .iter()
                .filter(|entry| card(entry.card.as_str()).is_land())
                .map(|entry| entry.count)
                .sum();
            assert_eq!((size, lands), (25, 10), "{}", enemy.id);
        }
        let boss = run.enemies_of(EnemyTier::Boss).next().unwrap();
        assert_eq!(boss.modifiers.len(), 2);
    }

    #[test]
    fn rejects_bad_run_content() {
        let load = |planeswalkers: &str, relics: &str, enemies: &str| {
            let mut files = files(&CARDS, DECK, BALANCE);
            files.run = RunFiles {
                planeswalkers,
                relics,
                enemies,
            };
            GameData::load(&files).map(|_| ())
        };
        assert_eq!(load(PLANESWALKERS, RELICS, ENEMIES), Ok(()));
        assert_eq!(
            load("[]", RELICS, ENEMIES),
            Err(DataError::Run(RunDataError::NoPlaneswalkers))
        );
        assert_eq!(
            load(
                &PLANESWALKERS.replace("grizzly_bears", "forest"),
                RELICS,
                ENEMIES
            ),
            Err(DataError::Run(RunDataError::LandInSpells {
                id: "ranger".to_string(),
                card: CardDefId("forest".to_string())
            }))
        );
        assert_eq!(
            load(
                &PLANESWALKERS.replace("\"change\": 1", "\"change\": 0"),
                RELICS,
                ENEMIES
            ),
            Err(DataError::Run(RunDataError::ZeroChange(
                "ranger_passive".to_string()
            )))
        );
        assert_eq!(
            load(
                PLANESWALKERS,
                RELICS,
                &ENEMIES.replace("\"tier\": \"boss\"", "\"tier\": \"elite\"")
            ),
            Err(DataError::Run(RunDataError::NoEnemies(
                crate::run::EnemyTier::Boss
            )))
        );
        assert_eq!(
            load(
                PLANESWALKERS,
                RELICS,
                &ENEMIES.replace("big_bears", "bears")
            ),
            Err(DataError::Run(RunDataError::DuplicateId(
                "bears".to_string()
            )))
        );
        let gold_passive = PLANESWALKERS.replace(
            r#"{ "type": "starting_life", "who": "owner", "change": 1 }"#,
            r#"{ "type": "gold_per_win", "amount": 5 }"#,
        );
        assert_eq!(
            load(&gold_passive, RELICS, ENEMIES),
            Err(DataError::Run(RunDataError::GoldOnNonRelic(
                "ranger_passive".to_string()
            )))
        );
        let bad_relic = r#"[{ "id": "stone", "name": "Stone", "text": "Nothing.",
            "effects": [{ "type": "abilities", "abilities": [
                { "kind": "static", "type": "anthem", "affects": "enchanted",
                  "power": 1, "toughness": 1 }] }] }]"#;
        assert!(matches!(
            load(PLANESWALKERS, bad_relic, ENEMIES),
            Err(DataError::Run(RunDataError::Emblem { .. }))
        ));
        assert!(matches!(
            load(PLANESWALKERS, "{", ENEMIES),
            Err(DataError::Parse { file, .. }) if file == "run/relics.json"
        ));
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
