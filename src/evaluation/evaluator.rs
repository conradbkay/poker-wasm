// --- Hand Evaluation Functions ---

#[inline]
pub fn next_p(ranks_data: &[u8], p_plus_card: usize) -> u32 {
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
