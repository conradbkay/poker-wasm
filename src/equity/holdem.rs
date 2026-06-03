use crate::evaluation::{gen_board_eval, IDX2HAND};
use super::blocker::ComboInfo;

use crate::{Equity, EquityResult, HoldemRange};

/// Computes leaf (5-card board) equity for `hero_range` vs `vs_range`.
///
/// Walks the combos once in strength order, carrying the blocker-adjusted villain
/// weight in two `[f32; 52]` accumulators (per-card weight of strictly-weaker combos,
/// plus per-card totals). This replaces the old `n_combos * 52` prefix-sum matrix:
/// same win/tie/lose output, but O(52) scratch instead of up to ~275 KB per leaf.
pub fn calculate_leaf_equity(
    hand_ranks_data: &[i32],
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

    let n_combos = all_combos.len();

    // Total villain weight, and per-card villain weight (used for the `lose` complement).
    let mut total_weight = 0.0f32;
    let mut blocked_total = [0.0f32; 52];
    for combo_info in &all_combos {
        total_weight += combo_info.vs_weight;
        blocked_total[combo_info.combo[0] as usize] += combo_info.vs_weight;
        blocked_total[combo_info.combo[1] as usize] += combo_info.vs_weight;
    }

    let mut result = Vec::with_capacity(hero_range.range.iter().filter(|&&w| w > 0.0).count());

    // Per-card villain weight of all combos in strictly-weaker strength groups.
    let mut weaker_sum = 0.0f32;
    let mut weaker_minus = [0.0f32; 52];

    let mut i = 0;
    while i < n_combos {
        // Collect the equal-strength group [i, j) and its per-card weight.
        let group_p = all_combos[i].p;
        let mut j = i;
        let mut group_sum = 0.0f32;
        let mut group_minus = [0.0f32; 52];
        while j < n_combos && all_combos[j].p == group_p {
            let combo_info = &all_combos[j];
            group_sum += combo_info.vs_weight;
            group_minus[combo_info.combo[0] as usize] += combo_info.vs_weight;
            group_minus[combo_info.combo[1] as usize] += combo_info.vs_weight;
            j += 1;
        }

        // Emit hero hands in this group. `weaker_minus` still holds the weight of
        // everything strictly weaker (we roll the group in only after emitting).
        for combo_info in &all_combos[i..j] {
            if combo_info.self_weight == 0.0 {
                continue;
            }
            let c1 = combo_info.combo[0] as usize;
            let c2 = combo_info.combo[1] as usize;
            let own = combo_info.vs_weight;

            // `+ own`: hero's own combo shares both cards, so it is subtracted twice
            // (once per card) and must be added back once.
            let win = weaker_sum - weaker_minus[c1] - weaker_minus[c2];
            let tie = group_sum - group_minus[c1] - group_minus[c2] + own;
            let unblocked_total = total_weight - blocked_total[c1] - blocked_total[c2] + own;
            let lose = unblocked_total - win - tie;

            result.push(EquityResult {
                combo: combo_info.combo,
                hand_idx: combo_info.idx as usize,
                equity: Equity { win, tie, lose },
            });
        }

        // Roll this group into the strictly-weaker accumulator for later groups.
        weaker_sum += group_sum;
        for c in 0..52 {
            weaker_minus[c] += group_minus[c];
        }

        i = j;
    }

    result
}

/// Calculate equity with board enumeration (3, 4, or 5-card boards)
pub fn calculate_equity_vs_range(
    hand_ranks_data: &[i32],
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
