use wasm_bindgen::prelude::*;

// Module declarations
mod equity;
mod evaluation;
mod range;
mod types;

// Re-exports for use throughout the crate and externally
pub use equity::*;
pub use evaluation::*;
pub use range::*;
pub use types::*;

/// Main calculator struct - holds cached ranges.
#[wasm_bindgen]
pub struct EquityCalculator {
    cached_hero_range: Option<HoldemRange>,
    cached_vs_range: Option<HoldemRange>,
    cached_omaha_hero_range: Option<OmahaRange>,
    cached_omaha_vs_range: Option<OmahaRange>,
}

#[wasm_bindgen]
impl EquityCalculator {
    /// The evaluator is tableless (lookup tables are baked into the wasm binary),
    /// so no data needs to be passed in.
    #[allow(clippy::new_without_default)]
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        EquityCalculator {
            cached_hero_range: None,
            cached_vs_range: None,
            cached_omaha_hero_range: None,
            cached_omaha_vs_range: None,
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

    /// Set the cached Omaha hero range (the hands whose equity is computed).
    /// Call this once before `omahaEquityVsRange` to avoid repeated memory transfers.
    #[wasm_bindgen(js_name = setOmahaHeroRange)]
    pub fn set_omaha_hero_range(&mut self, range: OmahaRange) {
        self.cached_omaha_hero_range = Some(range);
    }

    /// Set the cached Omaha villain range (the opposing hands).
    /// Call this once before `omahaEquityVsRange` to avoid repeated memory transfers.
    #[wasm_bindgen(js_name = setOmahaVsRange)]
    pub fn set_omaha_vs_range(&mut self, range: OmahaRange) {
        self.cached_omaha_vs_range = Some(range);
    }

    /// Calculate equity for each hand in hero_range vs vs_range
    /// Enumerates all possible runouts for incomplete boards (3 or 4 cards)
    /// IMPORTANT: Call setHeroRange and setVsRange before using this method
    #[wasm_bindgen(js_name = equityVsRange)]
    pub fn equity_vs_range(&self, board: &[u8]) -> Result<Vec<EquityResult>, String> {
        let hero_range = self
            .cached_hero_range
            .as_ref()
            .ok_or("No hero range set. Call setHeroRange first.")?;
        let vs_range = self
            .cached_vs_range
            .as_ref()
            .ok_or("No villain range set. Call setVsRange first.")?;

        equity::holdem::calculate_equity_vs_range(hero_range, vs_range, board)
    }

    /// Equity of every hand in the hero range vs the villain range, aggregated
    /// over all runouts of a 3-, 4-, or 5-card board. A single hero hand is just a
    /// one-hand range. `maxRunouts`, when given on a 3-card board, samples that
    /// many turn/river runouts (Monte Carlo) instead of enumerating all of them.
    /// IMPORTANT: Call setOmahaHeroRange and setOmahaVsRange before using this.
    #[wasm_bindgen(js_name = omahaEquityVsRange)]
    pub fn omaha_equity_vs_range(
        &self,
        board: &[u8],
        max_runouts: Option<usize>,
    ) -> Result<Vec<OmahaEquityResult>, String> {
        let hero_range = self
            .cached_omaha_hero_range
            .as_ref()
            .ok_or("No Omaha hero range set. Call setOmahaHeroRange first.")?;
        let vs_range = self
            .cached_omaha_vs_range
            .as_ref()
            .ok_or("No Omaha villain range set. Call setOmahaVsRange first.")?;

        equity::omaha::calculate_omaha_range_equity(hero_range, vs_range, board, max_runouts)
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

    #[wasm_bindgen(getter, js_name = handIdx)]
    pub fn hand_idx(&self) -> usize {
        self.hand_idx
    }
}

// HoldemRange WASM bindings are in range/holdem.rs
