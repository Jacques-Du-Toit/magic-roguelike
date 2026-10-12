//! The full state of a duel.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::action::{CardFace, CardRef, Target};
use super::config::RulesConfig;
use super::error::InternalError;
use super::ids::{CardId, ObjectId, PlayerId};
use super::mana::ManaPool;
use super::rng::DuelRng;
use crate::cards::{
    AbilityDef, Affects, CardDef, CardDefId, Effect, EmblemDef, Keyword, Restricted, StaticAbility,
};
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
    /// The permanents with an anthem or a keyword grant, in the order they
    /// entered. Kept beside `battlefield` so that working out a creature's
    /// power doesn't mean reading every permanent's card.
    pub static_sources: Vec<ObjectId>,
    /// The command zone: emblems last the whole duel.
    pub emblems: Vec<Emblem>,
    /// The top of the stack is the end of the list.
    pub stack: Vec<StackItem>,
    pub pending_triggers: Vec<PendingTrigger>,
    /// Tokens that have left the battlefield. They cease to exist the next
    /// time state-based actions are checked.
    pub stray_tokens: Vec<ObjectId>,
    /// The "once each turn" abilities that have triggered this turn.
    pub triggered_this_turn: Vec<TriggeredAbility>,
    /// Who gets priority once the waiting triggered abilities are on the
    /// stack: the active player, or a player who has just cast a spell.
    pub priority_to: PlayerId,
    /// The spell or ability that is part-way through resolving, while one
    /// of its effects waits for a player's choice.
    pub resolving: Option<Resolving>,
    pub combat: Option<Combat>,
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
    pub mana_pool: ManaPool,
    pub mulligans_taken: u8,
    pub kept_hand: bool,
    pub lands_played_this_turn: u8,
    /// How many times this player has drawn from an empty library.
    pub fatigue_draws: u32,
    /// Whether the life loss that took this player to 0 or less was fatigue.
    pub lost_to_fatigue: bool,
    /// The player's cards in the command zone: their commander, while it is
    /// not somewhere else.
    #[serde(default)]
    pub command: Vec<ObjectId>,
    /// The physical card that is this player's commander, if they have one.
    #[serde(default)]
    pub commander: Option<CardId>,
    /// How many times the commander has been cast from the command zone.
    #[serde(default)]
    pub commander_casts: u32,
}

impl PlayerState {
    pub(crate) fn new(life: i32, opening_hand_size: u8) -> PlayerState {
        PlayerState {
            life,
            opening_hand_size,
            library: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            mana_pool: ManaPool::default(),
            mulligans_taken: 0,
            kept_hand: false,
            lands_played_this_turn: 0,
            fatigue_draws: 0,
            lost_to_fatigue: false,
            command: Vec::new(),
            commander: None,
            commander_casts: 0,
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
    /// On the battlefield: its controller hasn't had it since the start of
    /// their most recent turn.
    pub summoning_sick: bool,
    /// Damage marked on a creature this turn.
    pub damage: i32,
    /// A source with deathtouch has dealt damage to this creature since
    /// state-based actions were last checked.
    pub deathtouch_damage: bool,
    /// Power and toughness changes that last until end of turn.
    pub power_bonus: i32,
    pub toughness_bonus: i32,
    /// For an Aura on the battlefield, the permanent it enchants.
    pub attached_to: Option<ObjectId>,
    /// Keywords it has until end of turn.
    pub granted_keywords: Vec<Keyword>,
    /// +1/+1 counters on it.
    pub plus_counters: u32,
    /// It doesn't untap during its controller's next untap step.
    pub skip_untap: bool,
    /// The definition `def` names, remembered so that it isn't looked up by
    /// name every time a rule needs it.
    #[serde(skip)]
    found_def: FoundDef,
}

/// A card definition found once and kept beside the id that names it. It
/// is no part of the state: it is left out of saves and of comparisons, and
/// an object without one (or with a stale one) is looked up by id.
#[derive(Debug, Clone, Copy, Default)]
struct FoundDef(Option<&'static CardDef>);

impl PartialEq for FoundDef {
    fn eq(&self, _: &FoundDef) -> bool {
        true
    }
}

impl Eq for FoundDef {}

/// A spell or ability in the middle of resolving. Its targets were checked
/// when it began to resolve; `steps` are the effects still to carry out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolving {
    pub item: StackItem,
    /// The next effect is the first of the list.
    pub steps: Vec<EffectStep>,
    /// All its targets had become illegal, so it does nothing.
    pub fizzled: bool,
}

/// One effect with the targets it will be carried out on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectStep {
    pub effect: Effect,
    pub targets: Vec<Target>,
}

/// A triggered ability that has triggered and is waiting to be put on the
/// stack, which happens the next time a player would get priority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTrigger {
    pub ability: TriggeredAbility,
    pub controller: PlayerId,
    /// What set it off, when that is something other than the permanent
    /// the ability is on: the creature that entered or died.
    pub subject: Option<ObjectId>,
}

/// An emblem in the command zone. It has an object id so that its abilities
/// have a source, but it is not a card and no `GameObject` has its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emblem {
    pub id: ObjectId,
    pub controller: PlayerId,
    pub def: EmblemDef,
}

/// A triggered ability of a card or of an emblem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggeredAbility {
    Card {
        ability: AbilityRef,
    },
    /// `abilities[index]` of the emblem with this id.
    Emblem {
        emblem: ObjectId,
        index: usize,
    },
    /// The triggered ability a keyword stands for (prowess), on the
    /// permanent `source`, a card `def`.
    Keyword {
        source: ObjectId,
        def: CardDefId,
        keyword: Keyword,
    },
}

impl TriggeredAbility {
    /// The object or emblem the ability comes from.
    pub fn source(&self) -> ObjectId {
        match self {
            TriggeredAbility::Card { ability } => ability.source,
            TriggeredAbility::Emblem { emblem, .. } => *emblem,
            TriggeredAbility::Keyword { source, .. } => *source,
        }
    }
}

/// Points at one ability of a card: `abilities[index]` of the definition.
/// `source` is the object the ability came from, which may be gone by now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AbilityRef {
    pub source: ObjectId,
    pub def: CardDefId,
    pub index: usize,
}

/// The combat of the current turn, from the beginning of combat to its end.
/// During the declaration decisions it holds the choices made so far.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct Combat {
    /// In the order they were declared.
    pub attackers: Vec<Attacker>,
    /// Creatures that dealt damage in the first-strike step, and so don't
    /// deal it again in the regular one.
    pub dealt_first_strike_damage: Vec<ObjectId>,
    /// How the attackers with several blockers divide their damage in the
    /// damage step under way. Empty outside one.
    #[serde(default)]
    pub assignments: Vec<DamageAssignment>,
}

/// How one attacker's combat damage is divided among its blockers (and the
/// defending player, for a trampler). It starts as the automatic division,
/// and the attacking player may change it before confirming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DamageAssignment {
    pub attacker: ObjectId,
    /// The damage to divide.
    pub power: i32,
    /// The blockers in the order they were declared, then the player if
    /// the attacker has trample.
    pub shares: Vec<DamageShare>,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DamageShare {
    pub target: Target,
    pub amount: i32,
    /// What it takes to destroy a blocker; 0 for the player.
    pub lethal: i32,
}

impl DamageAssignment {
    /// Damage not yet given to anyone.
    pub fn unassigned(&self) -> i32 {
        self.power - self.shares.iter().map(|share| share.amount).sum::<i32>()
    }

    /// Whether every blocker has been assigned lethal damage.
    pub fn blockers_have_lethal(&self) -> bool {
        self.shares
            .iter()
            .filter(|share| matches!(share.target, Target::Object { .. }))
            .all(|share| share.amount >= share.lethal)
    }

    /// Damage assigned to the defending player.
    pub fn to_player(&self) -> i32 {
        self.shares
            .iter()
            .filter(|share| matches!(share.target, Target::Player { .. }))
            .map(|share| share.amount)
            .sum()
    }

    /// Whether the division obeys the rules: all of it assigned, and none
    /// to the player while a blocker has less than lethal.
    pub fn is_legal(&self) -> bool {
        self.unassigned() == 0 && (self.to_player() == 0 || self.blockers_have_lethal())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Attacker {
    pub attacker: ObjectId,
    /// In the order they were declared.
    pub blockers: Vec<ObjectId>,
    /// Stays true if every blocker later leaves combat.
    pub blocked: bool,
}

impl Combat {
    pub fn is_attacking(&self, creature: ObjectId) -> bool {
        self.attackers.iter().any(|a| a.attacker == creature)
    }

    pub fn is_blocking(&self, creature: ObjectId) -> bool {
        self.attackers
            .iter()
            .any(|a| a.blockers.contains(&creature))
    }
}

impl GameObject {
    fn new(id: ObjectId, card: CardId, def: CardDefId, owner: PlayerId, zone: Zone) -> GameObject {
        GameObject {
            found_def: FoundDef(card_def(&def).ok()),
            id,
            card,
            def,
            owner,
            zone,
            tapped: false,
            summoning_sick: zone == Zone::Battlefield,
            damage: 0,
            deathtouch_damage: false,
            power_bonus: 0,
            toughness_bonus: 0,
            attached_to: None,
            granted_keywords: Vec::new(),
            plus_counters: 0,
            skip_untap: false,
        }
    }
}

/// A spell or ability on the stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackItem {
    /// For a spell, the id of its card object (in `Zone::Stack`).
    pub id: ObjectId,
    pub controller: PlayerId,
    pub kind: StackItemKind,
    pub targets: Vec<Target>,
    /// For a triggered ability, what set it off (see `PendingTrigger`).
    pub subject: Option<ObjectId>,
}

impl StackItem {
    /// What its effects come from: the spell itself, or the permanent or
    /// emblem whose ability it is.
    pub fn source(&self) -> ObjectId {
        match &self.kind {
            StackItemKind::Spell => self.id,
            StackItemKind::Trigger { ability } => ability.source(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StackItemKind {
    Spell,
    /// A triggered ability. It is not a card, so no object has its id.
    Trigger {
        ability: TriggeredAbility,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    Library,
    Hand,
    Battlefield,
    Graveyard,
    Stack,
    /// Where a commander waits to be cast. (Emblems are there too, but
    /// they are not cards.)
    Command,
}

/// The steps of a turn, in order. The two main phases count as steps here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    BeginCombat,
    DeclareAttackers,
    DeclareBlockers,
    /// Only happens when a creature in combat has first strike.
    FirstStrikeDamage,
    CombatDamage,
    EndCombat,
    PostcombatMain,
    End,
    Cleanup,
}

impl Step {
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
    /// Choose attackers one at a time, then confirm.
    DeclareAttackers { player: PlayerId },
    /// Choose blocks one at a time, then confirm.
    DeclareBlockers { player: PlayerId },
    /// Divide the combat damage of `attacker`, which has several
    /// blockers, then confirm.
    AssignCombatDamage {
        player: PlayerId,
        attacker: ObjectId,
    },
    /// Choose the targets of a triggered ability of `source` (a permanent
    /// or an emblem) that is about to go on the stack.
    ChooseTargets { player: PlayerId, source: ObjectId },
    /// Scry or surveil: look at the next card of the library and keep it on
    /// top, or put it on the bottom (scry) or into the graveyard (surveil). `kept` cards have been put back on top so far and
    /// `remaining` are still to look at, this one included.
    Scry {
        player: PlayerId,
        kept: u8,
        remaining: u8,
        /// Surveil: the card not kept goes to the graveyard, not the
        /// bottom of the library.
        surveil: bool,
    },
}

impl Decision {
    pub fn player(&self) -> PlayerId {
        match *self {
            Decision::Mulligan { player }
            | Decision::BottomCards { player, .. }
            | Decision::Priority { player }
            | Decision::Discard { player, .. }
            | Decision::DeclareAttackers { player }
            | Decision::DeclareBlockers { player }
            | Decision::AssignCombatDamage { player, .. }
            | Decision::ChooseTargets { player, .. }
            | Decision::Scry { player, .. } => player,
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
            static_sources: Vec::new(),
            emblems: Vec::new(),
            stack: Vec::new(),
            pending_triggers: Vec::new(),
            stray_tokens: Vec::new(),
            triggered_this_turn: Vec::new(),
            priority_to: starting_player,
            resolving: None,
            combat: None,
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
        let object = self.object(id)?;
        match object.found_def.0 {
            Some(def) if def.id == object.def => Ok(def),
            _ => card_def(&object.def),
        }
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

    pub fn is_on_battlefield(&self, id: ObjectId) -> bool {
        self.objects
            .get(&id)
            .is_some_and(|object| object.zone == Zone::Battlefield)
    }

    /// Whether `id` is a creature on the battlefield.
    pub(crate) fn is_creature(&self, id: ObjectId) -> Result<bool, InternalError> {
        Ok(self.is_on_battlefield(id) && self.def(id)?.is_creature())
    }

    /// The static abilities of the emblems `player` controls.
    fn emblem_statics(&self, player: PlayerId) -> impl Iterator<Item = &StaticAbility> {
        self.emblems
            .iter()
            .filter(move |emblem| emblem.controller == player)
            .flat_map(|emblem| &emblem.def.abilities)
            .filter_map(|ability| match ability {
                AbilityDef::Static(ability) => Some(ability),
                _ => None,
            })
    }

    /// Calls `visit` with every anthem and keyword grant that reaches the
    /// creature `id`: those of its controller's emblems, and those of the
    /// permanents on the battlefield (an Aura on it, or a permanent of its
    /// controller's that affects their creatures). Nothing reaches a card
    /// that isn't a creature on the battlefield.
    fn statics_on(
        &self,
        id: ObjectId,
        mut visit: impl FnMut(&StaticAbility),
    ) -> Result<(), InternalError> {
        let object = self.object(id)?;
        if object.zone != Zone::Battlefield || !self.def(id)?.is_creature() {
            return Ok(());
        }
        let controller = object.owner;
        for ability in self.emblem_statics(controller) {
            visit(ability);
        }
        for &other in &self.static_sources {
            let holder = self.object(other)?;
            for ability in &self.def(other)?.abilities {
                let AbilityDef::Static(ability) = ability else {
                    continue;
                };
                let affects = match ability {
                    StaticAbility::Anthem { affects, .. }
                    | StaticAbility::GrantKeyword { affects, .. } => *affects,
                    StaticAbility::Restriction { .. } => continue,
                };
                let reaches = match affects {
                    Affects::Enchanted => holder.attached_to == Some(id),
                    Affects::CreaturesYouControl => holder.owner == controller,
                    Affects::OtherCreaturesYouControl => holder.owner == controller && other != id,
                };
                if reaches {
                    visit(ability);
                }
            }
        }
        Ok(())
    }

    /// What lasting effects add to a creature's power and toughness:
    /// +1/+1 counters, anthems, and Auras that change them. Not what wears
    /// off at end of turn.
    pub(crate) fn static_bonus(&self, id: ObjectId) -> Result<(i32, i32), InternalError> {
        let mut anthems = Vec::new();
        self.statics_on(id, |ability| {
            if let StaticAbility::Anthem {
                power,
                toughness,
                with_keyword,
                ..
            } = ability
            {
                anthems.push((*power, *toughness, *with_keyword));
            }
        })?;
        let counters = i32::try_from(self.object(id)?.plus_counters).unwrap_or(i32::MAX);
        let mut bonus = (counters, counters);
        for (power, toughness, with_keyword) in anthems {
            let applies = match with_keyword {
                Some(keyword) => self.has_keyword(id, keyword)?,
                None => true,
            };
            if applies {
                bonus.0 += power;
                bonus.1 += toughness;
            }
        }
        Ok(bonus)
    }

    pub(crate) fn power(&self, id: ObjectId) -> Result<i32, InternalError> {
        Ok(self.def(id)?.power.unwrap_or(0)
            + self.object(id)?.power_bonus
            + self.static_bonus(id)?.0)
    }

    pub(crate) fn toughness(&self, id: ObjectId) -> Result<i32, InternalError> {
        Ok(self.def(id)?.toughness.unwrap_or(0)
            + self.object(id)?.toughness_bonus
            + self.static_bonus(id)?.1)
    }

    /// The keywords a creature on the battlefield has been given: until end
    /// of turn, or by an emblem or another permanent.
    fn given_keywords(&self, id: ObjectId) -> Result<Vec<Keyword>, InternalError> {
        let object = self.object(id)?;
        let has_counter = object.plus_counters > 0;
        let mut keywords = Vec::new();
        self.statics_on(id, |ability| {
            if let StaticAbility::GrantKeyword {
                keyword,
                with_counter,
                ..
            } = ability
                && (has_counter || !with_counter)
            {
                keywords.push(*keyword);
            }
        })?;
        if object.zone == Zone::Battlefield && self.def(id)?.is_creature() {
            keywords.extend(&object.granted_keywords);
        }
        Ok(keywords)
    }

    /// Whether the object has the keyword, printed or granted.
    pub(crate) fn has_keyword(
        &self,
        id: ObjectId,
        keyword: Keyword,
    ) -> Result<bool, InternalError> {
        Ok(self.def(id)?.has_keyword(keyword) || self.given_keywords(id)?.contains(&keyword))
    }

    /// The keywords a permanent has, printed first and then granted.
    pub(crate) fn keywords(&self, id: ObjectId) -> Result<Vec<Keyword>, InternalError> {
        let mut keywords = self.def(id)?.keywords.clone();
        for keyword in self.given_keywords(id)? {
            if !keywords.contains(&keyword) {
                keywords.push(keyword);
            }
        }
        Ok(keywords)
    }

    /// Adds an emblem to the command zone.
    pub(crate) fn add_emblem(&mut self, controller: PlayerId, def: EmblemDef) -> ObjectId {
        let id = self.next_object_id();
        self.emblems.push(Emblem {
            id,
            controller,
            def,
        });
        id
    }
    /// Whether an effect stops this creature from attacking or blocking.
    /// So far only an Aura's "enchanted creature can't..." does.
    pub(crate) fn is_restricted(
        &self,
        creature: ObjectId,
        from: Restricted,
    ) -> Result<bool, InternalError> {
        for &id in &self.battlefield {
            if self.object(id)?.attached_to != Some(creature) {
                continue;
            }
            for ability in &self.def(id)?.abilities {
                if let AbilityDef::Static(StaticAbility::Restriction {
                    affects: Affects::Enchanted,
                    cant,
                }) = ability
                    && cant.contains(&from)
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// The creatures a player controls, in the order they entered.
    pub(crate) fn creatures_of(&self, player: PlayerId) -> Result<Vec<ObjectId>, InternalError> {
        let mut creatures = Vec::new();
        for permanent in self.permanents_of(player) {
            if self.def(permanent.id)?.is_creature() {
                creatures.push(permanent.id);
            }
        }
        Ok(creatures)
    }

    /// The permanents a player controls, in the order they entered.
    pub fn permanents_of(&self, player: PlayerId) -> impl Iterator<Item = &GameObject> {
        self.battlefield
            .iter()
            .filter_map(|id| self.objects.get(id))
            .filter(move |object| object.owner == player)
    }

    /// The list of object ids for a zone. The stack has no such list: its
    /// cards are tracked by their `StackItem`s.
    fn zone_mut(&mut self, owner: PlayerId, zone: Zone) -> Option<&mut Vec<ObjectId>> {
        match zone {
            Zone::Library => Some(&mut self.player_mut(owner).library),
            Zone::Hand => Some(&mut self.player_mut(owner).hand),
            Zone::Graveyard => Some(&mut self.player_mut(owner).graveyard),
            Zone::Battlefield => Some(&mut self.battlefield),
            Zone::Command => Some(&mut self.player_mut(owner).command),
            Zone::Stack => None,
        }
    }

    /// Whether the object is its owner's commander, in whatever zone.
    pub(crate) fn is_commander(&self, id: ObjectId) -> bool {
        self.objects
            .get(&id)
            .is_some_and(|object| self.player(object.owner).commander == Some(object.card))
    }

    /// What `player`'s commander costs to cast from the command zone now:
    /// its mana cost plus the tax for each earlier cast from there.
    pub(crate) fn commander_cost(&self, player: PlayerId, def: &CardDef) -> crate::cards::ManaCost {
        let tax = self
            .rules
            .commander_tax
            .saturating_mul(self.player(player).commander_casts);
        def.mana_cost.plus_generic(tax)
    }

    /// What casting the card costs its owner from where it is: a commander
    /// in the command zone pays its tax.
    pub(crate) fn casting_cost(
        &self,
        id: ObjectId,
    ) -> Result<crate::cards::ManaCost, InternalError> {
        let object = self.object(id)?;
        let def = self.def(id)?;
        Ok(if object.zone == Zone::Command {
            self.commander_cost(object.owner, def)
        } else {
            def.mana_cost.clone()
        })
    }

    pub(crate) fn next_object_id(&mut self) -> ObjectId {
        let id = ObjectId(self.next_object_id);
        self.next_object_id += 1;
        id
    }

    fn add_to_zone(&mut self, object: GameObject) {
        let (id, owner, zone) = (object.id, object.owner, object.zone);
        let has_statics = card_def(&object.def).is_ok_and(|def| {
            def.abilities.iter().any(|ability| {
                matches!(
                    ability,
                    AbilityDef::Static(
                        StaticAbility::Anthem { .. } | StaticAbility::GrantKeyword { .. }
                    )
                )
            })
        });
        if zone == Zone::Battlefield && has_statics {
            self.static_sources.push(id);
        }
        self.objects.insert(id, object);
        match (zone, self.zone_mut(owner, zone)) {
            (Zone::Library, Some(list)) => list.insert(0, id),
            (_, Some(list)) => list.push(id),
            // The caller pushes the stack item.
            (_, None) => {}
        }
    }

    /// Adds a brand-new card to a zone (the bottom, for the library). A
    /// card created on the stack still needs its `StackItem`.
    pub(crate) fn create_card(&mut self, owner: PlayerId, def: CardDefId, zone: Zone) -> ObjectId {
        let id = self.next_object_id();
        let card = CardId(self.next_card_id);
        self.next_card_id += 1;
        self.add_to_zone(GameObject::new(id, card, def, owner, zone));
        id
    }

    /// Moves a card to another zone. The card becomes a new object with none
    /// of the old one's status, and the new id is returned. A card moved to
    /// the library goes to the bottom; one moved to the stack still needs
    /// its `StackItem`.
    ///
    /// A commander on its way to a graveyard goes to its owner's command
    /// zone instead, so the caller should look at where the new object is.
    pub(crate) fn move_object(
        &mut self,
        id: ObjectId,
        to: Zone,
    ) -> Result<ObjectId, InternalError> {
        let to = if to == Zone::Graveyard && self.is_commander(id) {
            Zone::Command
        } else {
            to
        };
        let (owner, from) = {
            let object = self.object(id)?;
            (object.owner, object.zone)
        };
        let not_in_zone = || InternalError::new(format!("{id:?} is not in its zone {from:?}"));
        match self.zone_mut(owner, from) {
            Some(list) => {
                let position = list
                    .iter()
                    .position(|listed| *listed == id)
                    .ok_or_else(not_in_zone)?;
                list.remove(position);
            }
            None => {
                let position = self
                    .stack
                    .iter()
                    .position(|item| item.id == id)
                    .ok_or_else(not_in_zone)?;
                self.stack.remove(position);
            }
        }
        let old = self
            .objects
            .remove(&id)
            .ok_or_else(|| InternalError::new(format!("no object {id:?}")))?;
        self.static_sources.retain(|source| *source != id);

        let new_id = self.next_object_id();
        if to != Zone::Battlefield && card_def(&old.def)?.token {
            self.stray_tokens.push(new_id);
        }
        self.add_to_zone(GameObject::new(new_id, old.card, old.def, old.owner, to));
        Ok(new_id)
    }

    /// Deals the physical cards of `objects` out again among those same
    /// objects, in an order taken from `rng`. For the AI's guesses at cards
    /// it can't see: only for objects in a hand or a library, where a card
    /// has no status of its own.
    pub(crate) fn redeal_cards(
        &mut self,
        objects: &[ObjectId],
        rng: &mut DuelRng,
    ) -> Result<(), InternalError> {
        let mut cards = Vec::with_capacity(objects.len());
        for &id in objects {
            let object = self.object_mut(id)?;
            let def = std::mem::replace(&mut object.def, CardDefId(String::new()));
            cards.push((object.card, def, object.found_def));
        }
        // Start from the cards in a fixed order, so the deal depends on the
        // dice alone and not on where the cards really were.
        cards.sort_by_key(|(card, _, _)| *card);
        rng.shuffle(&mut cards);
        for (&id, (card, def, found_def)) in objects.iter().zip(cards) {
            let object = self.object_mut(id)?;
            object.card = card;
            object.def = def;
            object.found_def = found_def;
        }
        Ok(())
    }

    /// Makes the object a copy of another card. For the AI's guesses at a
    /// card it can't see, in a hand or a library.
    pub(crate) fn imagine_card(
        &mut self,
        id: ObjectId,
        def: &'static CardDef,
    ) -> Result<(), InternalError> {
        let object = self.object_mut(id)?;
        object.def = def.id.clone();
        object.found_def = FoundDef(Some(def));
        Ok(())
    }

    /// Tokens that have left the battlefield cease to exist. Returns
    /// whether any did.
    pub(crate) fn remove_stray_tokens(&mut self) -> bool {
        let strays = std::mem::take(&mut self.stray_tokens);
        for &id in &strays {
            if let Some(object) = self.objects.remove(&id) {
                if let Some(list) = self.zone_mut(object.owner, object.zone) {
                    list.retain(|listed| *listed != id);
                }
                self.stack.retain(|item| item.id != id);
            }
        }
        !strays.is_empty()
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
                (Zone::Command, &state.command),
            ] {
                for id in list {
                    assert!(
                        listed.insert(*id, (player, zone)).is_none(),
                        "{id:?} is listed twice"
                    );
                }
            }
        }
        for item in &self.stack {
            if let StackItemKind::Trigger { .. } = item.kind {
                assert!(!self.objects.contains_key(&item.id));
                continue;
            }
            let owner = self.objects[&item.id].owner;
            assert!(
                listed.insert(item.id, (owner, Zone::Stack)).is_none(),
                "{:?} is listed twice",
                item.id
            );
        }
        for id in &self.battlefield {
            let owner = self.objects[id].owner;
            assert!(
                listed.insert(*id, (owner, Zone::Battlefield)).is_none(),
                "{id:?} is listed twice"
            );
        }
        assert_eq!(listed.len(), self.objects.len());
        for id in &self.static_sources {
            assert!(
                self.battlefield.contains(id),
                "{id:?} is a stale static source"
            );
        }
        let mut cards = std::collections::BTreeSet::new();
        for (id, object) in &self.objects {
            assert_eq!(object.id, *id);
            assert_eq!(listed.get(id), Some(&(object.owner, object.zone)));
            assert!(cards.insert(object.card), "{:?} appears twice", object.card);
        }
    }
}
