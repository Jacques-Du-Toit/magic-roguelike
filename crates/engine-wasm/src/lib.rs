//! Thin wasm-bindgen wrapper around the engine. It only serialises in and
//! out; it contains no game logic. Data crosses the boundary as JSON strings
//! (see `docs/ARCHITECTURE.md` §8).

use wasm_bindgen::prelude::*;

/// Returns the engine's `EngineInfo` as a JSON string.
#[wasm_bindgen]
pub fn engine_info() -> Result<String, JsError> {
    engine_info_json().map_err(|e| JsError::new(&e.to_string()))
}

fn engine_info_json() -> Result<String, serde_json::Error> {
    serde_json::to_string(&engine::engine_info())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_info_json_round_trips() {
        let json = engine_info_json().unwrap();
        let info: engine::EngineInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, engine::engine_info());
    }
}
