//! Turning what each side brings to a fight into a `DuelConfig`. This is the
//! one place where relics, boons, passives and boss emblems reach the duel.

use crate::balance::Balance;
use crate::cards::{CardDefId, Color};
use crate::data::CardDatabase;
use crate::decks::DeckEntry;
use crate::duel::{CardInstance, DuelConfig, InternalError, PlayerConfig, RulesConfig};

use super::defs::{EnemyDef, ModifierDef, ModifierEffect, PlaneswalkerDef, Who};
use super::lands::{basic_land, basic_lands, main_color};

/// One side of a duel: its library and the modifiers it owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loadout<'a> {
    /// Lands and spells together.
    pub library: Vec<CardInstance>,
    pub modifiers: Vec<&'a ModifierDef>,
    /// The basic land a "start with a land" modifier gives this side.
    pub main_land: Option<CardDefId>,
}

fn instances(entries: &[DeckEntry]) -> Vec<CardInstance> {
    entries
        .iter()
        .flat_map(|entry| (0..entry.count).map(|_| CardInstance::new(entry.card.clone())))
        .collect()
}

/// A planeswalker's starting deck, without basic lands.
pub fn starting_deck(planeswalker: &PlaneswalkerDef) -> Vec<CardInstance> {
    instances(&planeswalker.spells)
}

/// The full library for a player's deck of spells and non-basic lands: the
/// deck plus its automatic basic lands.
pub fn library_with_lands(
    deck: &[CardInstance],
    colors: &[Color],
    cards: &CardDatabase,
    balance: &Balance,
) -> Result<Vec<CardInstance>, InternalError> {
    let mut library = deck.to_vec();
    for (land, count) in basic_lands(deck, colors, cards, &balance.run.lands)? {
        library.extend((0..count).map(|_| CardInstance::new(land.clone())));
    }
    Ok(library)
}

impl<'a> Loadout<'a> {
    /// The player's side: their deck with its basic lands, and their
    /// modifiers (passive, relics, boons).
    pub fn player(
        deck: &[CardInstance],
        colors: &[Color],
        modifiers: Vec<&'a ModifierDef>,
        cards: &CardDatabase,
        balance: &Balance,
    ) -> Result<Loadout<'a>, InternalError> {
        let main_land = match main_color(deck, colors, cards)? {
            Some(color) => Some(basic_land(color, cards)?),
            None => None,
        };
        Ok(Loadout {
            library: library_with_lands(deck, colors, cards, balance)?,
            modifiers,
            main_land,
        })
    }

    /// An enemy's side: its hand-built deck and its emblems.
    pub fn enemy(enemy: &'a EnemyDef, cards: &CardDatabase) -> Result<Loadout<'a>, InternalError> {
        let library = instances(&enemy.cards);
        let main_land = match main_color(&library, &[], cards)? {
            Some(color) => Some(basic_land(color, cards)?),
            None => None,
        };
        Ok(Loadout {
            library,
            modifiers: enemy.modifiers.iter().collect(),
            main_land,
        })
    }
}

/// The duel between two loadouts. `p1` sits in seat P1.
pub fn duel_config(p1: Loadout<'_>, p2: Loadout<'_>, balance: &Balance) -> DuelConfig {
    let mut players = [&p1, &p2].map(|loadout| PlayerConfig {
        library: loadout.library.clone(),
        starting_life: balance.duel.starting_life,
        opening_hand_size: balance.duel.opening_hand_size,
        starting_permanents: Vec::new(),
        emblems: Vec::new(),
    });
    for (owner, loadout) in [&p1, &p2].into_iter().enumerate() {
        let opponent = 1 - owner;
        for modifier in &loadout.modifiers {
            for effect in &modifier.effects {
                match effect {
                    ModifierEffect::StartingLife { who, change } => {
                        let seat = match who {
                            Who::Owner => owner,
                            Who::Opponent => opponent,
                        };
                        let life = &mut players[seat].starting_life;
                        // Nobody starts a duel already dead.
                        *life = life.saturating_add(*change).max(1);
                    }
                    ModifierEffect::OpeningHand { change } => {
                        let hand = &mut players[owner].opening_hand_size;
                        *hand = hand.saturating_add_signed(*change).max(1);
                    }
                    ModifierEffect::StartingBasicLand {} => {
                        if let Some(land) = &loadout.main_land {
                            players[owner]
                                .starting_permanents
                                .push(CardInstance::new(land.clone()));
                        }
                    }
                    // Collected into one emblem per modifier below.
                    ModifierEffect::Abilities { .. } => {}
                    // Acts on the run, not the duel.
                    ModifierEffect::GoldPerWin { .. } => {}
                }
            }
            if let Some(emblem) = modifier.emblem() {
                players[owner].emblems.push(emblem);
            }
        }
    }
    DuelConfig {
        players,
        rules: RulesConfig::from_balance(&balance.duel),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;
    use crate::duel::{PlayerId, new_duel};
    use crate::run::defs::{EnemyId, PlaneswalkerId};

    fn planeswalker(id: &str) -> &'static PlaneswalkerDef {
        data::embedded()
            .unwrap()
            .run
            .planeswalker(&PlaneswalkerId(id.to_string()))
            .unwrap()
    }

    fn enemy(id: &str) -> &'static EnemyDef {
        data::embedded()
            .unwrap()
            .run
            .enemy(&EnemyId(id.to_string()))
            .unwrap()
    }

    fn player_loadout<'a>(
        planeswalker: &'a PlaneswalkerDef,
        extra: &[&'a ModifierDef],
    ) -> Loadout<'a> {
        let data = data::embedded().unwrap();
        let mut modifiers = vec![&planeswalker.passive];
        modifiers.extend(extra);
        Loadout::player(
            &starting_deck(planeswalker),
            &planeswalker.colors,
            modifiers,
            &data.cards,
            &data.balance,
        )
        .unwrap()
    }

    #[test]
    fn a_starting_deck_gets_its_lands_and_its_passive() {
        let data = data::embedded().unwrap();
        let warden = planeswalker("warden");
        let config = duel_config(
            player_loadout(warden, &[]),
            Loadout::enemy(enemy("goblin_raid"), &data.cards).unwrap(),
            &data.balance,
        );
        let [player, opponent] = &config.players;
        assert_eq!(player.library.len(), 25);
        let count = |card: &str| {
            player
                .library
                .iter()
                .filter(|instance| instance.card_id.as_str() == card)
                .count()
        };
        // 6 white symbols and 10 green.
        assert_eq!((count("plains"), count("forest")), (4, 6));
        assert_eq!(player.starting_life, 20);
        assert_eq!(player.emblems.len(), 1);
        assert_eq!(player.emblems[0].name, "Warden's Welcome");
        assert_eq!(opponent.library.len(), 25);
        assert!(opponent.emblems.is_empty());
        assert!(new_duel(config, 1).is_ok());
    }

    #[test]
    fn the_boss_brings_its_emblems() {
        let data = data::embedded().unwrap();
        let config = duel_config(
            player_loadout(planeswalker("firebrand"), &[]),
            Loadout::enemy(enemy("pyre_tyrant"), &data.cards).unwrap(),
            &data.balance,
        );
        let [player, boss] = &config.players;
        assert_eq!(boss.starting_life, 25);
        assert_eq!(boss.emblems.len(), 1);
        assert_eq!(boss.emblems[0].name, "The Pyre");
        assert_eq!(player.starting_life, 20);
        let state = new_duel(config, 3).unwrap();
        assert_eq!(state.player(PlayerId::P2).life, 25);
        assert_eq!(state.emblems.len(), 2);
    }

    #[test]
    fn boons_and_relics_change_the_setup() {
        let data = data::embedded().unwrap();
        let firebrand = planeswalker("firebrand");
        let mut extra: Vec<&ModifierDef> = data.balance.run.boons.iter().collect();
        extra.extend(&data.run.relics);
        let config = duel_config(
            player_loadout(firebrand, &extra),
            Loadout::enemy(enemy("pyre_tyrant"), &data.cards).unwrap(),
            &data.balance,
        );
        let [player, boss] = &config.players;
        // +5 from the boon and +3 from the relic.
        assert_eq!(player.starting_life, 28);
        assert_eq!(player.opening_hand_size, 8);
        // The boss's own +5 and the boon's -5.
        assert_eq!(boss.starting_life, 20);
        // The Firebrand's spells have more red symbols than black.
        assert_eq!(player.starting_permanents.len(), 1);
        assert_eq!(player.starting_permanents[0].card_id.as_str(), "mountain");
        // The passive and the three relics with abilities.
        let names: Vec<&str> = player.emblems.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Firebrand's Fury",
                "War Banner",
                "Sentinel's Horn",
                "Soothing Spring"
            ]
        );
        let state = new_duel(config, 3).unwrap();
        assert_eq!(state.player(PlayerId::P1).hand.len(), 8);
        assert_eq!(state.permanents_of(PlayerId::P1).count(), 1);
    }

    #[test]
    fn modifiers_cannot_start_a_player_dead() {
        let data = data::embedded().unwrap();
        let mut wound = data.balance.run.boons[2].clone();
        wound.effects = vec![ModifierEffect::StartingLife {
            who: Who::Opponent,
            change: -100,
        }];
        let config = duel_config(
            player_loadout(planeswalker("warden"), &[&wound]),
            Loadout::enemy(enemy("forest_pack"), &data.cards).unwrap(),
            &data.balance,
        );
        assert_eq!(config.players[1].starting_life, 1);
    }
}
