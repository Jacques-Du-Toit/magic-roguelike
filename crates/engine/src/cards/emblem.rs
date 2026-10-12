//! Emblems: abilities that sit in the command zone for a whole duel. Relics,
//! boons, planeswalker passives and boss emblems all reach the duel as
//! emblems (see `docs/ARCHITECTURE.md` §6). They are built from the same
//! ability and effect types as cards.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use super::{
    AbilityDef, Affects, CardError, Effect, StaticAbility, TargetSpec, Trigger, TriggerSubject,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct EmblemDef {
    pub name: String,
    /// Rules text shown to the players.
    pub text: String,
    pub abilities: Vec<AbilityDef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EmblemError {
    #[error("name is empty")]
    EmptyName,
    #[error("an emblem needs at least one ability")]
    NoAbilities,
    #[error("an emblem can't have a `{0}` ability")]
    UnsupportedAbility(&'static str),
    #[error("an emblem's static ability must affect `creatures_you_control`")]
    Affects,
    #[error("an emblem isn't a permanent: nothing can trigger on it or refer to it")]
    SelfTrigger,
    #[error("an anthem must change power or toughness")]
    EmptyAnthem,
    #[error(transparent)]
    Effect(#[from] CardError),
}

/// Checks everything about an emblem that its JSON shape alone doesn't
/// guarantee.
pub fn validate_emblem(emblem: &EmblemDef) -> Result<(), EmblemError> {
    if emblem.name.trim().is_empty() {
        return Err(EmblemError::EmptyName);
    }
    if emblem.abilities.is_empty() {
        return Err(EmblemError::NoAbilities);
    }
    for ability in &emblem.abilities {
        match ability {
            AbilityDef::Spell { .. } => return Err(EmblemError::UnsupportedAbility("spell")),
            AbilityDef::Mana { .. } => return Err(EmblemError::UnsupportedAbility("mana")),
            AbilityDef::Activated { .. } => {
                return Err(EmblemError::UnsupportedAbility("activated"));
            }
            AbilityDef::Triggered {
                trigger, effects, ..
            } => {
                if matches!(
                    trigger,
                    Trigger::EntersBattlefield {
                        subject: TriggerSubject::This,
                        ..
                    } | Trigger::LeavesBattlefield {}
                        | Trigger::PutIntoGraveyardFromBattlefield {}
                        | Trigger::Dies {
                            subject: TriggerSubject::This
                        }
                        | Trigger::Attacks {}
                        | Trigger::DealsDamageToOpponent {}
                ) {
                    return Err(EmblemError::SelfTrigger);
                }
                if effects.contains(&Effect::SacrificeThis {}) {
                    return Err(EmblemError::SelfTrigger);
                }
                for spec in effects.iter().flat_map(|effect| effect.targets()) {
                    if spec == (TargetSpec::This {}) {
                        return Err(EmblemError::SelfTrigger);
                    }
                    if spec.is_chosen() {
                        return Err(CardError::TargetedTrigger.into());
                    }
                }
                super::validate_triggered(trigger, effects)?;
            }
            AbilityDef::Static(StaticAbility::Restriction { .. }) => {
                return Err(EmblemError::UnsupportedAbility("restriction"));
            }
            AbilityDef::Static(StaticAbility::CantBeBlockedByPower { .. }) => {
                return Err(EmblemError::UnsupportedAbility("cant_be_blocked_by_power"));
            }
            AbilityDef::Static(StaticAbility::ExtraCounters { .. }) => {
                return Err(EmblemError::UnsupportedAbility("extra_counters"));
            }
            AbilityDef::Static(StaticAbility::Anthem {
                affects,
                power,
                toughness,
                ..
            }) => {
                if *affects != Affects::CreaturesYouControl {
                    return Err(EmblemError::Affects);
                }
                if *power == 0 && *toughness == 0 {
                    return Err(EmblemError::EmptyAnthem);
                }
            }
            AbilityDef::Static(StaticAbility::GrantKeyword { affects, .. }) => {
                if *affects != Affects::CreaturesYouControl {
                    return Err(EmblemError::Affects);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn emblem(abilities: Value) -> Result<(), EmblemError> {
        let def: EmblemDef = serde_json::from_value(json!({
            "name": "Test", "text": "Test text.", "abilities": abilities
        }))
        .unwrap();
        validate_emblem(&def)
    }

    #[test]
    fn emblems_of_each_shape_pass() {
        assert_eq!(
            emblem(json!([
                { "kind": "triggered", "trigger": { "type": "beginning_of_upkeep" },
                  "effects": [{ "type": "lose_life", "player": "opponent", "amount": 1 }] },
                { "kind": "triggered",
                  "trigger": { "type": "enters_battlefield", "subject": "creature_you_control" },
                  "effects": [{ "type": "gain_life", "player": "controller", "amount": 1 }] },
                { "kind": "static", "type": "anthem", "affects": "creatures_you_control",
                  "power": 1, "toughness": 0 },
                { "kind": "static", "type": "grant_keyword", "affects": "creatures_you_control",
                  "keyword": "haste" }
            ])),
            Ok(())
        );
    }

    #[test]
    fn an_emblem_may_trigger_on_what_its_controller_does_but_never_on_itself() {
        let on = |trigger: Value, effect: Value| {
            emblem(json!([{ "kind": "triggered", "trigger": trigger, "effects": [effect] }]))
        };
        let gain = json!({ "type": "gain_life", "player": "controller", "amount": 1 });
        for trigger in [
            json!({ "type": "dies", "subject": "creature_you_control" }),
            json!({ "type": "gain_life" }),
            json!({ "type": "cast_spell", "kind": "instant_or_sorcery" }),
        ] {
            assert_eq!(on(trigger, gain.clone()), Ok(()));
        }
        for trigger in [
            json!({ "type": "dies", "subject": "self" }),
            json!({ "type": "attacks" }),
            json!({ "type": "deals_damage_to_opponent" }),
        ] {
            assert_eq!(on(trigger, gain.clone()), Err(EmblemError::SelfTrigger));
        }
        // "That creature" is fine; "this creature" is not.
        let haste = |target: &str| json!({ "type": "grant_keyword_until_eot", "target": { target: {} }, "keyword": "haste" });
        let enters = json!({ "type": "enters_battlefield", "subject": "creature_you_control" });
        assert_eq!(on(enters.clone(), haste("triggering")), Ok(()));
        assert_eq!(on(enters, haste("this")), Err(EmblemError::SelfTrigger));
    }

    #[test]
    fn rejects_emblems_that_could_not_work() {
        assert_eq!(emblem(json!([])), Err(EmblemError::NoAbilities));
        assert_eq!(
            emblem(json!([{ "kind": "mana", "cost": { "tap": true }, "produces": "{G}" }])),
            Err(EmblemError::UnsupportedAbility("mana"))
        );
        assert_eq!(
            emblem(
                json!([{ "kind": "static", "type": "anthem", "affects": "enchanted",
                "power": 1, "toughness": 1 }])
            ),
            Err(EmblemError::Affects)
        );
        assert_eq!(
            emblem(json!([{ "kind": "static", "type": "anthem",
                "affects": "creatures_you_control", "power": 0, "toughness": 0 }])),
            Err(EmblemError::EmptyAnthem)
        );
        assert_eq!(
            emblem(json!([{ "kind": "triggered",
                "trigger": { "type": "enters_battlefield", "subject": "self" },
                "effects": [{ "type": "draw_cards", "player": "controller", "amount": 1 }] }])),
            Err(EmblemError::SelfTrigger)
        );
        assert_eq!(
            emblem(
                json!([{ "kind": "triggered", "trigger": { "type": "beginning_of_upkeep" },
                "effects": [{ "type": "destroy", "target": { "creature": {} } }] }])
            ),
            Err(EmblemError::Effect(CardError::TargetedTrigger))
        );
        assert_eq!(
            emblem(
                json!([{ "kind": "triggered", "trigger": { "type": "beginning_of_upkeep" },
                "effects": [{ "type": "lose_life", "player": "opponent", "amount": 0 }] }])
            ),
            Err(EmblemError::Effect(CardError::ZeroAmount("lose_life")))
        );
    }
}
