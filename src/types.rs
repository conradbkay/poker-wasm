use wasm_bindgen::prelude::*;

/// Equity breakdown (win/tie/lose weights)
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Equity {
    pub(crate) win: f32,
    pub(crate) tie: f32,
    pub(crate) lose: f32,
}

/// Result for a single hand combo
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct EquityResult {
    pub(crate) combo: [u8; 2],
    pub(crate) hand_idx: usize,
    pub(crate) equity: Equity,
}
