//! Feats: what the player sets out to do in a duel beyond winning it. Two
//! are picked before a duel, each scores 0 to 3 when the duel is won, and
//! the total sets the reward (`DECISIONS.md` #68). The definitions are data
//! (`data/run/feats.json`); this is their format, what a duel is measured
//! by, and the scoring.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cards::CardType;
use crate::duel::{DuelState, GameEvent, InternalError, PlayerId};

use super::defs::{FeatId, PlaneswalkerId};

/// Something to do in a duel, in up to three tiers of the same measure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct FeatDef {
    pub id: FeatId,
    pub name: String,
    /// What to do, in words, with `{n}` where a tier's number goes.
    pub text: String,
    pub measure: Measure,
    /// The number each tier asks for, easiest first: a duel scores a point
    /// for each one met.
    pub tiers: Vec<i32>,
    /// "By your turn N": only what happens before the player's turn after
    /// that one begins counts. For the measures that are a high point.
    #[serde(default)]
    pub by_turn: Option<u32>,
    /// The characters it is offered to; empty for all of them.
    #[serde(default)]
    pub planeswalkers: Vec<PlaneswalkerId>,
}

/// What a feat counts. Each is a number about the player's duel so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    /// The lowest the player's life total has been.
    LowestLife,
    /// How many turns the player has taken. Fewer is better.
    TurnsTaken,
    /// How many of the player's creatures have died. Fewer is better.
    CreaturesLost,
    /// The greatest power a creature of the player's has had.
    GreatestPower,
    /// The most by which the total power of the player's creatures has
    /// been above that of the enemy's.
    PowerLead,
    /// The most creatures the player has controlled at once.
    MostCreatures,
    /// The most +1/+1 counters the player's creatures have had at once.
    MostCounters,
    /// The most enchantments the player has controlled at once.
    MostEnchantments,
}

impl Measure {
    /// Whether a tier asks for at most its number, not at least.
    pub fn fewer_is_better(self) -> bool {
        matches!(self, Measure::TurnsTaken | Measure::CreaturesLost)
    }

    /// Whether it is a high point reached at some moment, which a feat can
    /// put a deadline on.
    pub fn is_high_point(self) -> bool {
        !matches!(
            self,
            Measure::LowestLife | Measure::TurnsTaken | Measure::CreaturesLost
        )
    }
}

/// The high points of one stretch of a duel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HighPoints {
    pub greatest_power: i32,
    pub power_lead: i32,
    pub creatures: i32,
    pub counters: i32,
    pub enchantments: i32,
}

/// What a duel has been like for one player so far: everything a feat can
/// be scored from, whichever feats were picked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuelStats {
    pub player: PlayerId,
    /// Turns of the player's own begun so far.
    pub turns: u32,
    pub lowest_life: i32,
    pub creatures_lost: u32,
    /// `high_points[n]` is from the player's turn `n` beginning until their
    /// next one does; `high_points[0]` is before their first turn.
    pub high_points: Vec<HighPoints>,
}

fn count(items: usize) -> i32 {
    i32::try_from(items).unwrap_or(i32::MAX)
}

impl DuelStats {
    /// The stats of a duel that is about to be played.
    pub fn new(duel: &DuelState, player: PlayerId) -> Result<DuelStats, InternalError> {
        let mut stats = DuelStats {
            player,
            turns: 0,
            lowest_life: duel.player(player).life,
            creatures_lost: 0,
            high_points: Vec::new(),
        };
        stats.observe(duel, &[])?;
        Ok(stats)
    }

    /// Takes in what just happened: `events` are those of the action just
    /// applied, and `duel` is the duel after it. The position is looked at
    /// once per action, so a high point that came and went inside one
    /// action is not seen.
    pub fn observe(&mut self, duel: &DuelState, events: &[GameEvent]) -> Result<(), InternalError> {
        for event in events {
            match event {
                GameEvent::TurnBegan { player, .. } if *player == self.player => self.turns += 1,
                GameEvent::LifeChanged { player, life, .. } if *player == self.player => {
                    self.lowest_life = self.lowest_life.min(*life);
                }
                GameEvent::CreatureDied { player, .. } if *player == self.player => {
                    self.creatures_lost += 1;
                }
                _ => {}
            }
        }

        let mut now = HighPoints::default();
        let mut power = 0i32;
        for creature in duel.creatures_of(self.player)? {
            let creature_power = duel.power(creature)?;
            now.greatest_power = now.greatest_power.max(creature_power);
            power = power.saturating_add(creature_power);
            now.creatures += 1;
            now.counters = now
                .counters
                .saturating_add(count(duel.object(creature)?.plus_counters as usize));
        }
        let mut enemy_power = 0i32;
        for creature in duel.creatures_of(self.player.opponent())? {
            enemy_power = enemy_power.saturating_add(duel.power(creature)?);
        }
        now.power_lead = power.saturating_sub(enemy_power);
        for permanent in duel.permanents_of(self.player) {
            if duel.def(permanent.id)?.has_type(CardType::Enchantment) {
                now.enchantments += 1;
            }
        }

        let turn = self.turns as usize;
        if self.high_points.len() <= turn {
            self.high_points.resize(turn + 1, HighPoints::default());
        }
        let stretch = &mut self.high_points[turn];
        stretch.greatest_power = stretch.greatest_power.max(now.greatest_power);
        stretch.power_lead = stretch.power_lead.max(now.power_lead);
        stretch.creatures = stretch.creatures.max(now.creatures);
        stretch.counters = stretch.counters.max(now.counters);
        stretch.enchantments = stretch.enchantments.max(now.enchantments);
        Ok(())
    }

    /// The number a feat's tiers are compared with.
    pub fn value(&self, measure: Measure, by_turn: Option<u32>) -> i32 {
        let high_point = |of: fn(&HighPoints) -> i32| {
            let stretches = match by_turn {
                Some(turn) => (turn as usize).saturating_add(1),
                None => usize::MAX,
            };
            self.high_points
                .iter()
                .take(stretches)
                .map(of)
                .max()
                .unwrap_or(0)
        };
        match measure {
            Measure::LowestLife => self.lowest_life,
            Measure::TurnsTaken => i32::try_from(self.turns).unwrap_or(i32::MAX),
            Measure::CreaturesLost => i32::try_from(self.creatures_lost).unwrap_or(i32::MAX),
            Measure::GreatestPower => high_point(|stretch| stretch.greatest_power),
            Measure::PowerLead => high_point(|stretch| stretch.power_lead),
            Measure::MostCreatures => high_point(|stretch| stretch.creatures),
            Measure::MostCounters => high_point(|stretch| stretch.counters),
            Measure::MostEnchantments => high_point(|stretch| stretch.enchantments),
        }
    }
}

impl FeatDef {
    /// Whether the feat is offered to this character.
    pub fn is_for(&self, planeswalker: &PlaneswalkerId) -> bool {
        self.planeswalkers.is_empty() || self.planeswalkers.contains(planeswalker)
    }

    /// The number its tiers are compared with, for a duel so far.
    pub fn value(&self, stats: &DuelStats) -> i32 {
        stats.value(self.measure, self.by_turn)
    }

    /// How many of its tiers a duel with these stats meets, were it won
    /// now: its score.
    pub fn score(&self, stats: &DuelStats) -> u8 {
        let value = self.value(stats);
        let met = self
            .tiers
            .iter()
            .filter(|&&tier| {
                if self.measure.fewer_is_better() {
                    value <= tier
                } else {
                    value >= tier
                }
            })
            .count();
        u8::try_from(met).unwrap_or(u8::MAX)
    }
}

/// How a feat picked for a duel stands, or stood when the duel ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FeatScore {
    pub feat: FeatId,
    /// The tiers met: 0 to 3.
    pub score: u8,
    /// The number its tiers are compared with.
    pub value: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;
    use crate::duel::apply_action;
    use crate::duel::test_support::{
        cast, cast_at, empty_board, pass_both, pass_until, put_in_hand, put_lands,
        put_on_battlefield,
    };
    use crate::duel::{Step, Target};

    fn feat(id: &str) -> &'static FeatDef {
        data::embedded()
            .unwrap()
            .run
            .feat(&FeatId(id.to_string()))
            .unwrap_or_else(|| panic!("no feat `{id}`"))
    }

    /// Stats that have seen nothing but `change`.
    fn stats(change: impl FnOnce(&mut DuelStats)) -> DuelStats {
        let mut stats = DuelStats {
            player: PlayerId::P1,
            turns: 5,
            lowest_life: 20,
            creatures_lost: 0,
            high_points: vec![HighPoints::default(); 6],
        };
        change(&mut stats);
        stats
    }

    #[test]
    fn a_feat_scores_a_point_for_each_tier_met() {
        // Unbroken: never below 12 / 15 / 18.
        let unbroken = feat("unbroken");
        for (life, score) in [
            (20, 3),
            (18, 3),
            (17, 2),
            (15, 2),
            (14, 1),
            (12, 1),
            (11, 0),
        ] {
            assert_eq!(
                unbroken.score(&stats(|s| s.lowest_life = life)),
                score,
                "{life}"
            );
        }
        // Swift: by turn 9 / 7 / 6. Fewer is better.
        let swift = feat("swift");
        for (turns, score) in [(4, 3), (6, 3), (7, 2), (8, 1), (9, 1), (10, 0)] {
            assert_eq!(swift.score(&stats(|s| s.turns = turns)), score, "{turns}");
        }
        // Unbowed: at most 3 / 1 / 0 creatures lost.
        let unbowed = feat("unbowed");
        for (lost, score) in [(0, 3), (1, 2), (2, 1), (3, 1), (4, 0)] {
            assert_eq!(
                unbowed.score(&stats(|s| s.creatures_lost = lost)),
                score,
                "{lost}"
            );
        }
        // Legion: 4 / 6 / 8 creatures at once, whenever that was.
        let legion = feat("legion");
        for (creatures, score) in [(3, 0), (4, 1), (6, 2), (9, 3)] {
            assert_eq!(
                legion.score(&stats(|s| s.high_points[2].creatures = creatures)),
                score,
                "{creatures}"
            );
        }
    }

    #[test]
    fn a_deadline_counts_only_what_came_before_it() {
        // Towering: a creature with power 5 / 7 / 10 by turn 7.
        let towering = feat("towering");
        assert_eq!(towering.by_turn, Some(7));
        let at = |turn: usize, power: i32| {
            stats(|s| {
                s.turns = 9;
                s.high_points = vec![HighPoints::default(); 10];
                s.high_points[turn].greatest_power = power;
            })
        };
        assert_eq!(towering.score(&at(3, 7)), 2);
        // Through the enemy's turn after the player's seventh.
        assert_eq!(towering.score(&at(7, 10)), 3);
        assert_eq!(towering.score(&at(8, 10)), 0);
    }

    /// Casts a spell at one target, lets it resolve, and has `stats` take
    /// in what happened.
    fn cast_and_observe(
        duel: &mut DuelState,
        stats: &mut DuelStats,
        player: PlayerId,
        card: &str,
        target: Target,
    ) {
        let card = put_in_hand(duel, player, card);
        let mut events = apply_action(duel, player, cast_at(card, target)).unwrap();
        events.extend(pass_both(duel));
        stats.observe(duel, &events).unwrap();
    }

    #[test]
    fn a_duel_is_measured_as_it_is_played() {
        // My first turn has begun; the stats start counting from here.
        let (mut duel, me, opponent) = empty_board(1);
        let courser = put_on_battlefield(&mut duel, me, "centaur_courser");
        let bears = put_on_battlefield(&mut duel, me, "grizzly_bears");
        duel.object_mut(bears).unwrap().plus_counters = 2;
        put_on_battlefield(&mut duel, me, "glorious_anthem");
        put_on_battlefield(&mut duel, opponent, "raging_goblin");
        put_lands(&mut duel, me, "forest", 1);
        put_lands(&mut duel, opponent, "swamp", 4);
        let mut stats = DuelStats::new(&duel, me).unwrap();
        assert_eq!((stats.turns, stats.lowest_life), (0, 20));
        assert_eq!(duel.power(courser).unwrap(), 4);

        // A pump spell is a high point that wears off.
        let on_bears = Target::Object { object: bears };
        cast_and_observe(&mut duel, &mut stats, me, "giant_growth", on_bears);
        // Bears: 2/2, two counters, the Anthem's +1/+1, Giant Growth's +3/+3.
        assert_eq!(stats.value(Measure::GreatestPower, None), 8);
        assert_eq!(stats.value(Measure::MostCreatures, None), 2);
        assert_eq!(stats.value(Measure::MostCounters, None), 2);
        assert_eq!(stats.value(Measure::MostEnchantments, None), 1);
        // 8 and the Courser's 4 against the Goblin's 1.
        assert_eq!(stats.value(Measure::PowerLead, None), 11);

        // In the enemy's turn: a creature lost.
        let events = pass_until(&mut duel, 2, Step::PrecombatMain);
        stats.observe(&duel, &events).unwrap();
        cast_and_observe(&mut duel, &mut stats, opponent, "murder", on_bears);
        assert_eq!(stats.creatures_lost, 1);
        assert_eq!(stats.turns, 0);

        // A turn of my own begins.
        let events = pass_until(&mut duel, 3, Step::Upkeep);
        stats.observe(&duel, &events).unwrap();
        assert_eq!(stats.turns, 1);
        assert_eq!(stats.lowest_life, 20);
        // The high points stand though the Bears are gone, and belong to
        // the stretch they were reached in.
        assert_eq!(stats.value(Measure::GreatestPower, None), 8);
        assert_eq!(stats.value(Measure::GreatestPower, Some(0)), 8);
        assert_eq!(stats.high_points[1].greatest_power, 4);
    }

    #[test]
    fn life_lost_is_remembered_after_it_is_gained_back() {
        let (mut duel, me, opponent) = empty_board(1);
        put_lands(&mut duel, me, "plains", 2);
        put_lands(&mut duel, me, "mountain", 1);
        let mut stats = DuelStats::new(&duel, me).unwrap();
        let my_face = Target::Player { player: me };
        cast_and_observe(&mut duel, &mut stats, me, "shock", my_face);
        assert_eq!(stats.lowest_life, 18);
        let revitalize = put_in_hand(&mut duel, me, "revitalize");
        let mut events = apply_action(&mut duel, me, cast(revitalize)).unwrap();
        events.extend(pass_both(&mut duel));
        stats.observe(&duel, &events).unwrap();
        assert_eq!(duel.player(me).life, 21);
        assert_eq!(stats.lowest_life, 18);
        assert_eq!(duel.player(opponent).life, 20);
    }
}
