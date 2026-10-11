//! Colours and mana costs.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// One of the five colours of Magic, written as its mana letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum Color {
    W,
    U,
    B,
    R,
    G,
}

impl Color {
    fn from_letter(letter: &str) -> Option<Color> {
        match letter {
            "W" => Some(Color::W),
            "U" => Some(Color::U),
            "B" => Some(Color::B),
            "R" => Some(Color::R),
            "G" => Some(Color::G),
            _ => None,
        }
    }

    fn letter(self) -> &'static str {
        match self {
            Color::W => "W",
            Color::U => "U",
            Color::B => "B",
            Color::R => "R",
            Color::G => "G",
        }
    }
}

/// One symbol of a mana cost, e.g. `{2}` or `{G}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManaSymbol {
    Generic(u32),
    Colored(Color),
}

/// A mana cost such as `{1}{U}{U}`, or an amount of mana such as `{G}`.
///
/// In JSON it is the Scryfall string form. The empty string is "no cost"
/// (lands). Only generic and coloured symbols exist so far; anything else
/// fails to parse.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ManaCost {
    symbols: Vec<ManaSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManaCostError {
    #[error("mana cost `{0}` is not a sequence of {{...}} symbols")]
    Malformed(String),
    #[error("unsupported mana symbol `{{{0}}}`")]
    UnsupportedSymbol(String),
}

impl ManaCost {
    pub fn symbols(&self) -> &[ManaSymbol] {
        &self.symbols
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Total generic mana in the cost.
    pub fn generic(&self) -> u32 {
        self.symbols
            .iter()
            .map(|symbol| match symbol {
                ManaSymbol::Generic(amount) => *amount,
                ManaSymbol::Colored(_) => 0,
            })
            .sum()
    }

    /// The coloured symbols, in printed order, with repeats.
    pub fn colored(&self) -> impl Iterator<Item = Color> + '_ {
        self.symbols.iter().filter_map(|symbol| match symbol {
            ManaSymbol::Generic(_) => None,
            ManaSymbol::Colored(color) => Some(*color),
        })
    }

    /// The set of colours that appear in the cost.
    pub fn colors(&self) -> BTreeSet<Color> {
        self.colored().collect()
    }

    pub fn mana_value(&self) -> u32 {
        self.symbols
            .iter()
            .map(|symbol| match symbol {
                ManaSymbol::Generic(amount) => *amount,
                ManaSymbol::Colored(_) => 1,
            })
            .sum()
    }
}

impl FromStr for ManaCost {
    type Err = ManaCostError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let malformed = || ManaCostError::Malformed(text.to_string());
        let mut symbols = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            let inner = rest.strip_prefix('{').ok_or_else(malformed)?;
            let (symbol, after) = inner.split_once('}').ok_or_else(malformed)?;
            symbols.push(parse_symbol(symbol)?);
            rest = after;
        }
        Ok(ManaCost { symbols })
    }
}

fn parse_symbol(symbol: &str) -> Result<ManaSymbol, ManaCostError> {
    if let Some(color) = Color::from_letter(symbol) {
        return Ok(ManaSymbol::Colored(color));
    }
    let is_number = !symbol.is_empty() && symbol.bytes().all(|byte| byte.is_ascii_digit());
    if is_number && let Ok(amount) = symbol.parse::<u32>() {
        return Ok(ManaSymbol::Generic(amount));
    }
    Err(ManaCostError::UnsupportedSymbol(symbol.to_string()))
}

impl fmt::Display for ManaCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for symbol in &self.symbols {
            match symbol {
                ManaSymbol::Generic(amount) => write!(f, "{{{amount}}}")?,
                ManaSymbol::Colored(color) => write!(f, "{{{}}}", color.letter())?,
            }
        }
        Ok(())
    }
}

impl TryFrom<String> for ManaCost {
    type Error = ManaCostError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<ManaCost> for String {
    fn from(cost: ManaCost) -> String {
        cost.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cost(text: &str) -> ManaCost {
        text.parse().unwrap()
    }

    #[test]
    fn parses_generic_and_coloured_symbols() {
        let cancel = cost("{1}{U}{U}");
        assert_eq!(
            cancel.symbols(),
            [
                ManaSymbol::Generic(1),
                ManaSymbol::Colored(Color::U),
                ManaSymbol::Colored(Color::U)
            ]
        );
        assert_eq!(cancel.generic(), 1);
        assert_eq!(cancel.colored().collect::<Vec<_>>(), [Color::U, Color::U]);
        assert_eq!(cancel.mana_value(), 3);
        assert_eq!(cancel.colors(), BTreeSet::from([Color::U]));
    }

    #[test]
    fn parses_multi_digit_generic() {
        assert_eq!(cost("{12}{G}").mana_value(), 13);
    }

    #[test]
    fn empty_string_is_no_cost() {
        let none = cost("");
        assert!(none.is_empty());
        assert_eq!(none.mana_value(), 0);
    }

    #[test]
    fn displays_in_scryfall_form() {
        for text in ["", "{G}", "{4}{G}{G}", "{1}{W}", "{0}"] {
            assert_eq!(cost(text).to_string(), text);
        }
    }

    #[test]
    fn rejects_malformed_costs() {
        for text in ["1U", "{1", "1}", "{1}U", "{1}{", " {1}"] {
            assert_eq!(
                text.parse::<ManaCost>(),
                Err(ManaCostError::Malformed(text.to_string())),
                "{text}"
            );
        }
    }

    #[test]
    fn rejects_symbols_outside_scope() {
        for symbol in ["X", "W/U", "C", "S", "", "+1", "g"] {
            assert_eq!(
                format!("{{{symbol}}}").parse::<ManaCost>(),
                Err(ManaCostError::UnsupportedSymbol(symbol.to_string())),
                "{symbol}"
            );
        }
    }

    #[test]
    fn serialises_as_a_json_string() {
        let json = serde_json::to_string(&cost("{2}{R}")).unwrap();
        assert_eq!(json, "\"{2}{R}\"");
        assert_eq!(
            serde_json::from_str::<ManaCost>(&json).unwrap(),
            cost("{2}{R}")
        );
        assert!(serde_json::from_str::<ManaCost>("\"{X}\"").is_err());
    }
}
