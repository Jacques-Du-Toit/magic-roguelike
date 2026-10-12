//! What one player is allowed to see: the view of the state the UI renders,
//! and events with hidden information taken out.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::action::{Action, CardFace, CardRef, GameEvent, Target};
use super::error::InternalError;
use super::ids::{CardId, ObjectId, PlayerId};
use super::legal_actions;
use super::mana::ManaPool;
use super::state::{
    AbilityRef, Combat, Decision, DuelState, GameResult, StackItemKind, Step, TriggeredAbility,
    Zone, card_def,
};
use super::turn::scry_card;
use crate::cards::{CardDefId, Keyword, ManaCost};

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
    /// While a triggered ability waits for its targets: the name of the
    /// card or emblem it is on.
    pub aiming: Option<String>,
    /// The card the viewer is looking at while they scry. Only they see it.
    pub looking_at: Option<CardRef>,
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
    /// The player's emblems: relics, boons, passives and the like.
    pub emblems: Vec<EmblemView>,
    /// The player's commander, if they have one. Both players see it.
    pub commander: Option<CommanderView>,
}

/// A player's commander, wherever it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommanderView {
    pub card: CardId,
    pub def: CardDefId,
    /// The card's object while it waits in the command zone, which is what
    /// `Action::CastSpell` names to cast it. `None` while it is elsewhere.
    pub object: Option<ObjectId>,
    /// What it costs to cast from the command zone now, tax included.
    #[ts(type = "string")]
    pub cost: ManaCost,
    /// How many times it has been cast from the command zone.
    pub casts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EmblemView {
    pub id: ObjectId,
    pub name: String,
    pub text: String,
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
    /// The keywords it has now, printed first and then granted.
    pub keywords: Vec<Keyword>,
    /// +1/+1 counters on it.
    pub counters: u32,
    pub damage: i32,
    /// For an Aura, the permanent it enchants.
    pub attached_to: Option<ObjectId>,
    /// It is its owner's commander.
    pub commander: bool,
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
    /// A triggered ability of an emblem.
    Emblem {
        emblem: ObjectId,
        name: String,
    },
    /// The triggered ability a keyword of the card `def` stands for.
    Keyword {
        source: ObjectId,
        def: CardDefId,
        keyword: Keyword,
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
        let commander = match player_state.commander {
            None => None,
            Some(card) => {
                let object = state
                    .objects
                    .values()
                    .find(|object| object.card == card)
                    .ok_or_else(|| InternalError::new(format!("{player:?}'s commander is gone")))?;
                Some(CommanderView {
                    card,
                    def: object.def.clone(),
                    object: (object.zone == Zone::Command).then_some(object.id),
                    cost: state.commander_cost(player, card_def(&object.def)?),
                    casts: player_state.commander_casts,
                })
            }
        };
        Ok(PlayerSummary {
            commander,
            player,
            life: player_state.life,
            hand,
            library_size: u32::try_from(player_state.library.len()).unwrap_or(u32::MAX),
            graveyard,
            mana_pool: player_state.mana_pool.clone(),
            lands_played_this_turn: player_state.lands_played_this_turn,
            mulligans_taken: player_state.mulligans_taken,
            emblems: state
                .emblems
                .iter()
                .filter(|emblem| emblem.controller == player)
                .map(|emblem| EmblemView {
                    id: emblem.id,
                    name: emblem.def.name.clone(),
                    text: emblem.def.text.clone(),
                })
                .collect(),
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
            keywords: state.keywords(id)?,
            counters: permanent.plus_counters,
            damage: permanent.damage,
            attached_to: permanent.attached_to,
            commander: state.is_commander(id),
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
            StackItemKind::Trigger {
                ability: TriggeredAbility::Card { ability },
            } => StackItemViewKind::Ability {
                ability: ability.clone(),
            },
            StackItemKind::Trigger {
                ability: TriggeredAbility::Emblem { emblem, .. },
            } => StackItemViewKind::Emblem {
                emblem: *emblem,
                name: emblem_name(state, *emblem),
            },
            StackItemKind::Trigger {
                ability:
                    TriggeredAbility::Keyword {
                        source,
                        def,
                        keyword,
                    },
            } => StackItemViewKind::Keyword {
                source: *source,
                def: def.clone(),
                keyword: *keyword,
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
        aiming: match (state.decision, state.pending_triggers.first()) {
            (Some(Decision::ChooseTargets { .. }), Some(trigger)) => Some(match &trigger.ability {
                TriggeredAbility::Card { ability } => card_name(&ability.def),
                TriggeredAbility::Keyword { def, .. } => card_name(def),
                TriggeredAbility::Emblem { emblem, .. } => emblem_name(state, *emblem),
            }),
            _ => None,
        },
        looking_at: match state.decision {
            Some(Decision::Scry { player, kept, .. }) if player == viewer => {
                Some(state.card_ref(scry_card(state, player, kept)?)?)
            }
            _ => None,
        },
        result: state.result,
    })
}

fn emblem_name(state: &DuelState, emblem: ObjectId) -> String {
    state
        .emblems
        .iter()
        .find(|candidate| candidate.id == emblem)
        .map(|candidate| candidate.def.name.clone())
        .unwrap_or_default()
}

fn card_name(def: &CardDefId) -> String {
    card_def(def).map_or_else(|_| def.to_string(), |card| card.name.clone())
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

/// Whether the player has anything to do other than pass priority.
pub fn has_meaningful_action(state: &DuelState, player: PlayerId) -> bool {
    legal_actions(state, player)
        .iter()
        .any(|action| *action != Action::PassPriority)
}

/// Whether the UI should pass priority for the player without asking them
/// (Arena-style auto-pass). `hold_priority` is the player's toggle: with it
/// on, they are asked every time they get priority.
///
/// Otherwise the player is asked only when they have something to do other
/// than pass, and then only:
/// - when the opponent's spell or ability is on top of the stack (their own
///   is left to resolve);
/// - with an empty stack, in their own main phases, once attackers or
///   blockers have been declared, and in the opponent's end step.
///
/// Never true for any decision other than priority.
pub fn should_auto_pass(state: &DuelState, player: PlayerId, hold_priority: bool) -> bool {
    if state.decision != Some(Decision::Priority { player }) {
        return false;
    }
    if hold_priority {
        return false;
    }
    if !has_meaningful_action(state, player) {
        return true;
    }
    if let Some(top) = state.stack.last() {
        return top.controller == player;
    }
    let own_turn = state.active_player == player;
    let stop = match state.step {
        Step::PrecombatMain | Step::PostcombatMain => own_turn,
        Step::DeclareAttackers | Step::DeclareBlockers => true,
        Step::End => !own_turn,
        Step::Untap
        | Step::Upkeep
        | Step::Draw
        | Step::BeginCombat
        | Step::FirstStrikeDamage
        | Step::CombatDamage
        | Step::EndCombat
        | Step::Cleanup => false,
    };
    !stop
}
