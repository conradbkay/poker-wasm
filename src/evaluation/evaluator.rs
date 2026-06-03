// --- Hand Evaluation Functions ---

#[inline(always)]
pub fn next_p(ranks_data: &[i32], p_plus_card: usize) -> u32 {
    // next_p(x) is equivalent to final_p(x+1)
    final_p(ranks_data, p_plus_card + 1)
}

#[inline(always)]
pub fn final_p(ranks_data: &[i32], p: usize) -> u32 {
    // The rank table is logically u32s (preconverted once at construction),
    // so this is a single indexed load. `.get` preserves the original
    // out-of-bounds-returns-0 semantics.
    ranks_data.get(p).copied().unwrap_or(0) as u32
}

/// Walks `cards` from table position `p`, returning the intermediate position.
/// This is *not* a final hand rank for 5/6-card hands — call [`final_p`] on the
/// result to resolve those (as `gen_board_eval` and the Omaha evaluator do).
#[inline]
pub fn fast_eval(ranks_data: &[i32], cards: &[u8], mut p: usize) -> u32 {
    for &card in cards {
        p = next_p(ranks_data, p + card as usize) as usize;
    }
    p as u32
}

#[inline]
pub fn gen_board_eval<'a>(ranks_data: &'a [i32], board: &'a [u8]) -> impl Fn(&[u8]) -> i32 + 'a {
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
