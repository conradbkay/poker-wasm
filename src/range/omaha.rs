use wasm_bindgen::prelude::*;

/// Omaha range representation - simple array of hands with weights
/// Each hand is 4 hole cards (PLO4)
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct OmahaRange {
    // Parallel arrays for WASM compatibility
    hands: Vec<[u8; 4]>,
    weights: Vec<f32>,
}

#[wasm_bindgen]
impl OmahaRange {
    /// Create a new empty Omaha range
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            hands: Vec::new(),
            weights: Vec::new(),
        }
    }

    /// Add a hand to the range with a weight
    /// hand must be exactly 4 cards (PLO4)
    #[wasm_bindgen(js_name = addHand)]
    pub fn add_hand(&mut self, hand: &[u8], weight: f32) {
        if hand.len() != 4 {
            panic!("Omaha hand must be exactly 4 cards");
        }
        let hand_array = [hand[0], hand[1], hand[2], hand[3]];
        self.hands.push(hand_array);
        self.weights.push(weight);
    }

    /// Get the number of hands in the range
    #[wasm_bindgen(getter)]
    pub fn len(&self) -> usize {
        self.hands.len()
    }

    /// Check if the range is empty
    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.hands.is_empty()
    }
}

// Internal methods (not exposed to WASM)
impl OmahaRange {
    /// Iterator over (hand, weight) pairs
    pub fn iter(&self) -> impl Iterator<Item = (&[u8; 4], f32)> + '_ {
        self.hands.iter().zip(self.weights.iter().copied())
    }

    /// Get a specific hand by index
    pub fn get_hand(&self, idx: usize) -> Option<&[u8; 4]> {
        self.hands.get(idx)
    }

    /// Get a specific weight by index
    pub fn get_weight(&self, idx: usize) -> Option<f32> {
        self.weights.get(idx).copied()
    }
}

impl Default for OmahaRange {
    fn default() -> Self {
        Self::new()
    }
}