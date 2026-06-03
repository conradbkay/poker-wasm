use holdem_hand_evaluator::Hand;
use wasm_bindgen::prelude::*;

const PLO4_SUBSET_SLOT_COUNT: usize = 52 + 1326 + 22100 + 270725;
const INVALID_SUBSET_SLOT: u32 = u32::MAX;

#[inline]
fn choose2(n: usize) -> usize {
    n * (n - 1) / 2
}

#[inline]
fn choose3(n: usize) -> usize {
    n * (n - 1) * (n - 2) / 6
}

#[inline]
fn choose4(n: usize) -> usize {
    n * (n - 1) * (n - 2) * (n - 3) / 24
}

#[inline]
fn plo4_subset_direct_index(mask: u64) -> usize {
    let mut cards = [0usize; 4];
    let mut n = 0usize;
    let mut m = mask;
    while m != 0 {
        cards[n] = m.trailing_zeros() as usize;
        n += 1;
        m &= m - 1;
    }

    match n {
        1 => cards[0],
        2 => 52 + cards[0] + choose2(cards[1]),
        3 => 52 + 1326 + cards[0] + choose2(cards[1]) + choose3(cards[2]),
        4 => {
            52 + 1326 + 22100 + cards[0] + choose2(cards[1]) + choose3(cards[2]) + choose4(cards[3])
        }
        _ => unreachable!("PLO4 direct subset slots require 1-4 card subsets"),
    }
}

/// Open-addressing `u64 -> dense-slot` map for cached blocker subset slots.
/// Subset bitmasks are always non-zero, so `0` is the empty marker.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SubsetSlotMap {
    keys: Vec<u64>,
    slots: Vec<u32>,
    mask: usize,
    shift: u32,
    len: usize,
}

impl SubsetSlotMap {
    fn with_capacity(n: usize) -> Self {
        let cap = (n.max(8) * 2).next_power_of_two();
        Self {
            keys: vec![0u64; cap],
            slots: vec![0u32; cap],
            mask: cap - 1,
            shift: 64 - cap.trailing_zeros(),
            len: 0,
        }
    }

    #[inline]
    fn index(&self, key: u64) -> usize {
        (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> self.shift) as usize
    }

    #[inline]
    pub(crate) fn get(&self, key: u64) -> Option<u32> {
        let mut i = self.index(key);
        loop {
            let k = self.keys[i];
            if k == 0 {
                return None;
            }
            if k == key {
                return Some(self.slots[i]);
            }
            i = (i + 1) & self.mask;
        }
    }

    #[inline]
    fn get_or_insert(&mut self, key: u64) -> u32 {
        if (self.len + 1) * 2 > self.keys.len() {
            self.grow();
        }
        let mut i = self.index(key);
        loop {
            let k = self.keys[i];
            if k == 0 {
                self.keys[i] = key;
                let s = self.len as u32;
                self.slots[i] = s;
                self.len += 1;
                return s;
            }
            if k == key {
                return self.slots[i];
            }
            i = (i + 1) & self.mask;
        }
    }

    fn grow(&mut self) {
        let new_cap = self.keys.len() * 2;
        let mut new = SubsetSlotMap {
            keys: vec![0u64; new_cap],
            slots: vec![0u32; new_cap],
            mask: new_cap - 1,
            shift: 64 - new_cap.trailing_zeros(),
            len: self.len,
        };
        for (i, &k) in self.keys.iter().enumerate() {
            if k != 0 {
                let mut j = new.index(k);
                while new.keys[j] != 0 {
                    j = (j + 1) & new.mask;
                }
                new.keys[j] = k;
                new.slots[j] = self.slots[i];
            }
        }
        *self = new;
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

/// Omaha range representation - simple array of hands with weights
/// Supports PLO4 (4 cards), PLO5 (5 cards), and PLO6 (6 cards)
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct OmahaRange {
    // Parallel arrays for WASM compatibility
    // Using max size array [u8; 6], with hand_size indicating actual cards used
    hands: Vec<[u8; 6]>,
    weights: Vec<f32>,
    // Precomputed 52-bit card masks (one per hand), so card-removal checks are a
    // single AND instead of an O(n*m) nested loop. Kept in lockstep with `hands`.
    masks: Vec<u64>,
    // Precomputed 2-card hole-pair states. Omaha rank evaluation combines each
    // pair with each 3-card board subset, so cached ranges should not rebuild
    // these same C(k, 2) states on every equity call.
    pair_states: Vec<[Hand; 15]>,
    // Flat non-empty card subsets per hand. Each hand contributes `subset_stride`
    // masks, with odd-sized subsets first and even-sized subsets after them for
    // inclusion-exclusion.
    subset_masks: Vec<u64>,
    subset_slots: Vec<u32>,
    subset_slot_map: SubsetSlotMap,
    plo4_direct_subset_slots: Option<Vec<u32>>,
    subset_stride: usize,
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
        let subset_stride = (1usize << hand_size) - 1;
        Self {
            hands: Vec::new(),
            weights: Vec::new(),
            masks: Vec::new(),
            pair_states: Vec::new(),
            subset_masks: Vec::new(),
            subset_slots: Vec::new(),
            subset_slot_map: SubsetSlotMap::with_capacity(subset_stride),
            plo4_direct_subset_slots: (hand_size == 4)
                .then(|| vec![INVALID_SUBSET_SLOT; PLO4_SUBSET_SLOT_COUNT]),
            subset_stride,
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
        let mut mask = 0u64;
        for (i, &card) in hand.iter().enumerate() {
            hand_array[i] = card;
            mask |= 1u64 << card;
        }

        let mut odd_subsets = Vec::with_capacity(1usize << (self.hand_size - 1));
        let mut even_subsets = Vec::with_capacity(self.subset_stride - odd_subsets.capacity());
        let mut subset = mask;
        while subset != 0 {
            if subset.count_ones() & 1 == 1 {
                odd_subsets.push(subset);
            } else {
                even_subsets.push(subset);
            }
            subset = (subset - 1) & mask;
        }

        let mut pair_states = [Hand::new(); 15];
        let mut pair_count = 0usize;
        for a in 0..self.hand_size {
            for b in (a + 1)..self.hand_size {
                pair_states[pair_count] = Hand::new()
                    .add_card(hand[a] as usize)
                    .add_card(hand[b] as usize);
                pair_count += 1;
            }
        }
        self.hands.push(hand_array);
        self.weights.push(weight);
        self.masks.push(mask);
        self.pair_states.push(pair_states);
        for subset in odd_subsets.into_iter().chain(even_subsets) {
            self.subset_masks.push(subset);
            let slot = self.subset_slot_map.get_or_insert(subset);
            if let Some(direct) = &mut self.plo4_direct_subset_slots {
                direct[plo4_subset_direct_index(subset)] = slot;
            }
            self.subset_slots.push(slot);
        }
        debug_assert_eq!(
            self.subset_masks.len(),
            self.hands.len() * self.subset_stride
        );
        debug_assert_eq!(
            self.subset_slots.len(),
            self.hands.len() * self.subset_stride
        );
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
        self.hands
            .iter()
            .map(move |h| &h[..hand_size])
            .zip(self.weights.iter().copied())
    }

    /// Iterator over (hand slice, weight, precomputed card mask) triples.
    /// The mask lets callers do card-removal checks with a single AND.
    pub fn iter_masked(&self) -> impl Iterator<Item = (&[u8], f32, u64)> + '_ {
        let hand_size = self.hand_size;
        self.hands
            .iter()
            .map(move |h| &h[..hand_size])
            .zip(self.weights.iter().copied())
            .zip(self.masks.iter().copied())
            .map(|((h, w), m)| (h, w, m))
    }

    /// Iterator over hand data plus precomputed legal 2-card hole-pair states.
    pub(crate) fn iter_eval_ready(&self) -> impl Iterator<Item = (&[u8], f32, u64, &[Hand])> + '_ {
        let hand_size = self.hand_size;
        let pair_count = hand_size * (hand_size - 1) / 2;
        self.hands
            .iter()
            .map(move |h| &h[..hand_size])
            .zip(self.weights.iter().copied())
            .zip(self.masks.iter().copied())
            .zip(self.pair_states.iter().map(move |p| &p[..pair_count]))
            .map(|(((h, w), m), p)| (h, w, m, p))
    }

    /// Iterator over hand data plus cached evaluator and blocker subset state.
    pub(crate) fn iter_eval_and_subsets(
        &self,
    ) -> impl Iterator<Item = (&[u8], f32, u64, &[Hand], &[u64], &[u32])> + '_ {
        let hand_size = self.hand_size;
        let pair_count = hand_size * (hand_size - 1) / 2;
        let subset_stride = self.subset_stride;
        self.hands
            .iter()
            .map(move |h| &h[..hand_size])
            .zip(self.weights.iter().copied())
            .zip(self.masks.iter().copied())
            .zip(self.pair_states.iter().map(move |p| &p[..pair_count]))
            .zip(self.subset_masks.chunks_exact(subset_stride))
            .zip(self.subset_slots.chunks_exact(subset_stride))
            .map(|(((((h, w), m), p), sm), ss)| (h, w, m, p, sm, ss))
    }

    #[inline]
    pub(crate) fn subset_slot_count(&self) -> usize {
        self.subset_slot_map.len()
    }

    #[inline]
    pub(crate) fn subset_slot(&self, subset: u64) -> Option<u32> {
        if let Some(direct) = &self.plo4_direct_subset_slots {
            let slot = direct[plo4_subset_direct_index(subset)];
            return (slot != INVALID_SUBSET_SLOT).then_some(slot);
        }
        self.subset_slot_map.get(subset)
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
