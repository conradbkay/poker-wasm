use crate::evaluation::{gen_board_eval, IDX2HAND};
use super::blocker::ComboInfo;

use crate::{Equity, EquityResult, HoldemRange};

pub fn calculate_leaf_equity(
    hand_ranks_data: &[u8],
    hero_range: &HoldemRange,
    vs_range: &HoldemRange,
    board: &[u8],
) -> Vec<EquityResult> {
    assert!(board.len() >= 3 && board.len() <= 5, "board must be 3-5 cards");

    let board_eval = gen_board_eval(hand_ranks_data, board);

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
    let mut rank_ranges = Vec::new();
    let mut cur_p_idx_range: (usize, usize) = (0, 0);
    let mut cur_rank_idx = -1;
    let mut idx_to_range_idx: [usize; 1326] = [0; 1326];

    for (i, combo_info) in all_combos.iter().enumerate() {
        total_weight += combo_info.vs_weight;
        weight_prefix.push(total_weight);

        if combo_info.p != cur_p {
            cur_rank_idx += 1; // on first iter cur_rank_idx becomes 0
            cur_p = combo_info.p;
            cur_p_idx_range = (i, i);
            rank_ranges.push((i, i));
        } else {
            cur_p_idx_range.1 = i;
            rank_ranges[cur_rank_idx as usize].1 = i;
        }

        idx_to_range_idx[combo_info.idx as usize] = cur_rank_idx as usize;
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
        let rank_idx = idx_to_range_idx[combo_info.idx as usize];
        let (idx_range_start, idx_range_end) = rank_ranges[rank_idx];

        let mut beat_weight = if idx_range_start > 0 {
            weight_prefix[idx_range_start - 1]
        } else {
            0.0
        };
        let mut tie_weight =
            weight_prefix[idx_range_end] - beat_weight;
        let mut after_blocker_weight = total_weight;

        for &card in &combo {
            let card_usize = card as usize;
            if n_combos > 0 {
                let blocked_weight = blocked_prefix[(n_combos - 1) * 52 + card_usize];

                let blocked_beat_weight = if idx_range_start > 0 {
                    blocked_prefix[(idx_range_start - 1) * 52 + card_usize]
                } else {
                    0.0
                };
                let blocked_tie_weight = blocked_prefix[idx_range_end * 52 + card_usize] - if idx_range_start > 0 {
                    blocked_prefix[(idx_range_start - 1) * 52 + card_usize]
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

/// Calculate equity with board enumeration (3, 4, or 5-card boards)
pub fn calculate_equity_vs_range(
    hand_ranks_data: &[u8],
    hero_range: &HoldemRange,
    vs_range: &HoldemRange,
    board: &[u8],
) -> Result<Vec<EquityResult>, String> {
    if board.len() < 3 || board.len() > 5 {
        return Err("Board must have 3, 4, or 5 cards".to_string());
    }

    if board.len() == 5 {
        return Ok(calculate_leaf_equity(hand_ranks_data, hero_range, vs_range, board));
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
                let equity_results = calculate_leaf_equity(hand_ranks_data, hero_range, vs_range, &full_board);
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
            let equity_results = calculate_leaf_equity(hand_ranks_data, hero_range, vs_range, &full_board);
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
