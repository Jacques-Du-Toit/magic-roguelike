//! The full state of a duel.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::action::{CardFace, EventCard};
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
    pub starting_player: PlayerId,
    /// What the game is waiting for, and from whom.
    pub decision: Option<Decision>,
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
    pub mulligans_taken: u8,
    pub kept_hand: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameObject {
    pub id: ObjectId,
    pub card: CardId,
    pub def: CardDefId,
    pub owner: PlayerId,
    pub zone: Zone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    Library,
    Hand,
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
}

impl Decision {
    pub fn player(&self) -> PlayerId {
        match *self {
            Decision::Mulligan { player } | Decision::BottomCards { player, .. } => player,
        }
    }
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
            starting_player,
            decision: None,
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

    /// The card definition behind an object.
    pub(crate) fn def(&self, id: ObjectId) -> Result<&'static CardDef, InternalError> {
        card_def(&self.object(id)?.def)
    }

    pub(crate) fn event_card(&self, id: ObjectId) -> Result<EventCard, InternalError> {
        let object = self.object(id)?;
        Ok(EventCard {
            object: id,
            face: Some(CardFace {
                card: object.card,
                def: object.def.clone(),
            }),
        })
    }

    fn zone_mut(&mut self, owner: PlayerId, zone: Zone) -> &mut Vec<ObjectId> {
        let player = self.player_mut(owner);
        match zone {
            Zone::Library => &mut player.library,
            Zone::Hand => &mut player.hand,
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
            },
        );
        self.zone_mut(owner, zone).insert(0, id);
        id
    }

    /// Moves a card to another zone. The card becomes a new object, and the
    /// new id is returned. A card moved to the library goes to the bottom.
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
                zone: to,
                ..old
            },
        );
        let list = self.zone_mut(owner, to);
        match to {
            Zone::Library => list.insert(0, new_id),
            Zone::Hand => list.push(new_id),
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
            for (zone, list) in [(Zone::Library, &state.library), (Zone::Hand, &state.hand)] {
                for id in list {
                    assert!(
                        listed.insert(*id, (player, zone)).is_none(),
                        "{id:?} is listed twice"
                    );
                }
            }
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
