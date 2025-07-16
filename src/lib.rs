use wasm_bindgen::prelude::*;

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

    #[wasm_bindgen]
    pub fn leaf_equity_vs_range(
        &self,
        hero_range: &HoldemRange,
        vs_range: &HoldemRange,
        board: &[u8],
    ) -> Vec<EquityResult> {
        assert!(board.len() >= 3 && board.len() <= 5, "board must be 3-5 cards");

        let board_eval = gen_board_eval(&self.hand_ranks_data, board);

        let mut board_mask = 0u64;
        for &card in board {
            board_mask |= 1u64 << card;
        }

        let mut all_combos: Vec<ComboInfo> = Vec::with_capacity(1326);
        for (idx, &combo) in IDX2HAND.iter().enumerate() {
            if (board_mask & (1u64 << combo[0]) != 0) || (board_mask & (1u64 << combo[1]) != 0) {
                continue;
            }

            let vs_weight: f32 = vs_range.range[idx];
            let self_weight = hero_range.range[idx];

            if vs_weight > 0.0 || self_weight > 0.0 {
                all_combos.push(ComboInfo {
                    p: board_eval(&combo),
                    idx: idx as u16,
                    self_weight,
                    vs_weight,
                    combo,
                });
            }
        }

        all_combos.sort_unstable_by_key(|a| a.p);

        let n_combos: usize = all_combos.len();
        let mut total_weight = 0.0;
        let mut weight_prefix = Vec::with_capacity(n_combos);

        let mut cur_p = -1;
        let mut cur_p_idx_range = (0, 0);
        let mut idx_to_range = [(0u16, 0u16); 1326];

        for (i, combo_info) in all_combos.iter().enumerate() {
            total_weight += combo_info.vs_weight;
            weight_prefix.push(total_weight);

            if combo_info.p != cur_p {
                cur_p = combo_info.p;
                cur_p_idx_range = (i, i);
            } else {
                cur_p_idx_range.1 = i;
            }

            idx_to_range[combo_info.idx as usize] = (cur_p_idx_range.0 as u16, cur_p_idx_range.1 as u16);
        }

        let mut blocked_prefix = vec![0.0; n_combos * 52];

        if n_combos > 0 {
            // handle i=0 case
            let combo_info = &all_combos[0];
            let c1 = combo_info.combo[0] as usize;
            let c2 = combo_info.combo[1] as usize;
            blocked_prefix[c1] += combo_info.vs_weight;
            blocked_prefix[c2] += combo_info.vs_weight;

            for (i, combo_info) in all_combos.iter().enumerate().skip(1) {
                let prev_slice_start = (i - 1) * 52;
                let curr_slice_start = i * 52;
                
                // copy previous cumulative sums
                blocked_prefix
                    .copy_within(prev_slice_start..prev_slice_start + 52, curr_slice_start);

                // add current combo's weight
                let c1 = combo_info.combo[0] as usize;
                let c2 = combo_info.combo[1] as usize;
                blocked_prefix[curr_slice_start + c1] += combo_info.vs_weight;
                blocked_prefix[curr_slice_start + c2] += combo_info.vs_weight;
            }
        }

        let mut result = Vec::with_capacity(hero_range.range.iter().filter(|&&w| w > 0.0).count());
        for combo_info in all_combos.iter() {
            if combo_info.self_weight == 0.0 {
                continue;
            }
            
            let combo = combo_info.combo;
            let (idx_range_start, idx_range_end) = idx_to_range[combo_info.idx as usize];

            let mut beat_weight = if idx_range_start > 0 {
                weight_prefix[idx_range_start as usize - 1]
            } else {
                0.0
            };
            let mut tie_weight =
                weight_prefix[idx_range_end as usize] - beat_weight;
            let mut after_blocker_weight = total_weight;

            for &card in &combo {
                let card_usize = card as usize;
                if n_combos > 0 {
                    let blocked_weight = blocked_prefix[(n_combos - 1) * 52 + card_usize];

                    let blocked_beat_weight = if idx_range_start > 0 {
                        blocked_prefix[(idx_range_start as usize - 1) * 52 + card_usize]
                    } else {
                        0.0
                    };
                    let blocked_tie_weight = blocked_prefix[idx_range_end as usize * 52 + card_usize] - if idx_range_start > 0 {
                        blocked_prefix[(idx_range_start as usize - 1) * 52 + card_usize]
                    } else {
                        0.0
                    };

                    after_blocker_weight -= blocked_weight;
                    beat_weight -= blocked_beat_weight;
                    tie_weight -= blocked_tie_weight;
                }

            }

            let double_blocked_weight = if n_combos > 0 {
                combo_info.vs_weight
            } else {
                0.0
            };
            after_blocker_weight += double_blocked_weight;
            tie_weight += double_blocked_weight;

            let lose_weight = after_blocker_weight - beat_weight - tie_weight;

            result.push(EquityResult {
                combo,
                hand_idx: combo_info.idx as usize,
                equity: Equity {
                    win: beat_weight,
                    tie: tie_weight,
                    lose: lose_weight,
                },
            });
        }

        result
    }

 
    #[wasm_bindgen]
    pub fn equity_vs_range(
        &self,
        hero_range: &HoldemRange,
        vs_range: &HoldemRange,
        board: &[u8],
    ) -> Result<Vec<EquityResult>, String> {
        if board.len() < 3 || board.len() > 5 {
            return Err("Board must have 3, 4, or 5 cards".to_string());
        }

        if board.len() == 5 {
            return Ok(self.leaf_equity_vs_range(hero_range, vs_range, board));
        }

        let mut aggregated_equities = vec![Equity::default(); 1326];

        if board.len() == 3 {
            let mut board_mask = 0u64;
            for &c in board { board_mask |= 1u64 << c; }

            for turn in 0..52 {
                if (board_mask & (1u64 << turn)) != 0 { continue; }
                let turn_mask = board_mask | (1u64 << turn);
                for river in (turn + 1)..52 {
                    if (turn_mask & (1u64 << river)) != 0 { continue; }
                    
                    let full_board = [board[0], board[1], board[2], turn, river];
                    let equity_results = self.leaf_equity_vs_range(hero_range, vs_range, &full_board);
                    for result in equity_results {
                        aggregated_equities[result.hand_idx].win += result.equity.win;
                        aggregated_equities[result.hand_idx].tie += result.equity.tie;
                        aggregated_equities[result.hand_idx].lose += result.equity.lose;
                    }
                }
            }
        } else if board.len() == 4 {
            let mut board_mask = 0u64;
            for &c in board { board_mask |= 1u64 << c; }

            for river in 0..52 {
                if (board_mask & (1u64 << river)) != 0 { continue; }
                
                let full_board = [board[0], board[1], board[2], board[3], river];
                let equity_results = self.leaf_equity_vs_range(hero_range, vs_range, &full_board);
                for result in equity_results {
                    aggregated_equities[result.hand_idx].win += result.equity.win;
                    aggregated_equities[result.hand_idx].tie += result.equity.tie;
                    aggregated_equities[result.hand_idx].lose += result.equity.lose;
                }
            }
        }


        let mut final_results = Vec::new();
        hero_range.for_each_weighted(|_weight, hand_idx| {
            final_results.push(EquityResult {
                combo: HoldemRange::from_hand_idx(hand_idx),
                equity: aggregated_equities[hand_idx],
                hand_idx,
            });
        });

        Ok(final_results)
    }   
}

#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
/* values are the total weight, not percentage */
pub struct Equity {
    pub win: f32,
    pub tie: f32,
    pub lose: f32,
}

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct EquityResult {
    pub(crate) combo: [u8; 2],
    pub(crate) hand_idx: usize,
    pub(crate) equity: Equity,
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
}

// Card representation constants
const SUITS: &str = "cdhs";
const RANKS: &str = "23456789TJQKA";

// Card string formatting functions
pub fn card_to_string(card: u8) -> String {
    if card >= 52 {
        return "??".to_string();
    }
    
    let rank_idx = (card / 4) as usize;
    let suit_idx = (card % 4) as usize;
    
    let rank_char = RANKS.chars().nth(rank_idx).unwrap_or('?');
    let suit_char = SUITS.chars().nth(suit_idx).unwrap_or('?');
    
    format!("{rank_char}{suit_char}")
}

pub fn string_to_card(card_str: &str) -> Option<u8> {
    if card_str.len() != 2 {
        return None;
    }
    
    let chars: Vec<char> = card_str.chars().collect();
    let rank_char = chars[0];
    let suit_char = chars[1];
    
    let rank_idx = RANKS.find(rank_char)?;
    let suit_idx = SUITS.find(suit_char)?;
    
    Some((rank_idx * 4 + suit_idx) as u8)
}

pub fn hand_to_string(hand: &[u8]) -> String {
    if hand.len() < 2 {
        return "??".to_string();
    }
    format!("{}{}", card_to_string(hand[0]), card_to_string(hand[1]))
}

// Lookup table from hand index to combo
static IDX2HAND: [[u8; 2]; 1326] = {
    let mut hands = [[0u8; 2]; 1326];
    let mut idx = 0;
    let mut i = 0;
    while i < 52 {
        let mut j = 0;
        while j < i {
            hands[idx] = [j, i];
            idx += 1;
            j += 1;
        }
        i += 1;
    }
    hands
};

// --- Helper Functions ---

#[inline]
fn next_p(ranks_data: &[u8], p_plus_card: usize) -> u32 {
    // next_p(x) is equivalent to final_p(x+1)
    final_p(ranks_data, p_plus_card + 1)
}

pub fn final_p(ranks_data: &[u8], p: usize) -> u32 {
    let offset = p * 4;
    if offset + 4 <= ranks_data.len() {
        u32::from_le_bytes([
            ranks_data[offset],
            ranks_data[offset + 1],
            ranks_data[offset + 2],
            ranks_data[offset + 3],
        ])
    } else {
        0 // Fallback for out of bounds
    }
}

/**
 * doesn't return the correct final values for 5/6 cards, use fast_eval_partial for that
 */
pub fn fast_eval(ranks_data: &[u8], cards: &[u8], mut p: usize) -> u32 {
    for &card in cards {
        p = next_p(ranks_data, p + card as usize) as usize;
    }
    p as u32
}


#[inline]
pub fn gen_board_eval<'a>(ranks_data: &'a [u8], board: &'a [u8]) -> impl Fn(&[u8]) -> i32 + 'a {
    let board_p = fast_eval(ranks_data, board, 53) as usize;
    let board_len = board.len();
    
    move |hand: &[u8]| {
        let combined_p = fast_eval(ranks_data, hand, board_p);
        if board_len == 5 {
            combined_p as i32
        } else {
            final_p(ranks_data, combined_p as usize) as i32
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct HoldemRange {
    // Represents a range of Texas Hold'em hands.
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

impl HoldemRange {
    /** only iterates on combos with weight > 0 */
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

#[derive(Clone, Copy)]
struct ComboInfo {
    p: i32,
    idx: u16,
    self_weight: f32,
    vs_weight: f32,
    combo: [u8; 2],
}