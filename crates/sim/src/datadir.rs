//! `--data DIR`: reading the game's data from a folder when the sim starts,
//! in place of the data built into the engine, so that a changed card or
//! enemy deck can be measured without a rebuild. The engine does no file
//! IO; this hands it the text.

use std::fs;
use std::path::Path;

use engine::data::{self, DataFile, DataFiles, GameData, RunFiles};

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("can't read {}: {error}", path.display()))
}

/// The `.json` files directly inside a folder: name without the extension,
/// and text, in name order.
fn json_files(folder: &Path) -> Result<Vec<(String, String)>, String> {
    let entries = fs::read_dir(folder)
        .map_err(|error| format!("can't read {}: {error}", folder.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        files.push((name.to_string(), read(&path)?));
    }
    files.sort();
    Ok(files)
}

fn borrowed(files: &[(String, String)]) -> Vec<DataFile<'_>> {
    files
        .iter()
        .map(|(name, text)| DataFile { name, text })
        .collect()
}

/// Loads and checks the data in `dir`, a folder laid out like `data/`.
pub fn load(dir: &str) -> Result<GameData, String> {
    let dir = Path::new(dir);
    let cards = json_files(&dir.join("cards"))?;
    let tokens = json_files(&dir.join("tokens"))?;
    let decks = json_files(&dir.join("decks"))?;
    let balance = read(&dir.join("balance.json"))?;
    let run = |name: &str| read(&dir.join("run").join(format!("{name}.json")));
    let (planeswalkers, emblems, relics, wounds, enemies, events, feats) = (
        run("planeswalkers")?,
        run("emblems")?,
        run("relics")?,
        run("wounds")?,
        run("enemies")?,
        run("events")?,
        run("feats")?,
    );
    let files = DataFiles {
        cards: borrowed(&cards),
        tokens: borrowed(&tokens),
        decks: borrowed(&decks),
        balance: &balance,
        run: RunFiles {
            planeswalkers: &planeswalkers,
            emblems: &emblems,
            relics: &relics,
            wounds: &wounds,
            enemies: &enemies,
            events: &events,
            feats: &feats,
        },
    };
    GameData::load(&files).map_err(|error| format!("{}: {error}", dir.display()))
}

/// Loads `dir` and puts it in place of the engine's own data for the rest
/// of the program.
pub fn install(dir: &str) -> Result<(), String> {
    let loaded = load(dir)?;
    if data::replace_embedded(loaded) {
        Ok(())
    } else {
        Err("--data: the data was already replaced".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_repos_data_folder_loads_to_what_the_engine_has_built_in() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data");
        let loaded = load(dir).unwrap();
        assert_eq!(&loaded, data::embedded().unwrap());
        assert!(load("no/such/folder").is_err());
    }
}
