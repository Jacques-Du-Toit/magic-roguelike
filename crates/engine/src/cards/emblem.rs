//! Emblems: abilities that sit in the command zone for a whole duel. Relics,
//! boons, planeswalker passives and boss emblems all reach the duel as
//! emblems (see `docs/ARCHITECTURE.md` §6). They are built from the same
//! ability and effect types as cards.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use super::{AbilityDef, Affects, CardError, StaticAbility, Trigger, TriggerSubject};

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
    #[error("an emblem can't trigger on itself entering the battlefield")]
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
            AbilityDef::Triggered { trigger, effects } => {
                if matches!(
                    trigger,
                    Trigger::EntersBattlefield {
                        subject: TriggerSubject::This
                    }
                ) {
                    return Err(EmblemError::SelfTrigger);
                }
                if effects.is_empty() {
                    return Err(CardError::NoEffects.into());
                }
                for effect in effects {
                    if !effect.targets().is_empty() {
                        return Err(CardError::TargetedTrigger.into());
                    }
                    super::validate_effect(effect)?;
                }
            }
            AbilityDef::Static(StaticAbility::Restriction { .. }) => {
                return Err(EmblemError::UnsupportedAbility("restriction"));
            }
            AbilityDef::Static(StaticAbility::Anthem {
                affects,
                power,
                toughness,
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
