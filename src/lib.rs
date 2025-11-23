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
    cached_hero_range: Option<HoldemRange>,
    cached_vs_range: Option<HoldemRange>,
    cached_omaha_range: Option<OmahaRange>,
}

#[wasm_bindgen]
impl EquityCalculator {
    #[wasm_bindgen(constructor)]
    pub fn new(data: Vec<u8>) -> Self {
        EquityCalculator {
            hand_ranks_data: data,
            cached_hero_range: None,
            cached_vs_range: None,
            cached_omaha_range: None,
        }
    }

    /// Set the cached hero range for Holdem calculations
    /// Call this once before using cached methods to avoid repeated memory transfers
    #[wasm_bindgen(js_name = setHeroRange)]
    pub fn set_hero_range(&mut self, range: HoldemRange) {
        self.cached_hero_range = Some(range);
    }

    /// Set the cached villain range for Holdem calculations
    /// Call this once before using cached methods to avoid repeated memory transfers
    #[wasm_bindgen(js_name = setVsRange)]
    pub fn set_vs_range(&mut self, range: HoldemRange) {
        self.cached_vs_range = Some(range);
    }

    /// Set the cached Omaha range for Omaha calculations
    /// Call this once before using cached methods to avoid repeated memory transfers
    #[wasm_bindgen(js_name = setOmahaRange)]
    pub fn set_omaha_range(&mut self, range: OmahaRange) {
        self.cached_omaha_range = Some(range);
    }

    /// Calculate equity for each hand in hero_range vs vs_range
    /// Enumerates all possible runouts for incomplete boards (3 or 4 cards)
    /// IMPORTANT: Call setHeroRange and setVsRange before using this method
    #[wasm_bindgen]
    pub fn equity_vs_range(
        &self,
        board: &[u8],
    ) -> Result<Vec<EquityResult>, String> {
        let hero_range = self.cached_hero_range.as_ref()
            .ok_or("No hero range set. Call setHeroRange first.")?;
        let vs_range = self.cached_vs_range.as_ref()
            .ok_or("No villain range set. Call setVsRange first.")?;

        equity::holdem::calculate_equity_vs_range(
            &self.hand_ranks_data,
            hero_range,
            vs_range,
            board
        )
    }

    /// Calculate leaf equity (5-card board only, no enumeration)
    /// IMPORTANT: Call setHeroRange and setVsRange before using this method
    #[wasm_bindgen]
    pub fn leaf_equity_vs_range(
        &self,
        board: &[u8],
    ) -> Result<Vec<EquityResult>, String> {
        let hero_range = self.cached_hero_range.as_ref()
            .ok_or("No hero range set. Call setHeroRange first.")?;
        let vs_range = self.cached_vs_range.as_ref()
            .ok_or("No villain range set. Call setVsRange first.")?;

        Ok(equity::holdem::calculate_leaf_equity(
            &self.hand_ranks_data,
            hero_range,
            vs_range,
            board
        ))
    }

    /// Calculate Omaha equity for a single hand vs a range
    /// Returns equity for each possible runout
    /// IMPORTANT: Call setOmahaRange before using this method
    #[wasm_bindgen(js_name = omahaEquityVsRange)]
    pub fn omaha_equity_vs_range(
        &self,
        hero_hand: &[u8],
        board: &[u8],
    ) -> Result<Vec<RunoutEquities>, String> {
        let vs_range = self.cached_omaha_range.as_ref()
            .ok_or("No Omaha range set. Call setOmahaRange first.")?;

        equity::omaha::calculate_omaha_equity_vs_range(
            &self.hand_ranks_data,
            hero_hand,
            vs_range,
            board
        )
    }

    /// Calculate Omaha leaf equity (5-card board only, no enumeration)
    /// hero_hand must be 4, 5, or 6 cards (matching the range)
    /// board must be exactly 5 cards
    /// IMPORTANT: Call setOmahaRange before using this method
    #[wasm_bindgen(js_name = omahaLeafEquityVsRange)]
    pub fn omaha_leaf_equity_vs_range(
        &self,
        hero_hand: &[u8],
        board: &[u8],
    ) -> Result<RunoutEquities, String> {
        let vs_range = self.cached_omaha_range.as_ref()
            .ok_or("No Omaha range set. Call setOmahaRange first.")?;

        if ![4, 5, 6].contains(&hero_hand.len()) {
            return Err(format!("Hero hand must be 4, 5, or 6 cards, got {}", hero_hand.len()));
        }
        if hero_hand.len() != vs_range.hand_size() {
            return Err(format!(
                "Hero hand size ({}) must match range hand size ({})",
                hero_hand.len(),
                vs_range.hand_size()
            ));
        }
        if board.len() != 5 {
            return Err("Board must be exactly 5 cards".to_string());
        }

        let board_cards = [board[0], board[1], board[2], board[3], board[4]];

        Ok(equity::omaha::calculate_omaha_leaf_equity(
            &self.hand_ranks_data,
            hero_hand,
            vs_range,
            &board_cards
        ))
    }

    /// Calculate Omaha equity using Monte Carlo simulation on the flop
    /// hero_hand must be 4, 5, or 6 cards (matching the range)
    /// flop must be exactly 3 cards
    /// num_runouts controls accuracy vs speed tradeoff
    /// IMPORTANT: Call setOmahaRange before using this method
    #[wasm_bindgen(js_name = omahaMonteCarloFlop)]
    pub fn omaha_monte_carlo_flop(
        &self,
        hero_hand: &[u8],
        flop: &[u8],
        num_runouts: usize,
    ) -> Result<Vec<RunoutEquities>, String> {
        let vs_range = self.cached_omaha_range.as_ref()
            .ok_or("No Omaha range set. Call setOmahaRange first.")?;

        if ![4, 5, 6].contains(&hero_hand.len()) {
            return Err(format!("Hero hand must be 4, 5, or 6 cards, got {}", hero_hand.len()));
        }
        if hero_hand.len() != vs_range.hand_size() {
            return Err(format!(
                "Hero hand size ({}) must match range hand size ({})",
                hero_hand.len(),
                vs_range.hand_size()
            ));
        }
        if flop.len() != 3 {
            return Err("Flop must be exactly 3 cards".to_string());
        }

        let flop_cards = [flop[0], flop[1], flop[2]];

        Ok(equity::omaha::calculate_omaha_equity_monte_carlo_flop(
            &self.hand_ranks_data,
            hero_hand,
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
