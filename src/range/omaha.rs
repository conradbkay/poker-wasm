use wasm_bindgen::prelude::*;

/// Omaha range representation - simple array of hands with weights
/// Supports PLO4 (4 cards), PLO5 (5 cards), and PLO6 (6 cards)
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct OmahaRange {
    // Parallel arrays for WASM compatibility
    // Using max size array [u8; 6], with hand_size indicating actual cards used
    hands: Vec<[u8; 6]>,
    weights: Vec<f32>,
    hand_size: usize, // 4, 5, or 6
}

#[wasm_bindgen]
impl OmahaRange {
    /// Create a new empty Omaha range with specified hand size (4, 5, or 6)
    #[wasm_bindgen(constructor)]
    pub fn new(hand_size: usize) -> Self {
        if ![4, 5, 6].contains(&hand_size) {
            panic!("Hand size must be 4, 5, or 6");
        }
        Self {
            hands: Vec::new(),
            weights: Vec::new(),
            hand_size,
        }
    }

    /// Add a hand to the range with a weight
    /// hand must match the range's hand_size (4, 5, or 6 cards)
    #[wasm_bindgen(js_name = addHand)]
    pub fn add_hand(&mut self, hand: &[u8], weight: f32) {
        if hand.len() != self.hand_size {
            panic!("Hand must have exactly {} cards", self.hand_size);
        }
        let mut hand_array = [0u8; 6];
        for (i, &card) in hand.iter().enumerate() {
            hand_array[i] = card;
        }
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

    /// Get the hand size for this range (4, 5, or 6)
    #[wasm_bindgen(js_name = handSize)]
    pub fn hand_size(&self) -> usize {
        self.hand_size
    }
}

// Internal methods (not exposed to WASM)
impl OmahaRange {
    /// Iterator over (hand slice, weight) pairs
    /// Returns only the valid portion of each hand based on hand_size
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], f32)> + '_ {
        let hand_size = self.hand_size;
        self.hands.iter().map(move |h| &h[..hand_size]).zip(self.weights.iter().copied())
    }

    /// Get a specific hand by index (returns slice of valid cards)
    pub fn get_hand(&self, idx: usize) -> Option<&[u8]> {
        self.hands.get(idx).map(|h| &h[..self.hand_size])
    }

    /// Get a specific weight by index
    pub fn get_weight(&self, idx: usize) -> Option<f32> {
        self.weights.get(idx).copied()
    }

    /// Get the hand size for this range
    pub fn get_hand_size(&self) -> usize {
        self.hand_size
    }
}

impl Default for OmahaRange {
    fn default() -> Self {
        Self::new(4) // Default to PLO4
    }
}