//! What one player is allowed to see: the view of the state the UI renders,
//! and events with hidden information taken out.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::action::{Action, CardFace, CardRef, GameEvent, Target};
use super::error::InternalError;
use super::ids::{CardId, ObjectId, PlayerId};
use super::legal_actions;
use super::mana::ManaPool;
use super::state::{AbilityRef, Combat, Decision, DuelState, GameResult, StackItemKind, Step};
use crate::cards::CardDefId;

/// The duel as one player sees it. The opponent's hand is face down and
/// neither library's contents or order appear at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlayerView {
    pub viewer: PlayerId,
    pub turn: u32,
    pub active_player: PlayerId,
    pub starting_player: PlayerId,
    pub step: Step,
    /// What the game is waiting for, and from whom.
    pub decision: Option<Decision>,
    /// Indexed like `PlayerId`: P1 first.
    pub players: [PlayerSummary; 2],
    /// In the order the permanents entered.
    pub battlefield: Vec<PermanentView>,
    /// The top of the stack is the end of the list.
    pub stack: Vec<StackItemView>,
    pub combat: Option<Combat>,
    pub result: Option<GameResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PlayerSummary {
    pub player: PlayerId,
    pub life: i32,
    /// Face up for the viewer's own hand, face down for the opponent's.
    pub hand: Vec<CardRef>,
    pub library_size: u32,
    /// The most recent card is the end of the list.
    pub graveyard: Vec<CardRef>,
    pub mana_pool: ManaPool,
    pub lands_played_this_turn: u8,
    pub mulligans_taken: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PermanentView {
    pub object: ObjectId,
    pub card: CardId,
    pub def: CardDefId,
    pub controller: PlayerId,
    pub tapped: bool,
    pub summoning_sick: bool,
    /// Current power and toughness, for creatures.
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub damage: i32,
    /// For an Aura, the permanent it enchants.
    pub attached_to: Option<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StackItemView {
    pub id: ObjectId,
    pub controller: PlayerId,
    pub kind: StackItemViewKind,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StackItemViewKind {
    Spell {
        card: CardFace,
    },
    /// A triggered ability of the card `ability.def`.
    Ability {
        ability: AbilityRef,
    },
}

/// Builds the view of the duel for one player.
pub fn player_view(state: &DuelState, viewer: PlayerId) -> Result<PlayerView, InternalError> {
    let summary = |player: PlayerId| -> Result<PlayerSummary, InternalError> {
        let player_state = state.player(player);
        let mut hand = Vec::new();
        for &id in &player_state.hand {
            hand.push(if player == viewer {
                state.card_ref(id)?
            } else {
                CardRef {
                    object: id,
                    face: None,
                }
            });
        }
        let mut graveyard = Vec::new();
        for &id in &player_state.graveyard {
            graveyard.push(state.card_ref(id)?);
        }
        Ok(PlayerSummary {
            player,
            life: player_state.life,
            hand,
            library_size: u32::try_from(player_state.library.len()).unwrap_or(u32::MAX),
            graveyard,
            mana_pool: player_state.mana_pool.clone(),
            lands_played_this_turn: player_state.lands_played_this_turn,
            mulligans_taken: player_state.mulligans_taken,
        })
    };

    let mut battlefield = Vec::new();
    for &id in &state.battlefield {
        let permanent = state.object(id)?;
        let is_creature = state.is_creature(id)?;
        battlefield.push(PermanentView {
            object: id,
            card: permanent.card,
            def: permanent.def.clone(),
            controller: permanent.owner,
            tapped: permanent.tapped,
            summoning_sick: permanent.summoning_sick,
            power: if is_creature {
                Some(state.power(id)?)
            } else {
                None
            },
            toughness: if is_creature {
                Some(state.toughness(id)?)
            } else {
                None
            },
            damage: permanent.damage,
            attached_to: permanent.attached_to,
        });
    }

    let mut stack = Vec::new();
    for item in &state.stack {
        let kind = match &item.kind {
            StackItemKind::Spell => {
                let spell = state.object(item.id)?;
                StackItemViewKind::Spell {
                    card: CardFace {
                        card: spell.card,
                        def: spell.def.clone(),
                    },
                }
            }
            StackItemKind::Trigger { ability } => StackItemViewKind::Ability {
                ability: ability.clone(),
            },
        };
        stack.push(StackItemView {
            id: item.id,
            controller: item.controller,
            kind,
            targets: item.targets.clone(),
        });
    }

    Ok(PlayerView {
        viewer,
        turn: state.turn,
        active_player: state.active_player,
        starting_player: state.starting_player,
        step: state.step,
        decision: state.decision,
        players: [summary(PlayerId::P1)?, summary(PlayerId::P2)?],
        battlefield,
        stack,
        combat: state.combat.clone(),
        result: state.result,
    })
}

/// Removes what `viewer` isn't allowed to know from a list of events: which
/// cards the opponent drew or put on the bottom of their library.
pub fn redact_events(events: &[GameEvent], viewer: PlayerId) -> Vec<GameEvent> {
    events
        .iter()
        .cloned()
        .map(|event| match event {
            GameEvent::CardDrawn { player, card } if player != viewer => GameEvent::CardDrawn {
                player,
                card: face_down(card),
            },
            GameEvent::CardPutOnBottom { player, card } if player != viewer => {
                GameEvent::CardPutOnBottom {
                    player,
                    card: face_down(card),
                }
            }
            event => event,
        })
        .collect()
}

fn face_down(card: CardRef) -> CardRef {
    CardRef {
        object: card.object,
        face: None,
    }
}

/// Whether the player has anything to do other than pass priority. The UI
/// passes automatically when this is false, unless the player is holding
/// priority.
pub fn has_meaningful_action(state: &DuelState, player: PlayerId) -> bool {
    legal_actions(state, player)
        .iter()
        .any(|action| *action != Action::PassPriority)
}
