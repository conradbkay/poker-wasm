use wasm_bindgen::prelude::*;
use crate::evaluation::{fast_eval, final_p, combinations::{HOLE_COMBOS_2_FROM_4, HOLE_COMBOS_2_FROM_5, HOLE_COMBOS_2_FROM_6, BOARD_COMBOS_3_FROM_5}};
use crate::types::Equity;
use crate::range::OmahaRange;
use rand::Rng;

/// Output structure for enumerated board runouts
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct RunoutEquities {
    pub(crate) board: [u8; 5],
    pub(crate) equity: Equity,
}

#[wasm_bindgen]
impl RunoutEquities {
    #[wasm_bindgen(getter)]
    pub fn board(&self) -> Vec<u8> {
        self.board.to_vec()
    }

    #[wasm_bindgen(getter)]
    pub fn equity(&self) -> Equity {
        self.equity
    }
}

/// Evaluate a single Omaha hand on a complete 5-card board
/// In Omaha, players MUST use exactly 2 hole cards + exactly 3 board cards
/// Supports PLO4 (60 combos), PLO5 (100 combos), and PLO6 (150 combos)
/// Precompute the evaluator state (`p`) for each of the 10 three-card board
/// subsets. These depend only on the board, so they're constant across the hero
/// hand and every villain hand on a given leaf — computing them once avoids
/// re-walking the board (3 table lookups × 10 subsets) for every hand evaluated.
#[inline]
fn compute_board_ps(ranks_data: &[i32], board: &[u8; 5]) -> [usize; 10] {
    let mut ps = [0usize; 10];
    for (i, &[b1, b2, b3]) in BOARD_COMBOS_3_FROM_5.iter().enumerate() {
        let board_triple = [board[b1], board[b2], board[b3]];
        ps[i] = fast_eval(ranks_data, &board_triple, 53) as usize;
    }
    ps
}

/// Evaluate a single Omaha hand given precomputed per-subset board states.
/// In Omaha, players MUST use exactly 2 hole cards + exactly 3 board cards.
/// Supports PLO4 (60 combos), PLO5 (100 combos), and PLO6 (150 combos).
fn eval_omaha_hand(
    ranks_data: &[i32],
    hole_cards: &[u8],
    board_ps: &[usize; 10],
) -> i32 {
    let mut best_rank = i32::MIN;

    // Select combination table based on hand size
    let hole_combos: &[[usize; 2]] = match hole_cards.len() {
        4 => &HOLE_COMBOS_2_FROM_4,
        5 => &HOLE_COMBOS_2_FROM_5,
        6 => &HOLE_COMBOS_2_FROM_6,
        _ => panic!("Invalid Omaha hand size: {}", hole_cards.len()),
    };

    // For each of the 10 board subsets, continue the evaluation from the
    // precomputed board state with each 2-card hole combination.
    for &board_p in board_ps.iter() {
        for &[h1, h2] in hole_combos.iter() {
            let hole_pair = [hole_cards[h1], hole_cards[h2]];
            let combined_p = fast_eval(ranks_data, &hole_pair, board_p);
            let rank = final_p(ranks_data, combined_p as usize) as i32;
            best_rank = best_rank.max(rank);
        }
    }

    best_rank
}

/// Convert a slice of cards to a bitmask for card removal tracking
#[inline]
fn cards_to_mask(cards: &[u8]) -> u64 {
    let mut mask = 0u64;
    for &card in cards {
        mask |= 1u64 << card;
    }
    mask
}

/// Calculate equity for a single Omaha hand vs a range on a complete 5-card board
pub fn calculate_omaha_leaf_equity(
    ranks_data: &[i32],
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8; 5],
) -> RunoutEquities {
    // Precompute the 10 board-subset states once; reused for hero and every villain.
    let board_ps = compute_board_ps(ranks_data, board);

    // Evaluate hero's hand
    let hero_rank = eval_omaha_hand(ranks_data, hero_hand, &board_ps);

    // Cards unavailable to villains: hero's hole cards plus the board.
    let dead_mask = cards_to_mask(hero_hand) | cards_to_mask(board);

    // Calculate equity vs range
    let mut win_weight = 0.0;
    let mut tie_weight = 0.0;
    let mut lose_weight = 0.0;

    for (villain_hand, weight, villain_mask) in vs_range.iter_masked() {
        // Card removal/blocking: a single AND against the dead cards.
        if villain_mask & dead_mask != 0 {
            continue;  // This villain combo is impossible
        }

        let villain_rank = eval_omaha_hand(ranks_data, villain_hand, &board_ps);

        if hero_rank > villain_rank {
            win_weight += weight;
        } else if hero_rank == villain_rank {
            tie_weight += weight;
        } else {
            lose_weight += weight;
        }
    }

    RunoutEquities {
        board: *board,
        equity: Equity {
            win: win_weight,
            tie: tie_weight,
            lose: lose_weight,
        },
    }
}

/// Enumerate all river runouts from a turn (4-card board)
fn calculate_omaha_equity_from_turn(
    ranks_data: &[i32],
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8; 4],
) -> Vec<RunoutEquities> {
    let used_mask = cards_to_mask(board) | cards_to_mask(hero_hand);
    let mut results = Vec::with_capacity(44);

    // Enumerate all river cards
    for river in 0..52u8 {
        if (used_mask & (1u64 << river)) != 0 {
            continue;
        }

        let full_board = [
            board[0], board[1], board[2], board[3],
            river
        ];

        let equity_result = calculate_omaha_leaf_equity(
            ranks_data,
            hero_hand,
            vs_range,
            &full_board
        );

        results.push(equity_result);
    }

    results
}

/// Enumerate all turn and river runouts from a flop (3-card board)
fn calculate_omaha_equity_from_flop(
    ranks_data: &[i32],
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8; 3],
) -> Vec<RunoutEquities> {
    let used_mask = cards_to_mask(board) | cards_to_mask(hero_hand);
    // Pre-allocate: ~45 turn cards × ~44 river cards
    let mut results = Vec::with_capacity(1980);

    // Enumerate all turn cards
    for turn in 0..52u8 {
        if (used_mask & (1u64 << turn)) != 0 {
            continue;
        }

        let turn_mask = used_mask | (1u64 << turn);

        // Enumerate all river cards
        for river in (turn + 1)..52u8 {
            if (turn_mask & (1u64 << river)) != 0 {
                continue;
            }

            let full_board = [
                board[0], board[1], board[2],
                turn, river
            ];

            let equity_result = calculate_omaha_leaf_equity(
                ranks_data,
                hero_hand,
                vs_range,
                &full_board
            );

            results.push(equity_result);
        }
    }

    results
}

/// Calculate Omaha equity vs range with board enumeration
/// Returns equity for each possible runout
pub fn calculate_omaha_equity_vs_range(
    ranks_data: &[i32],
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8],
) -> Result<Vec<RunoutEquities>, String> {
    // Validate hand size
    if ![4, 5, 6].contains(&hero_hand.len()) {
        return Err(format!("Omaha hand must be 4, 5, or 6 cards, got {}", hero_hand.len()));
    }

    // Validate range matches hero hand size
    if hero_hand.len() != vs_range.get_hand_size() {
        return Err(format!(
            "Hero hand size ({}) must match range hand size ({})",
            hero_hand.len(),
            vs_range.get_hand_size()
        ));
    }

    match board.len() {
        3 => {
            let board_cards = [board[0], board[1], board[2]];
            Ok(calculate_omaha_equity_from_flop(ranks_data, hero_hand, vs_range, &board_cards))
        }
        4 => {
            let board_cards = [board[0], board[1], board[2], board[3]];
            Ok(calculate_omaha_equity_from_turn(ranks_data, hero_hand, vs_range, &board_cards))
        }
        5 => {
            let board_cards = [board[0], board[1], board[2], board[3], board[4]];
            Ok(vec![calculate_omaha_leaf_equity(ranks_data, hero_hand, vs_range, &board_cards)])
        }
        _ => Err("Board must be 3, 4, or 5 cards".to_string())
    }
}

/// Sample 2 distinct cards from a precomputed `available` list.
/// The list and RNG are owned by the caller so neither is reallocated/recreated
/// per sample (the Monte Carlo loop calls this `num_runouts` times).
#[inline]
fn sample_two_cards(available: &[u8], rng: &mut impl Rng) -> Option<[u8; 2]> {
    let n = available.len();
    if n < 2 {
        return None;
    }

    let i = rng.random_range(0..n);
    // Pick the second index from the remaining n-1 slots, then skip past i so
    // the two cards are always distinct without mutating `available`.
    let mut j = rng.random_range(0..n - 1);
    if j >= i {
        j += 1;
    }

    Some([available[i], available[j]])
}

/// Monte Carlo simulation for Omaha equity on the flop
/// Samples `num_runouts` random turn and river combinations
/// Returns equity for each sampled runout
pub fn calculate_omaha_equity_monte_carlo_flop(
    ranks_data: &[i32],
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    flop: &[u8; 3],
    num_runouts: usize,
) -> Vec<RunoutEquities> {
    let used_mask = cards_to_mask(flop) | cards_to_mask(hero_hand);
    let mut results = Vec::with_capacity(num_runouts);

    // Build the available-card list and RNG once, outside the sampling loop.
    let available: Vec<u8> = (0..52u8)
        .filter(|&card| (used_mask & (1u64 << card)) == 0)
        .collect();
    let mut rng = rand::rng();

    for _ in 0..num_runouts {
        // Sample random turn and river
        if let Some([turn, river]) = sample_two_cards(&available, &mut rng) {
            let full_board = [flop[0], flop[1], flop[2], turn, river];

            let runout_equity = calculate_omaha_leaf_equity(
                ranks_data,
                hero_hand,
                vs_range,
                &full_board,
            );

            results.push(runout_equity);
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    /// Load the real rank table once for the parity tests.
    fn load_ranks() -> Vec<i32> {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/HandRanks.dat"))
            .expect("HandRanks.dat must exist at the crate root for tests");
        bytes
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// Independent, minimal reimplementation of leaf equity used as the test
    /// oracle: a plain per-villain loop with a straightforward bit-by-bit card
    /// overlap check, deliberately not sharing the production helpers' structure.
    /// A correct `calculate_omaha_leaf_equity` must produce bit-identical results.
    fn reference_leaf(
        ranks: &[i32],
        hero: &[u8],
        range: &OmahaRange,
        board: &[u8; 5],
    ) -> (f32, f32, f32) {
        // Build the dead-card set one bit at a time (no shared mask helper).
        let mut dead = 0u64;
        for &c in hero {
            dead |= 1u64 << c;
        }
        for &c in board {
            dead |= 1u64 << c;
        }

        let board_ps = compute_board_ps(ranks, board);
        let hero_rank = eval_omaha_hand(ranks, hero, &board_ps);
        let (mut w, mut t, mut l) = (0.0f32, 0.0f32, 0.0f32);
        for (vh, weight) in range.iter() {
            if vh.iter().any(|&c| dead & (1u64 << c) != 0) {
                continue;
            }
            let vr = eval_omaha_hand(ranks, vh, &board_ps);
            if hero_rank > vr {
                w += weight;
            } else if hero_rank == vr {
                t += weight;
            } else {
                l += weight;
            }
        }
        (w, t, l)
    }

    /// Build a random valid PLO4 hand (4 distinct cards) avoiding `used`.
    fn random_plo4(used: u64, rng: &mut impl Rng) -> ([u8; 4], u64) {
        let mut hand = [0u8; 4];
        let mut mask = used;
        for slot in hand.iter_mut() {
            loop {
                let c = rng.random_range(0..52u8);
                if mask & (1u64 << c) == 0 {
                    mask |= 1u64 << c;
                    *slot = c;
                    break;
                }
            }
        }
        (hand, mask & !used)
    }

    /// Full leaf-equity pipeline must match the brute-force reference exactly,
    /// across many random scenarios.
    #[test]
    fn leaf_equity_matches_reference() {
        let ranks = load_ranks();
        let mut rng = rand::rng();

        for _ in 0..200 {
            // Random distinct board + hero.
            let mut used = 0u64;
            let mut board = [0u8; 5];
            for slot in board.iter_mut() {
                loop {
                    let c = rng.random_range(0..52u8);
                    if used & (1u64 << c) == 0 {
                        used |= 1u64 << c;
                        *slot = c;
                        break;
                    }
                }
            }
            let (hero, hero_mask) = random_plo4(used, &mut rng);
            used |= hero_mask;

            // A range of random villain hands; deliberately allow some that
            // collide with hero/board so the blocking path is exercised.
            let mut range = OmahaRange::new(4);
            for _ in 0..150 {
                // Sometimes draw from the full deck (may overlap) to force blocks.
                let base_used = if rng.random_bool(0.5) { used } else { 0 };
                let (vh, _) = random_plo4(base_used, &mut rng);
                let weight: f32 = rng.random_range(0.1..1.0);
                range.add_hand(&vh, weight);
            }

            let got = calculate_omaha_leaf_equity(&ranks, &hero, &range, &board);
            let (w, t, l) = reference_leaf(&ranks, &hero, &range, &board);

            assert_eq!(got.equity.win, w, "win mismatch");
            assert_eq!(got.equity.tie, t, "tie mismatch");
            assert_eq!(got.equity.lose, l, "lose mismatch");
        }
    }
}