use crate::evaluation::IDX2HAND;
use wasm_bindgen::prelude::*;

/// Represents a range of Texas Hold'em hands.
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct HoldemRange {
    pub(crate) range: Vec<f32>, // Weight for each of the 1326 possible hands
}

impl Default for HoldemRange {
    fn default() -> Self {
        Self {
            range: vec![0.0; 1326],
        }
    }
}

#[wasm_bindgen]
impl HoldemRange {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen]
    pub fn get_range(&self) -> Vec<f32> {
        self.range.clone()
    }

    #[wasm_bindgen]
    pub fn get_weight(&self, idx: usize) -> f32 {
        if idx < self.range.len() {
            self.range[idx]
        } else {
            0.0 // Return 0 for out of bounds
        }
    }

    #[wasm_bindgen]
    pub fn set(&mut self, idx: usize, weight: f32) {
        if idx < self.range.len() {
            self.range[idx] = weight;
        }
    }

    #[wasm_bindgen]
    pub fn set_hand(&mut self, hand: &[u8], weight: f32) -> Result<(), String> {
        if hand.len() != 2 {
            return Err("Hand must contain exactly 2 cards".to_string());
        }

        let hand_array = [hand[0], hand[1]];
        let hand_idx = Self::get_hand_idx(hand_array);

        if hand_idx >= 1326 {
            eprintln!("Invalid hand: {hand:?}, index: {hand_idx}");
            return Err("Invalid hand".to_string());
        }

        self.set(hand_idx, weight);
        Ok(())
    }

    #[wasm_bindgen]
    pub fn from_hand_idx_wasm(idx: usize) -> Vec<u8> {
        Self::from_hand_idx(idx).to_vec()
    }

    #[wasm_bindgen]
    pub fn get_hand_idx_wasm(hand: Vec<u8>) -> usize {
        let mut hand_array = [0; 2];
        hand_array.copy_from_slice(&hand[..2]);
        Self::get_hand_idx(hand_array)
    }
}

// Non-WASM impl block for internal Rust use
impl HoldemRange {
    /// Only iterates on combos with weight > 0
    pub fn for_each_weighted<F>(&self, mut f: F)
    where
        F: FnMut(f32, usize),
    {
        for (i, &weight) in self.range.iter().enumerate() {
            if weight > 0.0 {
                f(weight, i);
            }
        }
    }

    pub fn from_hand_idx(idx: usize) -> [u8; 2] {
        if idx < IDX2HAND.len() {
            IDX2HAND[idx]
        } else {
            [1, 0] // Fallback for invalid index
        }
    }

    #[inline]
    pub fn sort_two_cards_desc(hand: &mut [u8; 2]) {
        // sort descending
        if hand[0] < hand[1] {
            hand.swap(0, 1);
        }
    }

    pub fn get_hand_idx(mut hand: [u8; 2]) -> usize {
        Self::sort_two_cards_desc(&mut hand);
        // lower triangular matrix of card pairs
        (hand[0] as usize * (hand[0] as usize - 1)) / 2 + hand[1] as usize
    }
}
