//! Playing one AI-vs-AI game and recording what the report needs.

use engine::ai_choose_action;
use engine::balance::SimBalance;
use engine::cards::CardDefId;
use engine::data::{self, CardDatabase};
use engine::duel::{
    Action, DuelConfig, DuelState, GameEvent, GameResult, ObjectId, PlayerId, Target, WinReason,
    Zone, apply_action, new_duel,
};

/// How a game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Win {
        seat: PlayerId,
        by_fatigue: bool,
    },
    Draw,
    /// Stopped for running past the turn cap.
    TurnCap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameRecord {
    pub seed: u64,
    pub starting_player: PlayerId,
    /// Turns begun, counting both players'.
    pub turns: u32,
    pub outcome: Outcome,
    /// Indexed like `PlayerId`: P1 first.
    pub players: [PlayerRecord; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerRecord {
    pub mulligans: u8,
    /// Whether the player was stuck on lands, if the game lasted long enough
    /// to tell.
    pub stuck_on_lands: Option<bool>,
    /// Whether the player was flooded, if the game lasted long enough to
    /// tell.
    pub flooded: Option<bool>,
    /// How many of their own turns the player finished.
    pub turns_finished: u32,
    /// Untapped lands at the end of each of those turns, added up.
    pub untapped_lands: u32,
}

/// Plays one game and describes it move by move. Returns the text and
/// whether the game hit an error.
pub fn replay(config: DuelConfig, seed: u64, limits: &SimBalance) -> (String, bool) {
    let mut text = String::new();
    match write_replay(&mut text, config, seed, limits) {
        Ok(()) => (text, false),
        Err(error) => {
            text.push_str(&format!("ERROR: {error}\n"));
            (text, true)
        }
    }
}

fn write_replay(
    text: &mut String,
    config: DuelConfig,
    seed: u64,
    limits: &SimBalance,
) -> Result<(), String> {
    let cards = &data::embedded().map_err(|error| error.to_string())?.cards;
    let name = |def: &CardDefId| cards.get(def).map_or("?", |card| card.name.as_str());
    let named = |state: &DuelState, id: ObjectId| {
        state
            .objects
            .get(&id)
            .map_or("?", |object| name(&object.def))
    };
    let target_name = |state: &DuelState, target: &Target| match target {
        Target::Player { player } => format!("{player:?}"),
        Target::Object { object } => named(state, *object).to_string(),
    };

    let mut state = new_duel(config, seed).map_err(|error| error.to_string())?;
    text.push_str(&format!(
        "Replay, seed {seed}. {:?} goes first.\n",
        state.starting_player
    ));
    while state.result.is_none() {
        if state.turn > limits.turn_cap {
            text.push_str("Stopped at the turn cap.\n");
            return Ok(());
        }
        let player = state
            .waiting_on()
            .ok_or("the game has no result and waits on nobody")?;
        let action = ai_choose_action(&state, player).map_err(|error| format!("AI: {error}"))?;

        // Describe the action while the objects it names still exist.
        let line = match &action {
            Action::Mulligan => Some("mulligans".to_string()),
            Action::Keep => Some(format!("keeps {} cards", state.player(player).hand.len())),
            Action::BottomCard { card } => Some(format!("bottoms {}", named(&state, *card))),
            Action::Discard { card } => Some(format!("discards {}", named(&state, *card))),
            Action::PlayLand { card } => Some(format!("plays {}", named(&state, *card))),
            Action::CastSpell { card, targets } => {
                let targets: Vec<String> = targets
                    .iter()
                    .map(|target| target_name(&state, target))
                    .collect();
                let at = if targets.is_empty() {
                    String::new()
                } else {
                    format!(" at {}", targets.join(", "))
                };
                Some(format!("casts {}{at}", named(&state, *card)))
            }
            Action::ActivateAbility {
                source,
                targets,
                sacrifice,
                ..
            } => {
                let mut line = format!("activates {}", named(&state, *source));
                if let Some(sacrifice) = sacrifice {
                    line.push_str(&format!(", sacrificing {}", named(&state, *sacrifice)));
                }
                if !targets.is_empty() {
                    let targets: Vec<String> = targets
                        .iter()
                        .map(|target| target_name(&state, target))
                        .collect();
                    line.push_str(&format!(", at {}", targets.join(", ")));
                }
                Some(line)
            }
            Action::ChooseTargets { targets } => {
                let targets: Vec<String> = targets
                    .iter()
                    .map(|target| target_name(&state, target))
                    .collect();
                Some(format!("aims an ability at {}", targets.join(", ")))
            }
            Action::ScryKeep => Some("scries: keeps the card on top".to_string()),
            Action::ScryBottom => Some("scries: puts the card on the bottom".to_string()),
            Action::ConfirmAttackers => state
                .combat
                .as_ref()
                .filter(|combat| !combat.attackers.is_empty())
                .map(|combat| {
                    let attackers: Vec<&str> = combat
                        .attackers
                        .iter()
                        .map(|attacker| named(&state, attacker.attacker))
                        .collect();
                    format!("attacks with {}", attackers.join(", "))
                }),
            Action::ConfirmBlockers => state.combat.as_ref().and_then(|combat| {
                let blocks: Vec<String> = combat
                    .attackers
                    .iter()
                    .flat_map(|attacker| {
                        attacker.blockers.iter().map(|blocker| {
                            format!(
                                "{} with {}",
                                named(&state, attacker.attacker),
                                named(&state, *blocker)
                            )
                        })
                    })
                    .collect();
                (!blocks.is_empty()).then(|| format!("blocks {}", blocks.join(", ")))
            }),
            Action::PassPriority
            | Action::Attack { .. }
            | Action::Block { .. }
            | Action::RemoveAttacker { .. }
            | Action::RemoveBlocker { .. }
            | Action::ResetDeclaration
            | Action::AssignDamage { .. }
            | Action::UnassignDamage { .. }
            | Action::ConfirmDamage => None,
        };
        if let Some(line) = line {
            text.push_str(&format!("  {player:?} {line}\n"));
        }

        let events = apply_action(&mut state, player, action.clone())
            .map_err(|error| format!("{action:?}: {error}"))?;
        for event in &events {
            match event {
                GameEvent::TurnBegan { turn, player } => {
                    let [p1, p2] = &state.players;
                    text.push_str(&format!(
                        "Turn {turn} ({player:?}). Life: P1 {}, P2 {}. Hands: P1 {}, P2 {}.\n",
                        p1.life,
                        p2.life,
                        p1.hand.len(),
                        p2.hand.len()
                    ));
                }
                GameEvent::CreatureDied { player, card, .. } => {
                    let died = card.face.as_ref().map_or("?", |face| name(&face.def));
                    text.push_str(&format!("    {player:?}'s {died} dies\n"));
                }
                GameEvent::SpellCountered { card, .. } | GameEvent::SpellFizzled { card, .. } => {
                    let spell = card.face.as_ref().map_or("?", |face| name(&face.def));
                    text.push_str(&format!("    {spell} is countered or fizzles\n"));
                }
                GameEvent::LifeChanged {
                    player,
                    change,
                    life,
                } => text.push_str(&format!("    {player:?} life {life} ({change:+})\n")),
                GameEvent::GameOver { result } => match result {
                    GameResult::Win { player, reason } => {
                        text.push_str(&format!("{player:?} wins ({reason:?}).\n"));
                    }
                    GameResult::Draw => text.push_str("The game is a draw.\n"),
                },
                _ => {}
            }
        }
    }
    Ok(())
}

/// A player's lands at one moment.
struct Lands {
    on_battlefield: u32,
    untapped: u32,
    /// Lands among the cards the player has drawn (everything they own
    /// outside their library).
    drawn: u32,
    cards_drawn: u32,
}

fn count_lands(state: &DuelState, cards: &CardDatabase, player: PlayerId) -> Lands {
    let mut lands = Lands {
        on_battlefield: 0,
        untapped: 0,
        drawn: 0,
        cards_drawn: 0,
    };
    for object in state.objects.values() {
        if object.owner != player || object.zone == Zone::Library {
            continue;
        }
        lands.cards_drawn += 1;
        if !cards.get(&object.def).is_some_and(|def| def.is_land()) {
            continue;
        }
        lands.drawn += 1;
        if object.zone == Zone::Battlefield {
            lands.on_battlefield += 1;
            if !object.tapped {
                lands.untapped += 1;
            }
        }
    }
    lands
}

/// Plays one game with the AI on both sides. Any engine or AI error ends
/// the game and is returned as text.
pub fn play_game(config: DuelConfig, seed: u64, limits: &SimBalance) -> Result<GameRecord, String> {
    let cards = &data::embedded().map_err(|error| error.to_string())?.cards;
    let mut state = new_duel(config, seed).map_err(|error| error.to_string())?;
    let mut players = [PlayerRecord::default(), PlayerRecord::default()];

    let outcome = loop {
        if let Some(result) = state.result {
            break match result {
                GameResult::Win { player, reason } => Outcome::Win {
                    seat: player,
                    by_fatigue: reason == WinReason::Fatigue,
                },
                GameResult::Draw => Outcome::Draw,
            };
        }
        if state.turn > limits.turn_cap {
            break Outcome::TurnCap;
        }
        let player = state.waiting_on().ok_or_else(|| {
            format!(
                "turn {}: the game has no result and waits on nobody",
                state.turn
            )
        })?;
        let action = ai_choose_action(&state, player)
            .map_err(|error| format!("turn {}: AI: {error}", state.turn))?;
        let events = apply_action(&mut state, player, action.clone())
            .map_err(|error| format!("turn {}: {action:?}: {error}", state.turn))?;

        for event in &events {
            // A turn beginning means the other player just finished theirs.
            // Only the new active player's permanents have changed since.
            let GameEvent::TurnBegan { turn, player } = *event else {
                continue;
            };
            if turn < 2 {
                continue;
            }
            let finished = player.opponent();
            let own_turn = turn / 2;
            let lands = count_lands(&state, cards, finished);
            let record = &mut players[finished.index()];
            record.turns_finished += 1;
            record.untapped_lands += lands.untapped;
            if own_turn == limits.land_screw.turn {
                record.stuck_on_lands = Some(lands.on_battlefield <= limits.land_screw.max_lands);
            }
            if own_turn == limits.land_flood.turn {
                record.flooded = Some(
                    lands.drawn * 100 >= limits.land_flood.min_land_percent * lands.cards_drawn,
                );
            }
        }
    };

    for player in PlayerId::ALL {
        players[player.index()].mulligans = state.player(player).mulligans_taken;
    }
    Ok(GameRecord {
        seed,
        starting_player: state.starting_player,
        // The cap is noticed once the turn after it has begun.
        turns: state.turn.min(limits.turn_cap),
        outcome,
        players,
    })
}
