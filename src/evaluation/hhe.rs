//! Hand evaluation backed by `b-inary/holdem-hand-evaluator` (hhe).
//!
//! hhe is a fast, compact 5–7 card evaluator with precomputed lookup tables baked
//! into the binary — no runtime table to load. Card IDs match this crate's
//! representation (rank-major `2c`..`As`), so no translation is needed, and its
//! `Hand` (`{key, mask}`, `Copy`) composes incrementally, which lets board work
//! be shared across the hands evaluated on a given board.

use holdem_hand_evaluator::Hand;

/// Rank of the best 5-card Hold'em hand from `hole` (2 cards) + `board` (3–5
/// cards). Higher is stronger; the value is only meaningful relative to other
/// ranks from this evaluator (e.g. to decide which of two hands wins on the same
/// board). For ranking many hands on one board, prefer [`gen_board_eval_holdem_hhe`]
/// so the board is only built once.
#[inline]
pub fn holdem_hand_rank(hole: &[u8], board: &[u8]) -> i32 {
    let mut h = Hand::new();
    for &c in board {
        h = h.add_card(c as usize);
    }
    for &c in hole {
        h = h.add_card(c as usize);
    }
    h.evaluate() as i32
}

/// Builds a Hold'em board evaluator: the 5-card board state is built once, then
/// each 2-card combo is folded in and ranked.
#[inline]
pub fn gen_board_eval_holdem_hhe(board: &[u8]) -> impl Fn(&[u8]) -> i32 {
    let mut board_hand = Hand::new();
    for &card in board {
        board_hand = board_hand.add_card(card as usize);
    }

    move |hand: &[u8]| {
        let h = board_hand
            .add_card(hand[0] as usize)
            .add_card(hand[1] as usize);
        h.evaluate() as i32
    }
}

/// A partial board state for Omaha evaluation. Omaha uses exactly 2 hole + 3
/// board cards, so a 3-card board subset is accumulated here and completed with
/// each legal 2-hole-card pair to a 5-card hand for ranking.
///
/// hhe's `evaluate()` is valid for 5–7 card hands, so the 5-card Omaha hand is a
/// single direct table lookup.
#[derive(Clone, Copy)]
pub struct OmahaBoardState(Hand);

impl OmahaBoardState {
    /// State holding exactly `cards` (e.g. a 1-, 2-, or 3-card board subset).
    #[inline]
    pub fn from_cards(cards: &[u8]) -> Self {
        let mut h = Hand::new();
        for &c in cards {
            h = h.add_card(c as usize);
        }
        OmahaBoardState(h)
    }

    /// State extended by one more board card.
    #[inline]
    pub fn add_card(self, card: u8) -> Self {
        OmahaBoardState(self.0.add_card(card as usize))
    }

    /// Best 5-card rank completing this 3-card board state with every legal
    /// 2-hole-card combination.
    #[inline]
    pub fn best_over_holes(&self, hole: &[u8], combos: &[[usize; 2]]) -> i32 {
        let mut best = i32::MIN;
        for &[a, b] in combos {
            let h = self.0.add_card(hole[a] as usize).add_card(hole[b] as usize);
            best = best.max(h.evaluate() as i32);
        }
        best
    }
}
