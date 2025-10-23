use wasm_bindgen::prelude::*;
use crate::evaluation::{gen_board_eval, combinations::{HOLE_COMBOS_2_FROM_4, BOARD_COMBOS_3_FROM_5}};
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
/// This means evaluating all C(4,2) × C(5,3) = 6 × 10 = 60 possible 5-card hands
fn eval_omaha_hand(
    ranks_data: &[u8],
    hole_cards: &[u8; 4],
    board: &[u8; 5]
) -> i32 {
    let mut best_rank = i32::MIN;

    // Evaluate all 10 possible 3-card board combinations
    for &[b1, b2, b3] in BOARD_COMBOS_3_FROM_5.iter() {
        let board_triple = [board[b1], board[b2], board[b3]];

        // Create evaluator for this board combination
        let hand_eval = gen_board_eval(ranks_data, &board_triple);

        // Evaluate all 6 possible 2-card hole combinations
        for &[h1, h2] in HOLE_COMBOS_2_FROM_4.iter() {
            let hole_pair = [hole_cards[h1], hole_cards[h2]];
            let rank = hand_eval(&hole_pair);
            best_rank = best_rank.max(rank);
        }
    }

    best_rank
}

/// Check if two 4-card hands share any cards
#[inline]
fn hands_overlap(hand1: &[u8; 4], hand2: &[u8; 4]) -> bool {
    for &c1 in hand1 {
        for &c2 in hand2 {
            if c1 == c2 {
                return true;
            }
        }
    }
    false
}

/// Check if a 4-card hand overlaps with a 5-card board
#[inline]
fn hand_overlaps_board(hand: &[u8; 4], board: &[u8; 5]) -> bool {
    for &c1 in hand {
        for &c2 in board {
            if c1 == c2 {
                return true;
            }
        }
    }
    false
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
    ranks_data: &[u8],
    hero_hand: &[u8; 4],
    vs_range: &OmahaRange,
    board: &[u8; 5],
) -> RunoutEquities {
    // Evaluate hero's hand
    let hero_rank = eval_omaha_hand(ranks_data, hero_hand, board);

    // Calculate equity vs range
    let mut win_weight = 0.0;
    let mut tie_weight = 0.0;
    let mut lose_weight = 0.0;

    for (villain_hand, weight) in vs_range.iter() {
        // Check for card removal/blocking
        if hands_overlap(hero_hand, villain_hand) ||
           hand_overlaps_board(villain_hand, board) {
            continue;  // This villain combo is impossible
        }

        let villain_rank = eval_omaha_hand(ranks_data, villain_hand, board);

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
    ranks_data: &[u8],
    hero_hand: &[u8; 4],
    vs_range: &OmahaRange,
    board: &[u8; 4],
) -> Vec<RunoutEquities> {
    let mut results = Vec::new();
    let used_mask = cards_to_mask(board) | cards_to_mask(hero_hand);

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
    ranks_data: &[u8],
    hero_hand: &[u8; 4],
    vs_range: &OmahaRange,
    board: &[u8; 3],
) -> Vec<RunoutEquities> {
    let mut results = Vec::new();
    let used_mask = cards_to_mask(board) | cards_to_mask(hero_hand);

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
    ranks_data: &[u8],
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8],
) -> Result<Vec<RunoutEquities>, String> {
    if hero_hand.len() != 4 {
        return Err("Hero hand must be exactly 4 cards".to_string());
    }

    let hero_cards = [hero_hand[0], hero_hand[1], hero_hand[2], hero_hand[3]];

    match board.len() {
        3 => {
            let board_cards = [board[0], board[1], board[2]];
            Ok(calculate_omaha_equity_from_flop(ranks_data, &hero_cards, vs_range, &board_cards))
        }
        4 => {
            let board_cards = [board[0], board[1], board[2], board[3]];
            Ok(calculate_omaha_equity_from_turn(ranks_data, &hero_cards, vs_range, &board_cards))
        }
        5 => {
            let board_cards = [board[0], board[1], board[2], board[3], board[4]];
            Ok(vec![calculate_omaha_leaf_equity(ranks_data, &hero_cards, vs_range, &board_cards)])
        }
        _ => Err("Board must be 3, 4, or 5 cards".to_string())
    }
}

/// Sample 2 random cards from available deck (avoiding used cards)
/// Returns None if unable to sample (shouldn't happen with valid inputs)
fn sample_two_cards<R: Rng>(rng: &mut R, used_mask: u64) -> Option<[u8; 2]> {
    // Build list of available cards
    let mut available: Vec<u8> = (0..52u8)
        .filter(|&card| (used_mask & (1u64 << card)) == 0)
        .collect();

    if available.len() < 2 {
        return None;
    }

    // Sample first card
    let idx1 = rng.random_range(0..available.len());
    let card1 = available.swap_remove(idx1);

    // Sample second card
    let idx2 = rng.random_range(0..available.len());
    let card2 = available.swap_remove(idx2);

    Some([card1, card2])
}

/// Monte Carlo simulation for Omaha equity on the flop
/// Samples `num_runouts` random turn and river combinations
/// Returns equity for each sampled runout
pub fn calculate_omaha_equity_monte_carlo_flop(
    ranks_data: &[u8],
    hero_hand: &[u8; 4],
    vs_range: &OmahaRange,
    flop: &[u8; 3],
    num_runouts: usize,
) -> Vec<RunoutEquities> {
    let mut rng = rand::rng();
    let used_mask = cards_to_mask(flop) | cards_to_mask(hero_hand);
    let mut results = Vec::with_capacity(num_runouts);

    for _ in 0..num_runouts {
        // Sample random turn and river
        if let Some([turn, river]) = sample_two_cards(&mut rng, used_mask) {
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