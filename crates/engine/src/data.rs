//! Loads and validates everything under `data/`: cards, decklists and
//! balance numbers.
//!
//! The files are embedded at compile time, so the same loader works natively
//! and in wasm and the engine does no file IO.

use std::collections::BTreeMap;
use std::sync::{LazyLock, OnceLock};

use include_dir::{Dir, include_dir};
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::balance::{self, Balance, BalanceError};
use crate::cards::{self, CardDef, CardDefId, CardError, UPGRADE_MARK};
use crate::decks::{self, DeckDef, DeckError, DeckId};
use crate::duel::CardInstance;
use crate::run::{
    self, EnemyDef, EventDef, FeatDef, ModifierDef, PlaneswalkerDef, RunData, RunDataError,
};
static DATA_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../data");

static EMBEDDED: LazyLock<Result<GameData, DataError>> =
    LazyLock::new(|| GameData::load(&embedded_files()?));

/// Data loaded by the caller to be used in place of the embedded data.
static REPLACEMENT: OnceLock<GameData> = OnceLock::new();

/// The game data embedded in this build, or what `replace_embedded` put in
/// its place.
pub fn embedded() -> Result<&'static GameData, DataError> {
    match REPLACEMENT.get() {
        Some(data) => Ok(data),
        None => EMBEDDED.as_ref().map_err(Clone::clone),
    }
}

/// Uses `data` in place of the embedded data from now on, so that the sim
/// can try changed data files without a rebuild. It is for the start of a
/// program, before anything has read the data: it can be done once, and
/// `false` means it had been done already. The engine still does no file
/// IO: the caller reads the files.
pub fn replace_embedded(data: GameData) -> bool {
    REPLACEMENT.set(data).is_ok()
}

/// All card definitions, by id.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardDatabase {
    cards: BTreeMap<CardDefId, CardDef>,
    /// The upgraded sides of the cards that have one, by their own ids.
    upgraded: BTreeMap<CardDefId, CardDef>,
}

impl CardDatabase {
    /// A card or an upgraded side.
    pub fn get(&self, id: &CardDefId) -> Option<&CardDef> {
        self.cards.get(id).or_else(|| self.upgraded.get(id))
    }

    /// Every card, in id order. Upgraded sides are not among them: they
    /// are never offered, counted or guessed at as cards of their own.
    pub fn iter(&self) -> impl Iterator<Item = &CardDef> {
        self.cards.values()
    }

    /// Every upgraded side, in id order.
    pub fn upgraded_sides(&self) -> impl Iterator<Item = &CardDef> {
        self.upgraded.values()
    }

    /// The definition a card in a deck is played as: its upgraded side if
    /// it has been upgraded, the card itself otherwise.
    pub fn played_as(&self, instance: &CardInstance) -> Option<&CardDef> {
        let card = self.cards.get(&instance.card_id)?;
        if !instance.upgraded {
            return Some(card);
        }
        self.upgraded.get(card.upgrade.as_ref()?)
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
    /// Token definitions: card files that the loader marks as tokens.
    pub tokens: Vec<DataFile<'a>>,
    pub decks: Vec<DataFile<'a>>,
    pub balance: &'a str,
    pub run: RunFiles<'a>,
}

/// The raw contents of `data/run/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunFiles<'a> {
    pub planeswalkers: &'a str,
    pub emblems: &'a str,
    pub relics: &'a str,
    pub wounds: &'a str,
    pub enemies: &'a str,
    pub events: &'a str,
    pub feats: &'a str,
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
    #[error("{file}: {problem}")]
    Upgrade { file: String, problem: &'static str },
    #[error("{file}: {source}")]
    Deck { file: String, source: DeckError },
    #[error("{file}: `create_token` names `{token}`, which is not a file in tokens/")]
    UnknownToken { file: String, token: CardDefId },
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
        let sources = files
            .cards
            .iter()
            .map(|source| ("cards", false, source))
            .chain(files.tokens.iter().map(|source| ("tokens", true, source)));
        for (folder, token, source) in sources {
            let file = format!("{folder}/{}.json", source.name);
            let mut card: CardDef = parse(&file, source.text)?;
            card.token = token;
            if card.id.as_str() != source.name {
                return Err(DataError::IdMismatch {
                    file,
                    id: card.id.0,
                });
            }
            if let Err(source) = cards::validate(&card) {
                return Err(DataError::Card { file, source });
            }
            // Tokens may share a name, as in Magic: a Soldier with lifelink
            // and one without are both "Soldier".
            if !token && let Some(other) = names.insert(card.name.clone(), card.id.clone()) {
                return Err(DataError::DuplicateName {
                    file,
                    name: card.name,
                    other,
                });
            }
            // The file doesn't say these: an upgraded side is made here.
            card.upgrade = None;
            card.upgrade_of = None;
            if let Some(mut side) = card.upgraded_side() {
                if token || card.is_land() {
                    return Err(DataError::Upgrade {
                        file,
                        problem: "only a spell can have an upgraded side",
                    });
                }
                if let Err(source) = cards::validate(&side) {
                    return Err(DataError::Card {
                        file: format!("{file} (upgraded)"),
                        source,
                    });
                }
                // A new text over the same card is no upgrade.
                let mut same = side.clone();
                same.oracle_text.clone_from(&card.oracle_text);
                same.upgraded.clone_from(&card.upgraded);
                if same == card {
                    return Err(DataError::Upgrade {
                        file,
                        problem: "the upgraded side must change the card",
                    });
                }
                // The deck's lands are worked out from the card as it was.
                let colored = |def: &CardDef| def.mana_cost.colored().collect::<Vec<_>>();
                if colored(&side) != colored(&card) {
                    return Err(DataError::Upgrade {
                        file,
                        problem: "the upgraded side may change only the generic mana of the cost",
                    });
                }
                side.id = CardDefId(format!("{}{UPGRADE_MARK}", card.id));
                side.name = format!("{}{UPGRADE_MARK}", card.name);
                side.upgrade_of = Some(card.id.clone());
                card.upgrade = Some(side.id.clone());
                cards.upgraded.insert(side.id.clone(), side);
            }
            cards.cards.insert(card.id.clone(), card);
        }
        // Every token an effect creates has to exist.
        for card in cards.iter().chain(cards.upgraded_sides()) {
            for token in card.abilities.iter().flat_map(tokens_created) {
                if !cards.get(token).is_some_and(|def| def.token) {
                    return Err(DataError::UnknownToken {
                        file: format!(
                            "cards/{}.json",
                            card.upgrade_of.as_ref().unwrap_or(&card.id)
                        ),
                        token: token.clone(),
                    });
                }
            }
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
        let emblems: Vec<ModifierDef> = parse("run/emblems.json", files.run.emblems)?;
        let relics: Vec<ModifierDef> = parse("run/relics.json", files.run.relics)?;
        let wounds: Vec<ModifierDef> = parse("run/wounds.json", files.run.wounds)?;
        let enemies: Vec<EnemyDef> = parse("run/enemies.json", files.run.enemies)?;
        let events: Vec<EventDef> = parse("run/events.json", files.run.events)?;
        let feats: Vec<FeatDef> = parse("run/feats.json", files.run.feats)?;
        let run = RunData {
            planeswalkers,
            emblems,
            relics,
            wounds,
            enemies,
            events,
            feats,
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

/// The tokens an ability's effects create.
fn tokens_created(ability: &cards::AbilityDef) -> Vec<&CardDefId> {
    let effects = match ability {
        cards::AbilityDef::Spell { effects }
        | cards::AbilityDef::Triggered { effects, .. }
        | cards::AbilityDef::Activated { effects, .. } => effects.as_slice(),
        _ => &[],
    };
    effects
        .iter()
        .filter_map(|effect| match effect {
            cards::Effect::CreateToken { token, .. } => Some(token),
            _ => None,
        })
        .collect()
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
        tokens: embedded_json_files("tokens")?,
        decks: embedded_json_files("decks")?,
        balance,
        run: RunFiles {
            planeswalkers: run_file("planeswalkers")?,
            emblems: run_file("emblems")?,
            relics: run_file("relics")?,
            wounds: run_file("wounds")?,
            enemies: run_file("enemies")?,
            events: run_file("events")?,
            feats: run_file("feats")?,
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
    use crate::cards::{AbilityDef, CardType, Effect, Keyword, PlayerRef, Supertype, TargetSpec};

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
        "emblem": "ranger_passive" }]"#;
    const EMBLEMS: &str = r#"[{ "id": "ranger_passive", "name": "Vigour", "text": "More life.",
        "effects": [{ "type": "starting_life", "who": "owner", "change": 1 }] }]"#;
    const RELICS: &str = "[]";
    const WOUNDS: &str = r#"[{ "id": "scar", "name": "Scar", "text": "Less life.",
        "effects": [{ "type": "starting_life", "who": "owner", "change": -1 }] }]"#;
    const EVENTS: &str = "[]";
    const FEATS: &str = "[]";
    const ENEMIES: &str = r#"[
        { "id": "cubs", "name": "Cubs", "tier": "easy", "cards": [{ "card": "forest", "count": 2 }] },
        { "id": "bears", "name": "Bears", "tier": "normal", "cards": [{ "card": "forest", "count": 2 }] },
        { "id": "big_bears", "name": "Big Bears", "tier": "elite", "cards": [{ "card": "forest", "count": 2 }] },
        { "id": "bear_king", "name": "Bear King", "tier": "boss", "cards": [{ "card": "forest", "count": 2 }] }
    ]"#;
    const RUN: RunFiles<'static> = RunFiles {
        planeswalkers: PLANESWALKERS,
        emblems: EMBLEMS,
        relics: RELICS,
        wounds: WOUNDS,
        enemies: ENEMIES,
        events: EVENTS,
        feats: FEATS,
    };
    fn files<'a>(cards: &[(&'a str, &'a str)], deck: &'a str, balance: &'a str) -> DataFiles<'a> {
        DataFiles {
            cards: cards
                .iter()
                .map(|&(name, text)| DataFile { name, text })
                .collect(),
            tokens: Vec::new(),
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
    fn tokens_load_from_their_own_folder_and_stay_out_of_decks() {
        const ALARM: &str = r#"{ "id": "alarm", "name": "Alarm", "mana_cost": "{W}",
            "colors": ["W"], "types": ["Instant"], "rarity": "common",
            "abilities": [{ "kind": "spell",
                "effects": [{ "type": "create_token", "token": "soldier_token" }] }] }"#;
        const SOLDIER: &str = r#"{ "id": "soldier_token", "name": "Soldier", "mana_cost": "",
            "colors": ["W"], "types": ["Creature"], "power": 1, "toughness": 1,
            "rarity": "common" }"#;
        let soldier = CardDefId("soldier_token".to_string());
        let cards = [CARDS[0], CARDS[1], ("alarm", ALARM)];

        // The token an effect names has to be there.
        assert_eq!(
            load_with(&cards, DECK, BALANCE).map(|_| ()),
            Err(DataError::UnknownToken {
                file: "cards/alarm.json".to_string(),
                token: soldier.clone()
            })
        );
        let with_token = |deck: &'static str| {
            let mut files = files(&cards, deck, BALANCE);
            files.tokens = vec![DataFile {
                name: "soldier_token",
                text: SOLDIER,
            }];
            GameData::load(&files)
        };
        let data = with_token(DECK).unwrap();
        assert!(data.cards.get(&soldier).unwrap().token);
        assert!(
            !data
                .cards
                .get(&CardDefId("alarm".to_string()))
                .unwrap()
                .token
        );

        // A deck can't hold one, and a card file isn't one: the same file
        // in cards/ has a colour its cost doesn't explain.
        const TOKEN_DECK: &str = r#"{ "id": "bears", "name": "Bears",
            "cards": [{ "card": "soldier_token", "count": 2 }] }"#;
        assert_eq!(
            with_token(TOKEN_DECK).map(|_| ()),
            Err(DataError::Deck {
                file: "decks/bears.json".to_string(),
                source: DeckError::UnknownCard(soldier)
            })
        );
        let as_card = [CARDS[0], CARDS[1], ("soldier_token", SOLDIER)];
        assert!(matches!(
            load_with(&as_card, DECK, BALANCE),
            Err(DataError::Card {
                source: CardError::ColorsMismatch(_),
                ..
            })
        ));
    }

    fn with_upgrade(card: &str, upgraded: &str) -> String {
        let body = card.trim_end().trim_end_matches('}');
        format!("{body}, \"upgraded\": {upgraded} }}")
    }

    #[test]
    fn an_upgraded_side_is_a_card_of_its_own_that_is_never_listed() {
        let bears = with_upgrade(BEARS, r#"{ "oracle_text": "Bigger.", "power": 3 }"#);
        let data = load_with(&[("grizzly_bears", &bears), CARDS[1]], DECK, BALANCE).unwrap();
        let (id, side_id) = (
            CardDefId("grizzly_bears".to_string()),
            CardDefId("grizzly_bears+".to_string()),
        );
        let card = data.cards.get(&id).unwrap();
        let side = data.cards.get(&side_id).unwrap();
        assert_eq!(card.upgrade, Some(side_id.clone()));
        assert_eq!((card.power, card.upgrade_of.as_ref()), (Some(2), None));
        assert_eq!(side.name, "Grizzly Bears+");
        assert_eq!((side.power, side.toughness), (Some(3), Some(2)));
        assert_eq!(side.oracle_text, "Bigger.");
        assert_eq!(
            (side.upgrade_of.as_ref(), side.upgrade.as_ref()),
            (Some(&id), None)
        );
        // It is not a card to offer, count or guess at.
        assert_eq!(data.cards.len(), 2);
        assert!(data.cards.iter().all(|card| card.upgrade_of.is_none()));
        assert_eq!(data.cards.upgraded_sides().count(), 1);

        // A copy in a deck is played as one or the other.
        let mut copy = CardInstance::new(id.clone());
        assert_eq!(data.cards.played_as(&copy).unwrap().id, id);
        copy.upgraded = true;
        assert_eq!(data.cards.played_as(&copy).unwrap().id, side_id);
        // A card with no upgraded side has no upgraded copy.
        let mut forest = CardInstance::new(CardDefId("forest".to_string()));
        assert!(data.cards.played_as(&forest).is_some());
        forest.upgraded = true;
        assert!(data.cards.played_as(&forest).is_none());
    }

    #[test]
    fn an_upgraded_side_must_be_a_valid_card_that_differs() {
        let load = |card: &str, upgraded: &str| {
            let (name, base) = match card {
                "forest" => ("forest", FOREST),
                _ => ("grizzly_bears", BEARS),
            };
            let changed = with_upgrade(base, upgraded);
            let other = if name == "forest" { CARDS[0] } else { CARDS[1] };
            load_with(&[(name, &changed), other], DECK, BALANCE).map(|_| ())
        };
        let problem = |result: Result<(), DataError>| match result {
            Err(DataError::Upgrade { problem, .. }) => problem,
            other => panic!("expected an upgrade error, got {other:?}"),
        };
        // The same card under a new text is no upgrade; new stats under the
        // printed text are.
        assert!(problem(load("bears", r#"{ "oracle_text": "Bigger." }"#)).contains("change"));
        assert!(problem(load("bears", r#"{ "power": 2 }"#)).contains("change"));
        assert_eq!(load("bears", r#"{ "power": 3 }"#), Ok(()));
        // The coloured mana stays: the deck's lands are split by it.
        let cheaper = r#"{ "oracle_text": "Cheaper.", "mana_cost": "{2}" }"#;
        assert!(matches!(
            load("bears", cheaper),
            Err(DataError::Card {
                source: CardError::ColorsMismatch(_),
                ..
            })
        ));
        let other_colour = r#"{ "oracle_text": "Wider.", "mana_cost": "{G}{G}" }"#;
        assert!(problem(load("bears", other_colour)).contains("generic"));
        let land = r#"{ "oracle_text": "More." }"#;
        assert!(problem(load("forest", land)).contains("spell"));
        // It is validated like any card, and typos are errors.
        let no_power = r#"{ "oracle_text": "Odd.", "keywords": ["flying", "flying"] }"#;
        assert!(matches!(
            load("bears", no_power),
            Err(DataError::Card { .. })
        ));
        let typo = r#"{ "oracle_text": "Odd.", "powr": 3 }"#;
        assert!(matches!(load("bears", typo), Err(DataError::Parse { .. })));
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

    /// How many cards of each kind the pool holds, so that a card file lost
    /// or added by accident shows up here.
    #[test]
    fn the_card_pool_has_the_expected_size() {
        let count = |wanted: fn(&CardDef) -> bool| {
            data()
                .cards
                .iter()
                .filter(|card| !card.token && wanted(card))
                .count()
        };
        let basics = count(|card| card.supertypes.contains(&Supertype::Basic));
        let lands = count(|card| card.is_land());
        let creatures = count(|card| card.is_creature() && !card.is_legendary());
        let others = count(|card| !card.is_land() && !card.is_creature());
        // The commanders: never in a deck, a reward or a shop.
        let legends = count(|card| card.is_legendary());
        assert_eq!((basics, lands - basics), (5, 10));
        assert_eq!((creatures, others), (135, 72));
        assert_eq!(legends, 9);
        // 19 creature tokens and the wound.
        assert_eq!(data().cards.iter().filter(|card| card.token).count(), 20);
    }

    /// Cost, power/toughness and keywords of the M1 to M3 creatures, as
    /// printed. Every card, these and later ones, is checked against
    /// Scryfall by `tools/scryfall.mjs check`.
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
        let creature = TargetSpec::creature();
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
                toughness: 3,
                for_each: None
            }]
        );
        assert_eq!(card("cancel").types, [CardType::Instant]);
        assert_eq!(
            spell_effects("cancel"),
            [Effect::CounterSpell {
                target: TargetSpec::spell()
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
                    amount: 3,
                    for_each: None
                },
                Effect::DrawCards {
                    player: you,
                    amount: 1,
                    for_each: None
                }
            ]
        );
        assert_eq!(card("divination").types, [CardType::Sorcery]);
        assert_eq!(
            spell_effects("divination"),
            [Effect::DrawCards {
                player: you,
                amount: 2,
                for_each: None
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
                amount: 1,
                for_each: None
            }]
        );
        assert_eq!(
            enters_effects("angel_of_mercy"),
            [Effect::GainLife {
                player: you,
                amount: 3,
                for_each: None
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
                    amount: 2,
                    for_each: None
                }
            ]
        );
        // The printed card gains 1 life as well as drawing.
        assert_eq!(
            enters_effects("inspiring_overseer"),
            [
                Effect::GainLife {
                    player: you,
                    amount: 1,
                    for_each: None
                },
                Effect::DrawCards {
                    player: you,
                    amount: 1,
                    for_each: None
                }
            ]
        );
    }

    #[test]
    fn pacifism_is_an_aura_for_creatures() {
        let pacifism = card("pacifism");
        assert!(pacifism.is_aura());
        assert_eq!(pacifism.enchant, Some(TargetSpec::creature()));
    }

    #[test]
    fn test_decks_are_fifteen_spells_and_ten_lands() {
        assert!(data().decks.len() >= 3);
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
            ("azorius_guildgate", [Color::W, Color::U]),
            ("dimir_guildgate", [Color::U, Color::B]),
            ("gruul_guildgate", [Color::R, Color::G]),
            ("orzhov_guildgate", [Color::W, Color::B]),
            ("izzet_guildgate", [Color::U, Color::R]),
            ("golgari_guildgate", [Color::B, Color::G]),
            ("boros_guildgate", [Color::R, Color::W]),
            ("simic_guildgate", [Color::G, Color::U]),
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

    /// Whether a card is offered by rewards, drafts and shops to some
    /// character: a spell that is not legendary, in that character's
    /// colours.
    fn is_a_reward(card: &CardDef) -> bool {
        !card.is_land()
            && !card.is_legendary()
            && !card.colors.is_empty()
            && data().run.planeswalkers.iter().any(|planeswalker| {
                card.colors
                    .iter()
                    .all(|color| planeswalker.colors.contains(color))
            })
    }

    /// A card nobody can ever meet is dead weight: every card is in a test
    /// deck, a starting deck or an enemy deck. Non-basic lands are the
    /// exception (they come from rewards and shops), and so are tokens, the
    /// spells that rewards and drafts offer, and a legendary creature that
    /// leads no deck for now (a commander kept for later).
    #[test]
    fn every_card_is_in_some_deck() {
        let run = &data().run;
        for card in data().cards.iter().filter(|card| !card.token) {
            let id = card.id.as_str();
            let in_entries = |entries: &[crate::decks::DeckEntry]| {
                entries.iter().any(|entry| entry.card.as_str() == id)
            };
            let leads = |commander: &Option<CardDefId>| {
                commander.as_ref().is_some_and(|card| card.as_str() == id)
            };
            let used = data().decks.values().any(|deck| deck.count_of(id) > 0)
                || run.planeswalkers.iter().any(|def| in_entries(&def.spells))
                || run.enemies.iter().any(|def| in_entries(&def.cards))
                || run.planeswalkers.iter().any(|def| leads(&def.commander))
                || run.enemies.iter().any(|def| leads(&def.commander));
            let reward_land = card.is_land() && card.supertypes.is_empty();
            assert!(
                used || reward_land || is_a_reward(card) || card.is_legendary(),
                "{id} is in no deck"
            );
        }
    }

    /// The run itself shows every spell: each is in an enemy deck or a
    /// starting deck, not only in a test deck.
    #[test]
    fn every_spell_is_met_in_a_run() {
        let run = &data().run;
        for card in data()
            .cards
            .iter()
            .filter(|card| !card.token && !card.is_land() && !card.is_legendary())
        {
            let id = card.id.as_str();
            let in_entries = |entries: &[crate::decks::DeckEntry]| {
                entries.iter().any(|entry| entry.card.as_str() == id)
            };
            let met = run.planeswalkers.iter().any(|def| in_entries(&def.spells))
                || run.enemies.iter().any(|def| in_entries(&def.cards));
            assert!(met || is_a_reward(card), "{id} is in no run deck or reward");
        }
    }

    #[test]
    fn the_run_content_is_what_the_roadmap_asks_for() {
        use crate::run::EnemyTier;
        let run = &data().run;
        assert_eq!(run.planeswalkers.len(), 2);
        // Fifteen relics, two of them held back from every offer.
        assert_eq!(run.relics.len(), 15);
        assert_eq!(run.relics.iter().filter(|relic| relic.reserved).count(), 2);
        assert_eq!(run.events.len(), 5);
        assert_eq!(run.wounds.len(), 1);
        let count = |tier| run.enemies_of(tier).count();
        assert_eq!((count(EnemyTier::Easy), count(EnemyTier::Normal)), (5, 5));
        assert_eq!((count(EnemyTier::Hard), count(EnemyTier::Elite)), (4, 4));
        assert_eq!(count(EnemyTier::Boss), 1);
        for planeswalker in &run.planeswalkers {
            let spells: u32 = planeswalker.spells.iter().map(|entry| entry.count).sum();
            // The Warden starts lean, to be shaped by drafts.
            let expected = if planeswalker.id.as_str() == "warden" {
                10
            } else {
                15
            };
            assert_eq!(spells, expected, "{}", planeswalker.id);
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
        // Commanders: every character, elite and boss has one, and nobody
        // else does.
        assert!(run.emblems.is_empty());
        assert!(run.planeswalkers.iter().all(|def| def.commander.is_some()));
        for enemy in &run.enemies {
            let leads = matches!(enemy.tier, EnemyTier::Elite | EnemyTier::Boss);
            assert_eq!(enemy.commander.is_some(), leads, "{}", enemy.id);
        }
    }

    const LEGEND: &str = r#"{ "id": "bear_lord", "name": "Bear Lord", "mana_cost": "{2}{G}",
        "colors": ["G"], "supertypes": ["Legendary"], "types": ["Creature"],
        "power": 3, "toughness": 3, "rarity": "rare" }"#;
    const RED_LEGEND: &str = r#"{ "id": "fire_lord", "name": "Fire Lord", "mana_cost": "{2}{R}",
        "colors": ["R"], "supertypes": ["Legendary"], "types": ["Creature"],
        "power": 3, "toughness": 3, "rarity": "rare" }"#;

    #[test]
    fn a_commander_must_be_a_legendary_creature_in_the_decks_colours() {
        let load = |planeswalkers: &str, enemies: &str| {
            let mut files = files(
                &[
                    ("grizzly_bears", BEARS),
                    ("forest", FOREST),
                    ("bear_lord", LEGEND),
                    ("fire_lord", RED_LEGEND),
                ],
                DECK,
                BALANCE,
            );
            files.run = RunFiles {
                planeswalkers,
                enemies,
                ..RUN
            };
            GameData::load(&files).map(|_| ())
        };
        let led_by = |card: &str| {
            PLANESWALKERS.replace(
                r#""emblem":"#,
                &format!(r#""commander": "{card}", "emblem":"#),
            )
        };
        let bears_led_by = |card: &str| {
            ENEMIES.replace(
                r#""tier": "normal", "cards": [{ "card": "forest", "count": 2 }]"#,
                &format!(
                    r#""tier": "normal", "commander": "{card}",
                    "cards": [{{ "card": "grizzly_bears", "count": 2 }}]"#
                ),
            )
        };
        assert_eq!(
            load(&led_by("bear_lord"), &bears_led_by("bear_lord")),
            Ok(())
        );
        let bad = |card: &str| {
            Err(DataError::Run(RunDataError::BadCommander {
                id: "ranger".to_string(),
                card: CardDefId(card.to_string()),
            }))
        };
        assert_eq!(
            load(&led_by("grizzly_bears"), ENEMIES),
            bad("grizzly_bears")
        );
        assert_eq!(load(&led_by("nobody"), ENEMIES), bad("nobody"));
        assert_eq!(
            load(&led_by("fire_lord"), ENEMIES),
            Err(DataError::Run(RunDataError::CommanderColors {
                id: "ranger".to_string(),
                card: CardDefId("fire_lord".to_string()),
            }))
        );
        assert_eq!(
            load(PLANESWALKERS, &bears_led_by("fire_lord")),
            Err(DataError::Run(RunDataError::CommanderColors {
                id: "bears".to_string(),
                card: CardDefId("fire_lord".to_string()),
            }))
        );
        // A legendary card leads a deck; it is never in one.
        assert_eq!(
            load(
                &PLANESWALKERS.replace("grizzly_bears", "bear_lord"),
                ENEMIES
            ),
            Err(DataError::Run(RunDataError::BadEntry {
                id: "ranger".to_string(),
                card: CardDefId("bear_lord".to_string()),
            }))
        );
    }

    #[test]
    fn rejects_bad_run_content() {
        let load_with = |planeswalkers: &str, emblems: &str, relics: &str, enemies: &str| {
            let mut files = files(&CARDS, DECK, BALANCE);
            files.run = RunFiles {
                planeswalkers,
                emblems,
                relics,
                enemies,
                ..RUN
            };
            GameData::load(&files).map(|_| ())
        };
        let load = |planeswalkers: &str, relics: &str, enemies: &str| {
            load_with(planeswalkers, EMBLEMS, relics, enemies)
        };
        // A character need not have a starting emblem, and there need be none.
        assert_eq!(
            load_with(
                &PLANESWALKERS.replace("\"emblem\": \"ranger_passive\"", "\"commander\": null"),
                "[]",
                RELICS,
                ENEMIES
            ),
            Ok(())
        );
        assert_eq!(
            load(
                &PLANESWALKERS.replace("\"emblem\": \"ranger_passive\"", "\"emblem\": \"none\""),
                RELICS,
                ENEMIES
            ),
            Err(DataError::Run(RunDataError::UnknownEmblem {
                id: "ranger".to_string(),
                emblem: crate::run::ModifierId("none".to_string()),
            }))
        );
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
            load_with(
                PLANESWALKERS,
                &EMBLEMS.replace("\"change\": 1", "\"change\": 0"),
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
        let gold_passive = EMBLEMS.replace(
            r#"{ "type": "starting_life", "who": "owner", "change": 1 }"#,
            r#"{ "type": "gold_per_win", "amount": 5 }"#,
        );
        assert_eq!(
            load_with(PLANESWALKERS, &gold_passive, RELICS, ENEMIES),
            Err(DataError::Run(RunDataError::RunEffectOnNonRelic(
                "ranger_passive".to_string()
            )))
        );
        // Lower prices are a relic's business too, and stay within reason.
        let relic_with = |effect: &str| {
            format!(
                r#"[{{ "id": "stone", "name": "Stone", "text": "Something.",
                "effects": [{effect}] }}]"#
            )
        };
        let discount =
            |percent: u32| format!(r#"{{ "type": "shop_discount", "percent": {percent} }}"#);
        assert_eq!(
            load_with(
                PLANESWALKERS,
                &EMBLEMS.replace(
                    r#"{ "type": "starting_life", "who": "owner", "change": 1 }"#,
                    &discount(20),
                ),
                RELICS,
                ENEMIES
            ),
            Err(DataError::Run(RunDataError::RunEffectOnNonRelic(
                "ranger_passive".to_string()
            )))
        );
        assert_eq!(
            load(PLANESWALKERS, &relic_with(&discount(20)), ENEMIES),
            Ok(())
        );
        for percent in [0, 101] {
            assert_eq!(
                load(PLANESWALKERS, &relic_with(&discount(percent)), ENEMIES),
                Err(DataError::Run(RunDataError::BadDiscount(
                    "stone".to_string()
                )))
            );
        }
        // A token a relic starts a duel with, or creates, has to exist.
        let unknown = |token: &str| {
            Err(DataError::Run(RunDataError::UnknownToken {
                id: "stone".to_string(),
                token: CardDefId(token.to_string()),
            }))
        };
        assert_eq!(
            load(
                PLANESWALKERS,
                &relic_with(r#"{ "type": "starting_token", "token": "soldier_token" }"#),
                ENEMIES
            ),
            unknown("soldier_token")
        );
        // A card is not a token.
        assert_eq!(
            load(
                PLANESWALKERS,
                &relic_with(r#"{ "type": "starting_token", "token": "grizzly_bears" }"#),
                ENEMIES
            ),
            unknown("grizzly_bears")
        );
        assert_eq!(
            load(
                PLANESWALKERS,
                &relic_with(
                    r#"{ "type": "abilities", "abilities": [{ "kind": "triggered",
                    "trigger": { "type": "beginning_of_upkeep" },
                    "effects": [{ "type": "create_token", "token": "ghost_token" }] }] }"#
                ),
                ENEMIES
            ),
            unknown("ghost_token")
        );
        assert_eq!(
            load(
                PLANESWALKERS,
                &relic_with(
                    r#"{ "type": "starting_token", "token": "soldier_token", "count": 0 }"#
                ),
                ENEMIES
            ),
            Err(DataError::Run(RunDataError::ZeroChange(
                "stone".to_string()
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
    fn rejects_bad_events() {
        let load = |events: &str| {
            let mut files = files(&CARDS, DECK, BALANCE);
            files.run = RunFiles { events, ..RUN };
            GameData::load(&files).map(|_| ())
        };
        let event = |shown: &str, options: &str| {
            format!(
                r#"[{{ "id": "fork", "name": "A Fork", "text": "Two roads."{shown},
                "options": {options} }}]"#
            )
        };
        let option =
            |effects: &str| format!(r#"[{{ "text": "Go left.", "effects": [{effects}] }}]"#);
        let wound = |effect: &str| {
            format!(
                r#"{{ "type": "next_duel", "modifier": {{ "id": "wound", "name": "Wound",
                "text": "Ouch.", "effects": [{effect}] }} }}"#
            )
        };
        let life = r#"{ "type": "starting_life", "who": "owner", "change": -5 }"#;
        let run = |error: RunDataError| Err(DataError::Run(error));
        let fork = || "fork".to_string();

        let gold = r#"{ "type": "gain_gold", "amount": 5 }"#;
        assert_eq!(load(&event("", &option(gold))), Ok(()));
        assert_eq!(
            load(&event(
                r#", "relics": 2"#,
                &option(r#"{ "type": "gain_relic" }"#)
            )),
            Ok(())
        );
        assert_eq!(load(&event("", &option(&wound(life)))), Ok(()));

        assert_eq!(load(&event("", "[]")), run(RunDataError::NoOptions(fork())));
        assert_eq!(
            load(&event("", &option(""))),
            run(RunDataError::NoOptions(fork()))
        );
        assert_eq!(
            load(&event(
                "",
                &option(r#"{ "type": "pay_gold", "amount": 0 }"#)
            )),
            run(RunDataError::ZeroChange(fork()))
        );
        assert_eq!(
            load(&event("", &option(r#"{ "type": "take_wound" }"#))),
            Ok(())
        );
        // A relic or cards to give have to be shown first.
        assert_eq!(
            load(&event("", &option(r#"{ "type": "gain_relic" }"#))),
            run(RunDataError::NothingShown(fork()))
        );
        assert_eq!(
            load(&event("", &option(r#"{ "type": "gain_cards" }"#))),
            run(RunDataError::NothingShown(fork()))
        );
        assert_eq!(
            load(&event(
                r#", "relics": 2"#,
                &option(r#"{ "type": "gain_relic" }, { "type": "transmute" }"#)
            )),
            run(RunDataError::TwoPicks(fork()))
        );
        // What it leaves for the next duel is a modifier like any other,
        // and can't reach into the run.
        assert_eq!(
            load(&event(
                "",
                &option(&wound(r#"{ "type": "gold_per_win", "amount": 5 }"#))
            )),
            run(RunDataError::RunEffectOnNonRelic("wound".to_string()))
        );
        assert_eq!(
            load(&event(
                "",
                &option(&format!("{}, {}", wound(life), wound(life)))
            )),
            run(RunDataError::DuplicateId("wound".to_string()))
        );
        assert!(matches!(
            load("{"),
            Err(DataError::Parse { file, .. }) if file == "run/events.json"
        ));
    }

    #[test]
    fn rejects_bad_feats() {
        let load = |feats: &str| {
            let mut files = files(&CARDS, DECK, BALANCE);
            files.run = RunFiles { feats, ..RUN };
            GameData::load(&files).map(|_| ())
        };
        let feat = |text: &str, measure: &str, tiers: &str, more: &str| {
            format!(
                r#"[{{ "id": "feat", "name": "Feat", "text": "{text}",
                "measure": "{measure}", "tiers": {tiers}{more} }}]"#
            )
        };
        let run = |error: RunDataError| Err(DataError::Run(error));
        let id = || "feat".to_string();

        assert_eq!(
            load(&feat("Reach {n}.", "greatest_power", "[5, 7, 10]", "")),
            Ok(())
        );
        assert_eq!(
            load(&feat(
                "Reach {n} soon.",
                "greatest_power",
                "[5]",
                r#", "by_turn": 4, "planeswalkers": ["ranger"]"#
            )),
            Ok(())
        );
        // Fewer is better for some measures, and the tiers run that way.
        assert_eq!(
            load(&feat("Win by turn {n}.", "turns_taken", "[9, 7, 6]", "")),
            Ok(())
        );
        assert_eq!(
            load(&feat("Win by turn {n}.", "turns_taken", "[6, 7]", "")),
            run(RunDataError::FeatTiers(id()))
        );
        assert_eq!(
            load(&feat("Reach {n}.", "greatest_power", "[5, 5]", "")),
            run(RunDataError::FeatTiers(id()))
        );
        assert_eq!(
            load(&feat("Reach {n}.", "greatest_power", "[]", "")),
            run(RunDataError::FeatTiers(id()))
        );
        assert_eq!(
            load(&feat("Reach {n}.", "greatest_power", "[1, 2, 3, 4]", "")),
            run(RunDataError::FeatTiers(id()))
        );
        assert_eq!(
            load(&feat("Be big.", "greatest_power", "[5]", "")),
            run(RunDataError::FeatText(id()))
        );
        // A deadline is for a high point, not for the whole duel.
        assert_eq!(
            load(&feat(
                "Stay at {n}.",
                "lowest_life",
                "[12]",
                r#", "by_turn": 4"#
            )),
            run(RunDataError::FeatDeadline(id()))
        );
        assert_eq!(
            load(&feat(
                "Reach {n}.",
                "greatest_power",
                "[5]",
                r#", "planeswalkers": ["nobody"]"#
            )),
            run(RunDataError::UnknownPlaneswalker {
                id: id(),
                planeswalker: crate::run::PlaneswalkerId("nobody".to_string()),
            })
        );
        assert!(matches!(
            load(&feat("Reach {n}.", "biggest_hat", "[5]", "")),
            Err(DataError::Parse { file, .. }) if file == "run/feats.json"
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
