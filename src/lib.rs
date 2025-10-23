use wasm_bindgen::prelude::*;

// Module declarations
mod evaluation;
mod equity;
mod range;
mod types;

// Re-exports for use throughout the crate and externally
pub use evaluation::*;
pub use equity::*;
pub use range::*;
pub use types::*;

/// Main calculator struct - holds hand evaluation data
#[wasm_bindgen]
pub struct EquityCalculator {
    hand_ranks_data: Vec<u8>,
}

#[wasm_bindgen]
impl EquityCalculator {
    #[wasm_bindgen(constructor)]
    pub fn new(data: Vec<u8>) -> Self {
        EquityCalculator {
            hand_ranks_data: data,
        }
    }

    /// Calculate equity for each hand in hero_range vs vs_range
    /// Enumerates all possible runouts for incomplete boards (3 or 4 cards)
    #[wasm_bindgen]
    pub fn equity_vs_range(
        &self,
        hero_range: &HoldemRange,
        vs_range: &HoldemRange,
        board: &[u8],
    ) -> Result<Vec<EquityResult>, String> {
        equity::holdem::calculate_equity_vs_range(
            &self.hand_ranks_data,
            hero_range,
            vs_range,
            board
        )
    }

    /// Calculate leaf equity (5-card board only, no enumeration)
    #[wasm_bindgen]
    pub fn leaf_equity_vs_range(
        &self,
        hero_range: &HoldemRange,
        vs_range: &HoldemRange,
        board: &[u8],
    ) -> Vec<EquityResult> {
        equity::holdem::calculate_leaf_equity(
            &self.hand_ranks_data,
            hero_range,
            vs_range,
            board
        )
    }

    /// Calculate Omaha equity for a single hand vs a range
    /// Returns equity for each possible runout
    #[wasm_bindgen(js_name = omahaEquityVsRange)]
    pub fn omaha_equity_vs_range(
        &self,
        hero_hand: &[u8],
        vs_range: &OmahaRange,
        board: &[u8],
    ) -> Result<Vec<RunoutEquities>, String> {
        equity::omaha::calculate_omaha_equity_vs_range(
            &self.hand_ranks_data,
            hero_hand,
            vs_range,
            board
        )
    }

    /// Calculate Omaha leaf equity (5-card board only, no enumeration)
    /// hero_hand must be exactly 4 cards
    /// board must be exactly 5 cards
    #[wasm_bindgen(js_name = omahaLeafEquityVsRange)]
    pub fn omaha_leaf_equity_vs_range(
        &self,
        hero_hand: &[u8],
        vs_range: &OmahaRange,
        board: &[u8],
    ) -> Result<RunoutEquities, String> {
        if hero_hand.len() != 4 {
            return Err("Hero hand must be exactly 4 cards".to_string());
        }
        if board.len() != 5 {
            return Err("Board must be exactly 5 cards".to_string());
        }

        let hero_cards = [hero_hand[0], hero_hand[1], hero_hand[2], hero_hand[3]];
        let board_cards = [board[0], board[1], board[2], board[3], board[4]];

        Ok(equity::omaha::calculate_omaha_leaf_equity(
            &self.hand_ranks_data,
            &hero_cards,
            vs_range,
            &board_cards
        ))
    }

    /// Calculate Omaha equity using Monte Carlo simulation on the flop
    /// hero_hand must be exactly 4 cards
    /// flop must be exactly 3 cards
    /// num_runouts controls accuracy vs speed tradeoff
    #[wasm_bindgen(js_name = omahaMonteCarloFlop)]
    pub fn omaha_monte_carlo_flop(
        &self,
        hero_hand: &[u8],
        vs_range: &OmahaRange,
        flop: &[u8],
        num_runouts: usize,
    ) -> Result<Vec<RunoutEquities>, String> {
        if hero_hand.len() != 4 {
            return Err("Hero hand must be exactly 4 cards".to_string());
        }
        if flop.len() != 3 {
            return Err("Flop must be exactly 3 cards".to_string());
        }

        let hero_cards = [hero_hand[0], hero_hand[1], hero_hand[2], hero_hand[3]];
        let flop_cards = [flop[0], flop[1], flop[2]];

        Ok(equity::omaha::calculate_omaha_equity_monte_carlo_flop(
            &self.hand_ranks_data,
            &hero_cards,
            vs_range,
            &flop_cards,
            num_runouts
        ))
    }
}

// --- WASM Bindings for Types ---
// The types are defined in their respective modules, we just add WASM bindings here

#[wasm_bindgen]
impl Equity {
    // Expose fields through getters for WASM
    #[wasm_bindgen(getter)]
    pub fn win(&self) -> f32 {
        self.win
    }

    #[wasm_bindgen(getter)]
    pub fn tie(&self) -> f32 {
        self.tie
    }

    #[wasm_bindgen(getter)]
    pub fn lose(&self) -> f32 {
        self.lose
    }
}

#[wasm_bindgen]
impl EquityResult {
    #[wasm_bindgen(getter)]
    pub fn combo(&self) -> Vec<u8> {
        self.combo.to_vec()
    }

    #[wasm_bindgen(getter)]
    pub fn equity(&self) -> Equity {
        self.equity
    }

    #[wasm_bindgen(getter)]
    pub fn hand_idx(&self) -> usize {
        self.hand_idx
    }
}

// HoldemRange WASM bindings are in range/holdem.rs
