//! The full state of a duel.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::action::{CardFace, CardRef};
use super::config::RulesConfig;
use super::error::InternalError;
use super::ids::{CardId, ObjectId, PlayerId};
use super::rng::DuelRng;
use crate::cards::{CardDef, CardDefId};
use crate::data;

/// Everything about a duel in progress. It is plain data: it serialises, it
/// refers to card definitions by id, and all randomness comes from `rng`, so
/// the same seed and actions always give the same game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuelState {
    pub rules: RulesConfig,
    pub players: [PlayerState; 2],
    /// Every card in the duel, whatever zone it is in.
    pub objects: BTreeMap<ObjectId, GameObject>,
    /// Permanents, in the order they entered.
    pub battlefield: Vec<ObjectId>,
    pub starting_player: PlayerId,
    /// 0 until the mulligans are over, then counts every turn of either
    /// player.
    pub turn: u32,
    pub active_player: PlayerId,
    pub step: Step,
    /// How many players have passed priority in a row.
    pub consecutive_passes: u8,
    /// What the game is waiting for, and from whom. `None` once the game is
    /// over.
    pub decision: Option<Decision>,
    pub result: Option<GameResult>,
    pub rng: DuelRng,
    next_object_id: u32,
    next_card_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    pub life: i32,
    pub opening_hand_size: u8,
    /// The top of the library is the end of the list.
    pub library: Vec<ObjectId>,
    pub hand: Vec<ObjectId>,
    /// The most recent card is the end of the list.
    pub graveyard: Vec<ObjectId>,
    pub mulligans_taken: u8,
    pub kept_hand: bool,
    pub lands_played_this_turn: u8,
    /// How many times this player has drawn from an empty library.
    pub fatigue_draws: u32,
    /// Whether the life loss that took this player to 0 or less was fatigue.
    pub lost_to_fatigue: bool,
}

impl PlayerState {
    pub(crate) fn new(life: i32, opening_hand_size: u8) -> PlayerState {
        PlayerState {
            life,
            opening_hand_size,
            library: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            mulligans_taken: 0,
            kept_hand: false,
            lands_played_this_turn: 0,
            fatigue_draws: 0,
            lost_to_fatigue: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameObject {
    pub id: ObjectId,
    pub card: CardId,
    pub def: CardDefId,
    pub owner: PlayerId,
    pub zone: Zone,
    /// Only meaningful on the battlefield.
    pub tapped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    Library,
    Hand,
    Battlefield,
    Graveyard,
}

/// The steps of a turn, in order. The two main phases count as steps here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    PostcombatMain,
    End,
    Cleanup,
}

impl Step {
    /// The step after this one, or `None` at the end of the turn.
    pub fn next(self) -> Option<Step> {
        match self {
            Step::Untap => Some(Step::Upkeep),
            Step::Upkeep => Some(Step::Draw),
            Step::Draw => Some(Step::PrecombatMain),
            Step::PrecombatMain => Some(Step::PostcombatMain),
            Step::PostcombatMain => Some(Step::End),
            Step::End => Some(Step::Cleanup),
            Step::Cleanup => None,
        }
    }

    pub fn is_main(self) -> bool {
        matches!(self, Step::PrecombatMain | Step::PostcombatMain)
    }
}

/// A choice the game is waiting on. `legal_actions` lists the ways to answer
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decision {
    /// Keep the opening hand or mulligan.
    Mulligan { player: PlayerId },
    /// Put `remaining` more cards from hand on the bottom of the library.
    BottomCards { player: PlayerId, remaining: u8 },
    /// The player has priority.
    Priority { player: PlayerId },
    /// Discard `remaining` more cards, down to the maximum hand size.
    Discard { player: PlayerId, remaining: u8 },
}

impl Decision {
    pub fn player(&self) -> PlayerId {
        match *self {
            Decision::Mulligan { player }
            | Decision::BottomCards { player, .. }
            | Decision::Priority { player }
            | Decision::Discard { player, .. } => player,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameResult {
    Win {
        player: PlayerId,
        reason: WinReason,
    },
    /// Both players lost at the same time.
    Draw,
}

/// How the loser lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WinReason {
    /// The opponent's life reached 0.
    LifeZero,
    /// The opponent's life reached 0 by drawing from an empty library.
    Fatigue,
}

impl DuelState {
    pub(crate) fn new(
        rules: RulesConfig,
        players: [PlayerState; 2],
        starting_player: PlayerId,
        rng: DuelRng,
    ) -> DuelState {
        DuelState {
            rules,
            players,
            objects: BTreeMap::new(),
            battlefield: Vec::new(),
            starting_player,
            turn: 0,
            active_player: starting_player,
            step: Step::Untap,
            consecutive_passes: 0,
            decision: None,
            result: None,
            rng,
            next_object_id: 0,
            next_card_id: 0,
        }
    }

    pub fn player(&self, player: PlayerId) -> &PlayerState {
        &self.players[player.index()]
    }

    pub(crate) fn player_mut(&mut self, player: PlayerId) -> &mut PlayerState {
        &mut self.players[player.index()]
    }

    /// The player the game is waiting on, if any.
    pub fn waiting_on(&self) -> Option<PlayerId> {
        self.decision.map(|decision| decision.player())
    }

    pub(crate) fn object(&self, id: ObjectId) -> Result<&GameObject, InternalError> {
        self.objects
            .get(&id)
            .ok_or_else(|| InternalError::new(format!("no object {id:?}")))
    }

    pub(crate) fn object_mut(&mut self, id: ObjectId) -> Result<&mut GameObject, InternalError> {
        self.objects
            .get_mut(&id)
            .ok_or_else(|| InternalError::new(format!("no object {id:?}")))
    }

    /// The card definition behind an object.
    pub(crate) fn def(&self, id: ObjectId) -> Result<&'static CardDef, InternalError> {
        card_def(&self.object(id)?.def)
    }

    /// The object as an event or view shows it, face up.
    pub(crate) fn card_ref(&self, id: ObjectId) -> Result<CardRef, InternalError> {
        let object = self.object(id)?;
        Ok(CardRef {
            object: id,
            face: Some(CardFace {
                card: object.card,
                def: object.def.clone(),
            }),
        })
    }

    /// The permanents a player controls, in the order they entered.
    pub fn permanents_of(&self, player: PlayerId) -> impl Iterator<Item = &GameObject> {
        self.battlefield
            .iter()
            .filter_map(|id| self.objects.get(id))
            .filter(move |object| object.owner == player)
    }

    fn zone_mut(&mut self, owner: PlayerId, zone: Zone) -> &mut Vec<ObjectId> {
        match zone {
            Zone::Library => &mut self.player_mut(owner).library,
            Zone::Hand => &mut self.player_mut(owner).hand,
            Zone::Graveyard => &mut self.player_mut(owner).graveyard,
            Zone::Battlefield => &mut self.battlefield,
        }
    }

    fn next_object_id(&mut self) -> ObjectId {
        let id = ObjectId(self.next_object_id);
        self.next_object_id += 1;
        id
    }

    /// Adds a brand-new card to the bottom of its owner's library.
    pub(crate) fn create_card_in_library(&mut self, owner: PlayerId, def: CardDefId) -> ObjectId {
        let id = self.next_object_id();
        let card = CardId(self.next_card_id);
        self.next_card_id += 1;
        let zone = Zone::Library;
        self.objects.insert(
            id,
            GameObject {
                id,
                card,
                def,
                owner,
                zone,
                tapped: false,
            },
        );
        self.zone_mut(owner, zone).insert(0, id);
        id
    }

    /// Moves a card to another zone. The card becomes a new object with none
    /// of the old one's status, and the new id is returned. A card moved to
    /// the library goes to the bottom.
    pub(crate) fn move_object(
        &mut self,
        id: ObjectId,
        to: Zone,
    ) -> Result<ObjectId, InternalError> {
        let (owner, from) = {
            let object = self.object(id)?;
            (object.owner, object.zone)
        };
        let list = self.zone_mut(owner, from);
        let position = list
            .iter()
            .position(|listed| *listed == id)
            .ok_or_else(|| InternalError::new(format!("{id:?} is not in its zone {from:?}")))?;
        list.remove(position);
        let old = self
            .objects
            .remove(&id)
            .ok_or_else(|| InternalError::new(format!("no object {id:?}")))?;

        let new_id = self.next_object_id();
        self.objects.insert(
            new_id,
            GameObject {
                id: new_id,
                card: old.card,
                def: old.def,
                owner: old.owner,
                zone: to,
                tapped: false,
            },
        );
        let list = self.zone_mut(owner, to);
        match to {
            Zone::Library => list.insert(0, new_id),
            Zone::Hand | Zone::Battlefield | Zone::Graveyard => list.push(new_id),
        }
        Ok(new_id)
    }
}

/// Looks a card definition up in the embedded data.
pub(crate) fn card_def(id: &CardDefId) -> Result<&'static CardDef, InternalError> {
    data::embedded()
        .map_err(|error| InternalError::new(error.to_string()))?
        .cards
        .get(id)
        .ok_or_else(|| InternalError::new(format!("no card definition `{id}`")))
}

#[cfg(test)]
impl DuelState {
    /// Panics unless every object is in exactly one zone list, the one its
    /// own `zone` and `owner` say, and every card id is unique.
    pub(crate) fn assert_consistent(&self) {
        let mut listed = BTreeMap::new();
        for player in PlayerId::ALL {
            let state = self.player(player);
            for (zone, list) in [
                (Zone::Library, &state.library),
                (Zone::Hand, &state.hand),
                (Zone::Graveyard, &state.graveyard),
            ] {
                for id in list {
                    assert!(
                        listed.insert(*id, (player, zone)).is_none(),
                        "{id:?} is listed twice"
                    );
                }
            }
        }
        for id in &self.battlefield {
            let owner = self.objects[id].owner;
            assert!(
                listed.insert(*id, (owner, Zone::Battlefield)).is_none(),
                "{id:?} is listed twice"
            );
        }
        assert_eq!(listed.len(), self.objects.len());
        let mut cards = std::collections::BTreeSet::new();
        for (id, object) in &self.objects {
            assert_eq!(object.id, *id);
            assert_eq!(listed.get(id), Some(&(object.owner, object.zone)));
            assert!(cards.insert(object.card), "{:?} appears twice", object.card);
        }
    }
}
