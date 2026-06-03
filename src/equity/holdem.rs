use crate::evaluation::IDX2HAND;

use crate::{Equity, EquityResult, HoldemRange};
use holdem_hand_evaluator::Hand;

const MAX_HOLDEM_COMBOS: usize = 1326;
const RANK_BUCKETS: usize = 9 * 4096;
const BUCKET_MIN_COMBOS: usize = 768;

#[derive(Clone, Copy)]
struct HoldemComboInfo {
    rank: u16,
    idx: u16,
    self_weight: f32,
    vs_weight: f32,
    combo: [u8; 2],
}

struct RankGroup {
    rank: u16,
    sum: f32,
    minus: [f32; 52],
    head: u16,
}

#[inline]
fn board_mask(board: &[u8]) -> u64 {
    let mut mask = 0u64;
    for &card in board {
        mask |= 1u64 << card;
    }
    mask
}

#[inline]
fn board_hand(board: &[u8]) -> Hand {
    let mut hand = Hand::new();
    for &card in board {
        hand = hand.add_card(card as usize);
    }
    hand
}

#[inline(always)]
fn evaluate_combo(board_hand: Hand, combo: [u8; 2]) -> u16 {
    board_hand
        .add_card(combo[0] as usize)
        .add_card(combo[1] as usize)
        .evaluate()
}

/// Computes leaf (5-card board) equity for `hero_range` vs `vs_range`.
pub fn calculate_leaf_equity(
    hero_range: &HoldemRange,
    vs_range: &HoldemRange,
    board: &[u8],
) -> Vec<EquityResult> {
    let mut result = Vec::with_capacity(hero_range.range.iter().filter(|&&w| w > 0.0).count());
    visit_leaf_equity(hero_range, vs_range, board, |hand_idx, combo, equity| {
        result.push(EquityResult {
            combo,
            hand_idx,
            equity,
        });
    });
    result
}

fn visit_leaf_equity(
    hero_range: &HoldemRange,
    vs_range: &HoldemRange,
    board: &[u8],
    mut emit: impl FnMut(usize, [u8; 2], Equity),
) {
    assert!((3..=5).contains(&board.len()), "board must be 3-5 cards");

    let board_mask = board_mask(board);
    let board_hand = board_hand(board);

    let mut all_combos: Vec<HoldemComboInfo> = Vec::with_capacity(MAX_HOLDEM_COMBOS);
    for (idx, &combo) in IDX2HAND.iter().enumerate() {
        if (board_mask & (1u64 << combo[0]) != 0) || (board_mask & (1u64 << combo[1]) != 0) {
            continue;
        }

        let vs_weight: f32 = vs_range.range[idx];
        let self_weight = hero_range.range[idx];

        if vs_weight > 0.0 || self_weight > 0.0 {
            all_combos.push(HoldemComboInfo {
                rank: evaluate_combo(board_hand, combo),
                idx: idx as u16,
                self_weight,
                vs_weight,
                combo,
            });
        }
    }

    let mut total_weight = 0.0f32;
    let mut blocked_total = [0.0f32; 52];
    for combo_info in all_combos.iter() {
        total_weight += combo_info.vs_weight;
        blocked_total[combo_info.combo[0] as usize] += combo_info.vs_weight;
        blocked_total[combo_info.combo[1] as usize] += combo_info.vs_weight;
    }

    if all_combos.len() >= BUCKET_MIN_COMBOS {
        visit_leaf_equity_bucketed(&all_combos, total_weight, &blocked_total, &mut emit);
        return;
    }

    all_combos.sort_unstable_by_key(|a| a.rank);
    visit_leaf_equity_sorted(&all_combos, total_weight, &blocked_total, emit);
}

#[inline]
fn emit_combo_equity(
    combo_info: &HoldemComboInfo,
    weaker_sum: f32,
    weaker_minus: &[f32; 52],
    group_sum: f32,
    group_minus: &[f32; 52],
    total_weight: f32,
    blocked_total: &[f32; 52],
    emit: &mut impl FnMut(usize, [u8; 2], Equity),
) {
    if combo_info.self_weight == 0.0 {
        return;
    }

    let c1 = combo_info.combo[0] as usize;
    let c2 = combo_info.combo[1] as usize;
    let own = combo_info.vs_weight;

    let win = weaker_sum - weaker_minus[c1] - weaker_minus[c2];
    let tie = group_sum - group_minus[c1] - group_minus[c2] + own;
    let unblocked_total = total_weight - blocked_total[c1] - blocked_total[c2] + own;
    let lose = unblocked_total - win - tie;

    emit(
        combo_info.idx as usize,
        combo_info.combo,
        Equity { win, tie, lose },
    );
}

fn visit_leaf_equity_sorted(
    all_combos: &[HoldemComboInfo],
    total_weight: f32,
    blocked_total: &[f32; 52],
    mut emit: impl FnMut(usize, [u8; 2], Equity),
) {
    let mut weaker_sum = 0.0f32;
    let mut weaker_minus = [0.0f32; 52];

    let mut i = 0;
    while i < all_combos.len() {
        let group_rank = all_combos[i].rank;
        let mut j = i;
        let mut group_sum = 0.0f32;
        let mut group_minus = [0.0f32; 52];
        while j < all_combos.len() && all_combos[j].rank == group_rank {
            let combo_info = &all_combos[j];
            group_sum += combo_info.vs_weight;
            group_minus[combo_info.combo[0] as usize] += combo_info.vs_weight;
            group_minus[combo_info.combo[1] as usize] += combo_info.vs_weight;
            j += 1;
        }

        for combo_info in &all_combos[i..j] {
            emit_combo_equity(
                combo_info,
                weaker_sum,
                &weaker_minus,
                group_sum,
                &group_minus,
                total_weight,
                blocked_total,
                &mut emit,
            );
        }

        weaker_sum += group_sum;
        for c in 0..52 {
            weaker_minus[c] += group_minus[c];
        }

        i = j;
    }
}

fn visit_leaf_equity_bucketed(
    all_combos: &[HoldemComboInfo],
    total_weight: f32,
    blocked_total: &[f32; 52],
    emit: &mut impl FnMut(usize, [u8; 2], Equity),
) {
    const NONE: u16 = u16::MAX;

    let mut rank_to_group = [NONE; RANK_BUCKETS];
    let mut groups: Vec<RankGroup> = Vec::with_capacity(all_combos.len());
    let mut next = vec![NONE; all_combos.len()];

    for (combo_idx, combo_info) in all_combos.iter().enumerate() {
        let rank_idx = combo_info.rank as usize;
        let group_idx = if rank_to_group[rank_idx] == NONE {
            let group_idx = groups.len() as u16;
            rank_to_group[rank_idx] = group_idx;
            groups.push(RankGroup {
                rank: combo_info.rank,
                sum: 0.0,
                minus: [0.0; 52],
                head: NONE,
            });
            group_idx
        } else {
            rank_to_group[rank_idx]
        };

        let group = &mut groups[group_idx as usize];
        group.sum += combo_info.vs_weight;
        group.minus[combo_info.combo[0] as usize] += combo_info.vs_weight;
        group.minus[combo_info.combo[1] as usize] += combo_info.vs_weight;
        next[combo_idx] = group.head;
        group.head = combo_idx as u16;
    }

    groups.sort_unstable_by_key(|group| group.rank);

    let mut weaker_sum = 0.0f32;
    let mut weaker_minus = [0.0f32; 52];

    for group in &groups {
        let mut combo_idx = group.head;
        while combo_idx != NONE {
            let combo_info = &all_combos[combo_idx as usize];
            emit_combo_equity(
                combo_info,
                weaker_sum,
                &weaker_minus,
                group.sum,
                &group.minus,
                total_weight,
                blocked_total,
                emit,
            );
            combo_idx = next[combo_idx as usize];
        }

        weaker_sum += group.sum;
        for c in 0..52 {
            weaker_minus[c] += group.minus[c];
        }
    }
}

/// Calculate equity with board enumeration (3, 4, or 5-card boards).
pub fn calculate_equity_vs_range(
    hero_range: &HoldemRange,
    vs_range: &HoldemRange,
    board: &[u8],
) -> Result<Vec<EquityResult>, String> {
    if !(3..=5).contains(&board.len()) {
        return Err("Board must have 3, 4, or 5 cards".to_string());
    }

    if board.len() == 5 {
        return Ok(calculate_leaf_equity(hero_range, vs_range, board));
    }

    let mut aggregated_equities = [Equity::default(); 1326];

    let mut accumulate = |full_board: &[u8]| {
        visit_leaf_equity(
            hero_range,
            vs_range,
            full_board,
            |hand_idx, _combo, equity| {
                aggregated_equities[hand_idx].win += equity.win;
                aggregated_equities[hand_idx].tie += equity.tie;
                aggregated_equities[hand_idx].lose += equity.lose;
            },
        );
    };

    let board_mask = board_mask(board);

    if board.len() == 3 {
        for turn in 0..52 {
            if (board_mask & (1u64 << turn)) != 0 {
                continue;
            }
            let turn_mask = board_mask | (1u64 << turn);
            for river in (turn + 1)..52 {
                if (turn_mask & (1u64 << river)) != 0 {
                    continue;
                }
                accumulate(&[board[0], board[1], board[2], turn, river]);
            }
        }
    } else if board.len() == 4 {
        for river in 0..52 {
            if (board_mask & (1u64 << river)) != 0 {
                continue;
            }
            accumulate(&[board[0], board[1], board[2], board[3], river]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::{IDX2HAND, gen_board_eval_holdem_hhe};

    fn blocked(mask: u64, combo: [u8; 2]) -> bool {
        mask & ((1u64 << combo[0]) | (1u64 << combo[1])) != 0
    }

    fn reference_leaf(hero: &HoldemRange, vs: &HoldemRange, board: &[u8; 5]) -> Vec<EquityResult> {
        let eval = gen_board_eval_holdem_hhe(board);
        let board_mask = board_mask(board);

        let ranks: [i32; 1326] = std::array::from_fn(|idx| {
            let combo = IDX2HAND[idx];
            if blocked(board_mask, combo) {
                0
            } else {
                eval(&combo)
            }
        });

        let mut out = Vec::new();
        for hi in 0..1326 {
            let hc = IDX2HAND[hi];
            if hero.range[hi] == 0.0 {
                continue;
            }
            if blocked(board_mask, hc) {
                continue;
            }
            let hr = ranks[hi];
            let (mut win, mut tie, mut lose) = (0.0f32, 0.0f32, 0.0f32);
            for vi in 0..1326 {
                let vc = IDX2HAND[vi];
                let w = vs.range[vi];
                if w == 0.0 {
                    continue;
                }
                if blocked(board_mask, vc) {
                    continue;
                }
                if hc[0] == vc[0] || hc[0] == vc[1] || hc[1] == vc[0] || hc[1] == vc[1] {
                    continue;
                }
                let vr = ranks[vi];
                if hr > vr {
                    win += w;
                } else if hr == vr {
                    tie += w;
                } else {
                    lose += w;
                }
            }
            out.push(EquityResult {
                combo: hc,
                hand_idx: hi,
                equity: Equity { win, tie, lose },
            });
        }
        out
    }

    fn deterministic_range(
        size: usize,
        board: &[u8; 5],
        offset: usize,
        step: usize,
    ) -> HoldemRange {
        let mut range = HoldemRange::new();
        let board_mask = board_mask(board);
        let mut idx = offset % 1326;
        let mut added = 0;

        while added < size {
            let combo = IDX2HAND[idx];
            if !blocked(board_mask, combo) && range.range[idx] == 0.0 {
                let weight = ((idx % 11) as f32 + 1.0) / 12.0;
                range.set(idx, weight);
                added += 1;
            }
            idx = (idx + step) % 1326;
        }

        range
    }

    fn assert_results_close(mut got: Vec<EquityResult>, mut want: Vec<EquityResult>) {
        got.sort_by_key(|r| r.hand_idx);
        want.sort_by_key(|r| r.hand_idx);
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want.iter()) {
            assert_eq!(g.hand_idx, w.hand_idx);
            let close = |a: f32, b: f32| (a - b).abs() < 1e-2;
            assert!(
                close(g.equity.win, w.equity.win)
                    && close(g.equity.tie, w.equity.tie)
                    && close(g.equity.lose, w.equity.lose),
                "equity {:?} vs {:?}",
                g.equity,
                w.equity
            );
        }
    }

    #[test]
    fn leaf_equity_matches_reference() {
        let boards: [[u8; 5]; 4] = [
            [0, 5, 9, 21, 48],
            [4, 8, 12, 16, 20],
            [51, 47, 3, 19, 35],
            [2, 6, 10, 14, 50],
        ];
        for board in boards {
            let mut hero = HoldemRange::new();
            let mut vs = HoldemRange::new();
            for idx in 0..1326 {
                hero.set(idx, ((idx % 7) as f32 + 1.0) / 8.0);
                vs.set(idx, ((idx % 5) as f32 + 1.0) / 6.0);
            }
            assert_results_close(
                calculate_leaf_equity(&hero, &vs, &board),
                reference_leaf(&hero, &vs, &board),
            );
        }
    }

    #[test]
    fn holdem_range_vs_range_fast_correctness_sizes_100_1000() {
        let board = [0, 5, 9, 21, 48];

        for size in [100, 1000] {
            let hero = deterministic_range(size, &board, 17, 37);
            let vs = deterministic_range(size, &board, 91, 41);
            let got = calculate_equity_vs_range(&hero, &vs, &board).unwrap();
            let want = reference_leaf(&hero, &vs, &board);
            assert_results_close(got, want);
        }
    }
}
