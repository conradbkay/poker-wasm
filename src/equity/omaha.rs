use crate::evaluation::{
    OmahaBoardState,
    combinations::{
        BOARD_COMBOS_3_FROM_5, HOLE_COMBOS_2_FROM_4, HOLE_COMBOS_2_FROM_5, HOLE_COMBOS_2_FROM_6,
    },
};
use crate::range::OmahaRange;
use crate::types::Equity;
use holdem_hand_evaluator::Hand;
use rand::{Rng, SeedableRng, rngs::StdRng};
use wasm_bindgen::prelude::*;

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

/// Equity of one hero hand (4–6 cards), aggregated over all enumerated runouts.
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct OmahaEquityResult {
    pub(crate) hand: Vec<u8>,
    pub(crate) equity: Equity,
}

#[wasm_bindgen]
impl OmahaEquityResult {
    #[wasm_bindgen(getter)]
    pub fn hand(&self) -> Vec<u8> {
        self.hand.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn equity(&self) -> Equity {
        self.equity
    }
}

/// Precompute the board state for each of the 10 three-card board subsets. These
/// depend only on the board, so they're constant across the hero hand and every
/// villain hand on a given leaf — computing them once avoids rebuilding the board
/// portion for every hand evaluated.
#[inline]
fn compute_board_ps(board: &[u8; 5]) -> [OmahaBoardState; 10] {
    std::array::from_fn(|i| {
        let [b1, b2, b3] = BOARD_COMBOS_3_FROM_5[i];
        OmahaBoardState::from_cards(&[board[b1], board[b2], board[b3]])
    })
}

/// Evaluate a single Omaha hand given precomputed per-subset board states.
/// In Omaha, players MUST use exactly 2 hole cards + exactly 3 board cards.
/// Supports PLO4 (60 combos), PLO5 (100 combos), and PLO6 (150 combos).
fn eval_omaha_hand(hole_cards: &[u8], board_ps: &[OmahaBoardState; 10]) -> i32 {
    let (pair_states, pair_count) = compute_hole_pair_states(hole_cards);
    eval_omaha_pair_states(&pair_states[..pair_count], board_ps)
}

fn compute_hole_pair_states(hole_cards: &[u8]) -> ([Hand; 15], usize) {
    let combos = hole_combos(hole_cards.len());
    let mut pair_states = [Hand::new(); 15];
    for (i, &[a, b]) in combos.iter().enumerate() {
        pair_states[i] = Hand::new()
            .add_card(hole_cards[a] as usize)
            .add_card(hole_cards[b] as usize);
    }
    (pair_states, combos.len())
}

fn eval_omaha_pair_states(pair_states: &[Hand], board_ps: &[OmahaBoardState; 10]) -> i32 {
    let mut best_rank = i32::MIN;
    for board_p in board_ps {
        best_rank = best_rank.max(board_p.best_over_hole_pair_states(pair_states));
    }
    best_rank
}

/// The C(k,2) two-card hole combinations for a `k`-card Omaha hand.
#[inline]
fn hole_combos(hand_size: usize) -> &'static [[usize; 2]] {
    match hand_size {
        4 => &HOLE_COMBOS_2_FROM_4,
        5 => &HOLE_COMBOS_2_FROM_5,
        6 => &HOLE_COMBOS_2_FROM_6,
        _ => panic!("Invalid Omaha hand size: {hand_size}"),
    }
}

/// Flop-shared Omaha evaluator.
///
/// On a fixed flop, the 10 three-card board subsets split by how they use the
/// (varying) turn/river:
///   * type-0 `{f0,f1,f2}`        — independent of turn/river  → one scalar per actor (`a`)
///   * type-1 `{2 flop, X}`       — depends on a single card X  → table `d[card]` per actor
///   * type-2 `{1 flop, t, r}`    — depends on both             → evaluated per runout
///
/// So each actor's best rank on a runout `(t, r)` is
/// `max(a, d[t], d[r], best over the 3 type-2 subsets)`, and only the last term
/// is recomputed per runout. The type-2 board states are board-only, so they're
/// shared across the hero and the whole villain range.
struct OmahaFlopEval {
    /// Board state after each single flop card (for the type-2 subsets).
    oneflop_p: [OmahaBoardState; 3],
    hero_pair_states: [Hand; 15],
    hero_pair_count: usize,
    hero_a: i32,
    hero_d: [i32; 52],
    /// Per-villain type-0 scalar and type-1 table (`m` rows of 52).
    villain_a: Vec<i32>,
    villain_d: Vec<i32>,
}

impl OmahaFlopEval {
    fn build(hero_hand: &[u8], vs_range: &OmahaRange, flop: &[u8; 3]) -> Self {
        let (hero_pair_states, hero_pair_count) = compute_hole_pair_states(hero_hand);
        let hero_pairs = &hero_pair_states[..hero_pair_count];

        // Board-only partial states (shared by hero and every villain).
        let flop_p = OmahaBoardState::from_cards(flop); // type-0: all three flop cards
        let twoflop_pairs = [[flop[0], flop[1]], [flop[0], flop[2]], [flop[1], flop[2]]];
        let twoflop_p: [OmahaBoardState; 3] =
            std::array::from_fn(|i| OmahaBoardState::from_cards(&twoflop_pairs[i]));
        let oneflop_p: [OmahaBoardState; 3] =
            std::array::from_fn(|i| OmahaBoardState::from_cards(&[flop[i]]));

        let flop_mask = cards_to_mask(flop);

        // type-1 board states bs1[pair][X] = state after {2 flop cards, X}, for
        // every card X not on the flop. Board-only, so shared across all actors.
        let empty = OmahaBoardState::from_cards(&[]);
        let mut bs1 = [[empty; 52]; 3];
        for (p, &base) in twoflop_p.iter().enumerate() {
            for x in 0..52usize {
                if flop_mask & (1u64 << x) != 0 {
                    continue;
                }
                bs1[p][x] = base.add_card(x as u8);
            }
        }

        // Fill an actor's type-0 scalar and type-1 table. `x` is a card index used
        // for both the bitmask skip and the `d`/`bs1` indexing, so a range loop is
        // the natural form here.
        #[allow(clippy::needless_range_loop)]
        let fill = |pairs: &[Hand], actor_mask: u64, a: &mut i32, d: &mut [i32]| {
            *a = flop_p.best_over_hole_pair_states(pairs);
            for x in 0..52usize {
                if (flop_mask | actor_mask) & (1u64 << x) != 0 {
                    continue;
                }
                let mut best = i32::MIN;
                for p in 0..3 {
                    best = best.max(bs1[p][x].best_over_hole_pair_states(pairs));
                }
                d[x] = best;
            }
        };

        let mut hero_a = i32::MIN;
        let mut hero_d = [i32::MIN; 52];
        fill(
            hero_pairs,
            cards_to_mask(hero_hand),
            &mut hero_a,
            &mut hero_d,
        );

        let m = vs_range.len();
        let mut villain_a = vec![i32::MIN; m];
        let mut villain_d = vec![i32::MIN; m * 52];
        for (v, (_hole, _w, mask, pairs)) in vs_range.iter_eval_ready().enumerate() {
            if mask & flop_mask != 0 {
                continue;
            }
            let mut a = i32::MIN;
            fill(pairs, mask, &mut a, &mut villain_d[v * 52..v * 52 + 52]);
            villain_a[v] = a;
        }

        OmahaFlopEval {
            oneflop_p,
            hero_pair_states,
            hero_pair_count,
            hero_a,
            hero_d,
            villain_a,
            villain_d,
        }
    }

    /// type-2 contribution: best over the three one-flop subsets completed with
    /// turn+river (the only part not precomputed). Combined with the cached
    /// `a`/`d[t]`/`d[r]` by the caller.
    #[inline]
    fn type2_best(&self, pairs: &[Hand], board3: &[OmahaBoardState; 3]) -> i32 {
        let mut e = i32::MIN;
        for bp in board3 {
            e = e.max(bp.best_over_hole_pair_states(pairs));
        }
        e
    }

    /// Equity for a single runout, reusing all precomputed sharing.
    fn equity_for_runout(
        &self,
        hero_hand: &[u8],
        vs_range: &OmahaRange,
        flop: &[u8; 3],
        turn: u8,
        river: u8,
    ) -> RunoutEquities {
        let t = turn as usize;
        let r = river as usize;

        // type-2 board states {one flop card, turn, river}: board-only, shared.
        let board3: [OmahaBoardState; 3] =
            std::array::from_fn(|i| self.oneflop_p[i].add_card(turn).add_card(river));

        let hero_pairs = &self.hero_pair_states[..self.hero_pair_count];
        let hero_e = self.type2_best(hero_pairs, &board3);
        let hero_rank = self
            .hero_a
            .max(self.hero_d[t])
            .max(self.hero_d[r])
            .max(hero_e);

        let dead_mask = cards_to_mask(hero_hand) | cards_to_mask(flop) | (1u64 << t) | (1u64 << r);

        let (mut win, mut tie, mut lose) = (0.0f32, 0.0f32, 0.0f32);
        for (v, (_hole, weight, mask, pairs)) in vs_range.iter_eval_ready().enumerate() {
            if mask & dead_mask != 0 {
                continue;
            }
            let d = &self.villain_d[v * 52..v * 52 + 52];
            let e = self.type2_best(pairs, &board3);
            let villain_rank = self.villain_a[v].max(d[t]).max(d[r]).max(e);

            if hero_rank > villain_rank {
                win += weight;
            } else if hero_rank == villain_rank {
                tie += weight;
            } else {
                lose += weight;
            }
        }

        RunoutEquities {
            board: [flop[0], flop[1], flop[2], turn, river],
            equity: Equity { win, tie, lose },
        }
    }
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

/// Rank of the best Omaha hand on a complete 5-card `board`, using exactly 2 of
/// the `hole` cards and 3 of the board (PLO4/5/6: `hole` is 4, 5, or 6 cards).
/// Higher is stronger; the value is only meaningful relative to other ranks from
/// this evaluator (e.g. to decide which of two hands wins on the same board).
pub fn omaha_hand_rank(hole: &[u8], board: &[u8; 5]) -> i32 {
    let board_ps = compute_board_ps(board);
    eval_omaha_hand(hole, &board_ps)
}

/// Calculate equity for a single Omaha hand vs a range on a complete 5-card board
pub fn calculate_omaha_leaf_equity(
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8; 5],
) -> RunoutEquities {
    let hero_mask = cards_to_mask(hero_hand);
    let board_mask = cards_to_mask(board);
    if hero_mask & board_mask != 0 {
        return RunoutEquities {
            board: *board,
            equity: Equity::default(),
        };
    }

    // Precompute the 10 board-subset states once; reused for hero and every villain.
    let board_ps = compute_board_ps(board);

    // Evaluate hero's hand
    let hero_rank = eval_omaha_hand(hero_hand, &board_ps);

    // Cards unavailable to villains: hero's hole cards plus the board.
    let dead_mask = hero_mask | board_mask;

    let mut win_weight = 0.0;
    let mut tie_weight = 0.0;
    let mut lose_weight = 0.0;

    for (_villain_hand, weight, villain_mask, pair_states) in vs_range.iter_eval_ready() {
        // Card removal/blocking: a single AND against the dead cards.
        if villain_mask & dead_mask != 0 {
            continue; // This villain combo is impossible
        }

        let villain_rank = eval_omaha_pair_states(pair_states, &board_ps);

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

/// Equity of every hand in a hero **range** vs a villain range on a fixed 5-card
/// board (results aligned to `hero_range` order).
///
/// The naive approach evaluates the whole villain range once per hero — O(H·m)
/// evaluations. Instead this evaluates each hand once, sorts villains by rank, and
/// sweeps hero hands in rank order while carrying running per-card-subset weight
/// sums. Multi-card blocker removal (a villain is dead if it shares *any* card with
/// the hero) is done by inclusion–exclusion over the hero's card subsets:
///
/// ```text
/// blocked(hero) = Σ_{∅≠S⊆C_hero} (-1)^(|S|+1) · W[S],   W[S] = villain weight whose cards ⊇ S
/// ```
///
/// generalizing Hold'em's per-card `weight_minus`. Cost is `(H+m)` evaluations +
/// `sort(m)` + `(H+m)·2^k` subset ops, versus the naive `H·m` evaluations.
///
/// Every live (board-disjoint) hand has exactly `k` cards, hence exactly `2^k-1`
/// non-empty subsets, so the per-hand subset lists are stored flat at a fixed
/// `stride` — no per-hand `Vec`. Hero subsets that no villain has are mapped to a
/// trailing always-zero `sentinel` slot so the inner IE loop stays branch-free.
pub fn calculate_omaha_leaf_equity_range(
    hero_range: &OmahaRange,
    vs_range: &OmahaRange,
    board: &[u8; 5],
) -> Vec<Equity> {
    let board_mask = cards_to_mask(board);
    let board_ps = compute_board_ps(board);

    let k = vs_range.hand_size();
    debug_assert_eq!(
        k,
        hero_range.hand_size(),
        "hero/villain hand sizes must match"
    );
    let stride = (1usize << k) - 1; // non-empty subsets of a board-disjoint k-card hand

    // --- Villains: evaluate once; give every distinct subset a dense slot. ---
    let m = vs_range.len();

    struct Villain {
        rank: i32,
        weight: f32,
        off: u32, // start of this villain's `stride` slots in `villain_slots`
    }
    let mut villains: Vec<Villain> = Vec::with_capacity(m);
    let mut villain_slots: Vec<u32> = Vec::with_capacity(m * stride);
    let mut total_all = 0.0f32;

    for (_hand, weight, mask, pair_states, _subset_masks, subset_slots) in
        vs_range.iter_eval_and_subsets()
    {
        if mask & board_mask != 0 {
            continue; // villain shares a board card -> impossible on this board
        }
        let rank = eval_omaha_pair_states(pair_states, &board_ps);
        let off = villain_slots.len() as u32;
        villain_slots.extend_from_slice(subset_slots);
        total_all += weight;
        villains.push(Villain { rank, weight, off });
    }

    let n_slots = vs_range.subset_slot_count();
    let arr_len = n_slots + 1; // index 0 is the always-zero hero-only subset sentinel

    // A_all[slot + 1] = total villain weight whose cards ⊇ subset(slot).
    let mut a_all = vec![0.0f32; arr_len];
    for v in &villains {
        for &sl in &villain_slots[v.off as usize..v.off as usize + stride] {
            a_all[sl as usize + 1] += v.weight;
        }
    }

    // Per-hero rank and compact subset slots for the IE sum, stored flat at the
    // same `stride`. Odd-card subsets are stored first and added; even-card
    // subsets follow and are subtracted. Slot 0 is an always-zero sentinel for
    // hero-only subsets no villain contains.
    let hero_count = hero_range.len();
    let mut hero_rank = vec![0i32; hero_count];
    let mut hero_blocked = vec![false; hero_count];
    let odd_terms = 1usize << (k - 1);
    let mut hero_slots: Vec<u32> = Vec::with_capacity(hero_count * stride);
    for (hi, (_hand, _w, mask, pair_states, subset_masks, _own_subset_slots)) in
        hero_range.iter_eval_and_subsets().enumerate()
    {
        let base = hero_slots.len();
        hero_slots.resize(base + stride, 0);
        if mask & board_mask != 0 {
            hero_blocked[hi] = true;
            continue;
        }
        hero_rank[hi] = eval_omaha_pair_states(pair_states, &board_ps);
        for (i, &subset) in subset_masks.iter().enumerate() {
            hero_slots[base + i] = vs_range.subset_slot(subset).map(|sl| sl + 1).unwrap_or(0);
        }
    }

    villains.sort_unstable_by_key(|v| v.rank);
    let mut hero_order: Vec<usize> = (0..hero_count).collect();
    hero_order.sort_unstable_by_key(|&i| hero_rank[i]);

    let mut equities = vec![Equity::default(); hero_count];

    let mut w_run = vec![0.0f32; arr_len]; // sums over villains strictly weaker than current rank
    let mut weaker_total = 0.0f32;
    let mut w_group = vec![0.0f32; arr_len]; // sums over villains of exactly the current rank
    let mut group_touched: Vec<u32> = Vec::new();

    let m = villains.len();
    let mut vi = 0usize; // fold pointer into the sorted villains
    let mut hoi = 0usize;
    while hoi < hero_count {
        let r = hero_rank[hero_order[hoi]];

        // Fold all strictly-weaker villains into the running sums.
        while vi < m && villains[vi].rank < r {
            let v = &villains[vi];
            for &sl in &villain_slots[v.off as usize..v.off as usize + stride] {
                w_run[sl as usize + 1] += v.weight;
            }
            weaker_total += v.weight;
            vi += 1;
        }

        // Build the equal-rank group sums (scan, don't advance the fold pointer).
        group_touched.clear();
        let mut group_total = 0.0f32;
        let mut gj = vi;
        while gj < m && villains[gj].rank == r {
            let v = &villains[gj];
            for &sl in &villain_slots[v.off as usize..v.off as usize + stride] {
                let idx = sl + 1;
                if w_group[idx as usize] == 0.0 {
                    group_touched.push(idx);
                }
                w_group[idx as usize] += v.weight;
            }
            group_total += v.weight;
            gj += 1;
        }

        // Emit every hero of this rank using the same running/group sums.
        while hoi < hero_count && hero_rank[hero_order[hoi]] == r {
            let hi = hero_order[hoi];
            if hero_blocked[hi] {
                hoi += 1;
                continue; // impossible on this board; leaves zero equity
            }
            let (mut blk_lt, mut blk_eq, mut blk_all) = (0.0f32, 0.0f32, 0.0f32);
            let terms = &hero_slots[hi * stride..hi * stride + stride];
            for &sl in &terms[..odd_terms] {
                let s = sl as usize;
                blk_lt += w_run[s];
                blk_eq += w_group[s];
                blk_all += a_all[s];
            }
            for &sl in &terms[odd_terms..] {
                let s = sl as usize;
                blk_lt -= w_run[s];
                blk_eq -= w_group[s];
                blk_all -= a_all[s];
            }
            let win = weaker_total - blk_lt;
            let tie = group_total - blk_eq;
            let lose = (total_all - blk_all) - win - tie;
            equities[hi] = Equity { win, tie, lose };
            hoi += 1;
        }

        for &sl in &group_touched {
            w_group[sl as usize] = 0.0;
        }
    }

    equities
}

/// Enumerate all river runouts from a turn (4-card board)
fn calculate_omaha_equity_from_turn(
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8; 4],
) -> Vec<RunoutEquities> {
    let hero_mask = cards_to_mask(hero_hand);
    let board_mask = cards_to_mask(board);
    if hero_mask & board_mask != 0 {
        return Vec::new();
    }
    let used_mask = board_mask | hero_mask;
    let mut results = Vec::with_capacity(44);

    for river in 0..52u8 {
        if (used_mask & (1u64 << river)) != 0 {
            continue;
        }
        let full_board = [board[0], board[1], board[2], board[3], river];
        results.push(calculate_omaha_leaf_equity(
            hero_hand,
            vs_range,
            &full_board,
        ));
    }

    results
}

/// Enumerate all turn and river runouts from a flop (3-card board)
fn calculate_omaha_equity_from_flop(
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8; 3],
) -> Vec<RunoutEquities> {
    let hero_mask = cards_to_mask(hero_hand);
    let board_mask = cards_to_mask(board);
    if hero_mask & board_mask != 0 {
        return Vec::new();
    }
    let used_mask = board_mask | hero_mask;
    // Pre-allocate: ~45 turn cards × ~44 river cards
    let mut results = Vec::with_capacity(1980);

    // Build the flop-shared evaluator once; every runout reuses its precomputed
    // board-subset tables and only re-evaluates the subsets that use both the turn
    // and the river.
    let eval = OmahaFlopEval::build(hero_hand, vs_range, board);

    for turn in 0..52u8 {
        if (used_mask & (1u64 << turn)) != 0 {
            continue;
        }

        let turn_mask = used_mask | (1u64 << turn);

        for river in (turn + 1)..52u8 {
            if (turn_mask & (1u64 << river)) != 0 {
                continue;
            }

            results.push(eval.equity_for_runout(hero_hand, vs_range, board, turn, river));
        }
    }

    results
}

/// Calculate Omaha equity vs range with board enumeration or flop sampling.
/// Returns one equity result per complete board runout.
pub fn calculate_omaha_runout_equity_vs_range(
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8],
    max_runouts: Option<usize>,
    seed: Option<u64>,
) -> Result<Vec<RunoutEquities>, String> {
    // Validate hand size
    if ![4, 5, 6].contains(&hero_hand.len()) {
        return Err(format!(
            "Omaha hand must be 4, 5, or 6 cards, got {}",
            hero_hand.len()
        ));
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
            Ok(match (max_runouts, seed) {
                (Some(num_runouts), Some(seed)) => {
                    let mut rng = StdRng::seed_from_u64(seed);
                    calculate_omaha_equity_monte_carlo_flop_with_rng(
                        hero_hand,
                        vs_range,
                        &board_cards,
                        num_runouts,
                        &mut rng,
                    )
                }
                (Some(num_runouts), None) => calculate_omaha_equity_monte_carlo_flop(
                    hero_hand,
                    vs_range,
                    &board_cards,
                    num_runouts,
                ),
                (None, _) => calculate_omaha_equity_from_flop(hero_hand, vs_range, &board_cards),
            })
        }
        4 => {
            let board_cards = [board[0], board[1], board[2], board[3]];
            Ok(calculate_omaha_equity_from_turn(
                hero_hand,
                vs_range,
                &board_cards,
            ))
        }
        5 => {
            let board_cards = [board[0], board[1], board[2], board[3], board[4]];
            Ok(vec![calculate_omaha_leaf_equity(
                hero_hand,
                vs_range,
                &board_cards,
            )])
        }
        _ => Err("Board must be 3, 4, or 5 cards".to_string()),
    }
}

/// Calculate Omaha equity vs range with complete board enumeration.
/// Returns one equity result per complete board runout.
pub fn calculate_omaha_equity_vs_range(
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    board: &[u8],
) -> Result<Vec<RunoutEquities>, String> {
    calculate_omaha_runout_equity_vs_range(hero_hand, vs_range, board, None, None)
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
fn calculate_omaha_equity_monte_carlo_flop_with_rng(
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    flop: &[u8; 3],
    num_runouts: usize,
    rng: &mut impl Rng,
) -> Vec<RunoutEquities> {
    let hero_mask = cards_to_mask(hero_hand);
    let flop_mask = cards_to_mask(flop);
    if hero_mask & flop_mask != 0 {
        return Vec::new();
    }
    let used_mask = flop_mask | hero_mask;
    let mut results = Vec::with_capacity(num_runouts);

    // Build the available-card list and RNG once, outside the sampling loop.
    let available: Vec<u8> = (0..52u8)
        .filter(|&card| (used_mask & (1u64 << card)) == 0)
        .collect();

    // The flop-shared evaluator precomputes the type-1 `D[X]` table over every
    // live card, which only pays off once enough runouts are sampled to amortize
    // it. Below that crossover (few samples, large range) the per-leaf path is
    // cheaper, so gate on sampling at least as many runouts as live cards.
    if num_runouts >= available.len() {
        let eval = OmahaFlopEval::build(hero_hand, vs_range, flop);
        for _ in 0..num_runouts {
            if let Some([turn, river]) = sample_two_cards(&available, rng) {
                results.push(eval.equity_for_runout(hero_hand, vs_range, flop, turn, river));
            }
        }
    } else {
        for _ in 0..num_runouts {
            if let Some([turn, river]) = sample_two_cards(&available, rng) {
                let full_board = [flop[0], flop[1], flop[2], turn, river];
                results.push(calculate_omaha_leaf_equity(
                    hero_hand,
                    vs_range,
                    &full_board,
                ));
            }
        }
    }

    results
}

/// Monte Carlo simulation for Omaha equity on the flop
/// Samples `num_runouts` random turn and river combinations
/// Returns equity for each sampled runout
pub fn calculate_omaha_equity_monte_carlo_flop(
    hero_hand: &[u8],
    vs_range: &OmahaRange,
    flop: &[u8; 3],
    num_runouts: usize,
) -> Vec<RunoutEquities> {
    let mut rng = rand::rng();
    calculate_omaha_equity_monte_carlo_flop_with_rng(
        hero_hand,
        vs_range,
        flop,
        num_runouts,
        &mut rng,
    )
}

/// Equity of every hand in `hero_range` vs `vs_range`, aggregated over all board
/// runouts. The board may be 3, 4, or 5 cards. When `max_runouts` is set on a
/// 3-card board, that many random turn/river runouts are sampled (Monte Carlo)
/// instead of enumerating all of them. Results are aligned to `hero_range` order;
/// each hero's win/tie/lose is the sum of its per-runout villain weight (ratios
/// are meaningful; absolute magnitude scales with the runout count).
pub fn calculate_omaha_range_equity(
    hero_range: &OmahaRange,
    vs_range: &OmahaRange,
    board: &[u8],
    max_runouts: Option<usize>,
) -> Result<Vec<OmahaEquityResult>, String> {
    let k = hero_range.hand_size();
    if k != vs_range.hand_size() {
        return Err(format!(
            "Hero range hand size ({}) must match villain range hand size ({})",
            k,
            vs_range.hand_size()
        ));
    }
    if !(3..=5).contains(&board.len()) {
        return Err("Board must be 3, 4, or 5 cards".to_string());
    }

    let mut agg = vec![Equity::default(); hero_range.len()];

    if hero_range.len() == 1 {
        // A single hero hand routes through the optimized single-hero engine,
        // which shares board evaluation across runouts; we then sum its runouts.
        let hero_hand: Vec<u8> = hero_range.iter().next().unwrap().0.to_vec();
        let runouts = match (board.len(), max_runouts) {
            (3, Some(n)) => {
                let flop = [board[0], board[1], board[2]];
                calculate_omaha_equity_monte_carlo_flop(&hero_hand, vs_range, &flop, n)
            }
            _ => calculate_omaha_equity_vs_range(&hero_hand, vs_range, board)?,
        };
        for r in &runouts {
            agg[0].win += r.equity.win;
            agg[0].tie += r.equity.tie;
            agg[0].lose += r.equity.lose;
        }
    } else {
        // A hero range uses the sorted + inclusion-exclusion primitive per runout.
        let accumulate = |board5: &[u8; 5], agg: &mut [Equity]| {
            for (a, e) in agg.iter_mut().zip(calculate_omaha_leaf_equity_range(
                hero_range, vs_range, board5,
            )) {
                a.win += e.win;
                a.tie += e.tie;
                a.lose += e.lose;
            }
        };

        let used = cards_to_mask(board);
        match board.len() {
            5 => accumulate(
                &[board[0], board[1], board[2], board[3], board[4]],
                &mut agg,
            ),
            4 => {
                for river in 0..52u8 {
                    if used & (1u64 << river) == 0 {
                        accumulate(&[board[0], board[1], board[2], board[3], river], &mut agg);
                    }
                }
            }
            _ => {
                if let Some(n) = max_runouts {
                    let available: Vec<u8> =
                        (0..52u8).filter(|&c| used & (1u64 << c) == 0).collect();
                    let mut rng = rand::rng();
                    for _ in 0..n {
                        if let Some([t, r]) = sample_two_cards(&available, &mut rng) {
                            accumulate(&[board[0], board[1], board[2], t, r], &mut agg);
                        }
                    }
                } else {
                    for turn in 0..52u8 {
                        if used & (1u64 << turn) != 0 {
                            continue;
                        }
                        let turn_mask = used | (1u64 << turn);
                        for river in (turn + 1)..52u8 {
                            if turn_mask & (1u64 << river) == 0 {
                                accumulate(&[board[0], board[1], board[2], turn, river], &mut agg);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(hero_range
        .iter()
        .zip(agg)
        .map(|((hand, _w), equity)| OmahaEquityResult {
            hand: hand.to_vec(),
            equity,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
    use std::collections::HashSet;
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    /// Independent, minimal reimplementation of leaf equity: a plain per-villain
    /// loop with a straightforward bit-by-bit card overlap check, deliberately not
    /// sharing the production helpers' structure. A correct
    /// `calculate_omaha_leaf_equity` must produce bit-identical results.
    fn reference_leaf(hero: &[u8], range: &OmahaRange, board: &[u8; 5]) -> (f32, f32, f32) {
        // Build the dead-card set one bit at a time (no shared mask helper).
        let mut dead = 0u64;
        for &c in hero {
            dead |= 1u64 << c;
        }
        for &c in board {
            dead |= 1u64 << c;
        }

        let board_ps = compute_board_ps(board);
        let hero_rank = eval_omaha_hand(hero, &board_ps);
        let (mut w, mut t, mut l) = (0.0f32, 0.0f32, 0.0f32);
        for (vh, weight) in range.iter() {
            if vh.iter().any(|&c| dead & (1u64 << c) != 0) {
                continue;
            }
            let vr = eval_omaha_hand(vh, &board_ps);
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

    fn deterministic_range(size: usize, board_mask: u64, seed: u64) -> OmahaRange {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut range = OmahaRange::new(4);
        let mut seen = HashSet::new();
        let max_attempts = size.saturating_mul(50).max(1);
        for _ in 0..max_attempts {
            if range.len() >= size {
                break;
            }
            let (hand, _) = random_plo4(board_mask, &mut rng);
            let mut key = hand;
            key.sort_unstable();
            if seen.insert(key) {
                range.add_hand(&hand, rng.random_range(0.1..1.0));
            }
        }
        assert_eq!(range.len(), size);
        range
    }

    #[derive(Default)]
    struct StageTimes {
        board: Duration,
        villain_build: Duration,
        all_weights: Duration,
        hero_build: Duration,
        sort: Duration,
        sweep: Duration,
    }

    fn timed_range_equity_plo4(
        hero_range: &OmahaRange,
        vs_range: &OmahaRange,
        board: &[u8; 5],
    ) -> (Vec<Equity>, StageTimes) {
        let mut times = StageTimes::default();

        let t = Instant::now();
        let board_mask = cards_to_mask(board);
        let board_ps = compute_board_ps(board);
        times.board += t.elapsed();

        let k = vs_range.hand_size();
        let stride = (1usize << k) - 1;
        let m = vs_range.len();

        struct ProfileVillain {
            rank: i32,
            weight: f32,
            off: u32,
        }
        let mut villains: Vec<ProfileVillain> = Vec::with_capacity(m);
        let mut villain_slots: Vec<u32> = Vec::with_capacity(m * stride);
        let mut total_all = 0.0f32;

        let t = Instant::now();
        for (_hand, weight, mask, pair_states, _subset_masks, subset_slots) in
            vs_range.iter_eval_and_subsets()
        {
            if mask & board_mask != 0 {
                continue;
            }
            let rank = eval_omaha_pair_states(pair_states, &board_ps);
            let off = villain_slots.len() as u32;
            villain_slots.extend_from_slice(subset_slots);
            total_all += weight;
            villains.push(ProfileVillain { rank, weight, off });
        }
        times.villain_build += t.elapsed();

        let n_slots = vs_range.subset_slot_count();
        let arr_len = n_slots + 1;

        let t = Instant::now();
        let mut a_all = vec![0.0f32; arr_len];
        for v in &villains {
            for &sl in &villain_slots[v.off as usize..v.off as usize + stride] {
                a_all[sl as usize + 1] += v.weight;
            }
        }
        times.all_weights += t.elapsed();

        let hero_count = hero_range.len();
        let mut hero_rank = vec![0i32; hero_count];
        let mut hero_blocked = vec![false; hero_count];
        let odd_terms = 1usize << (k - 1);
        let mut hero_slots: Vec<u32> = Vec::with_capacity(hero_count * stride);

        let t = Instant::now();
        for (hi, (_hand, _w, mask, pair_states, subset_masks, _own_subset_slots)) in
            hero_range.iter_eval_and_subsets().enumerate()
        {
            let base = hero_slots.len();
            hero_slots.resize(base + stride, 0);
            if mask & board_mask != 0 {
                hero_blocked[hi] = true;
                continue;
            }
            hero_rank[hi] = eval_omaha_pair_states(pair_states, &board_ps);
            for (i, &subset) in subset_masks.iter().enumerate() {
                hero_slots[base + i] = vs_range.subset_slot(subset).map(|sl| sl + 1).unwrap_or(0);
            }
        }
        times.hero_build += t.elapsed();

        let t = Instant::now();
        villains.sort_unstable_by_key(|v| v.rank);
        let mut hero_order: Vec<usize> = (0..hero_count).collect();
        hero_order.sort_unstable_by_key(|&i| hero_rank[i]);
        times.sort += t.elapsed();

        let t = Instant::now();
        let mut equities = vec![Equity::default(); hero_count];
        let mut w_run = vec![0.0f32; arr_len];
        let mut weaker_total = 0.0f32;
        let mut w_group = vec![0.0f32; arr_len];
        let mut group_touched: Vec<u32> = Vec::new();

        let m = villains.len();
        let mut vi = 0usize;
        let mut hoi = 0usize;
        while hoi < hero_count {
            let r = hero_rank[hero_order[hoi]];

            while vi < m && villains[vi].rank < r {
                let v = &villains[vi];
                for &sl in &villain_slots[v.off as usize..v.off as usize + stride] {
                    w_run[sl as usize + 1] += v.weight;
                }
                weaker_total += v.weight;
                vi += 1;
            }

            group_touched.clear();
            let mut group_total = 0.0f32;
            let mut gj = vi;
            while gj < m && villains[gj].rank == r {
                let v = &villains[gj];
                for &sl in &villain_slots[v.off as usize..v.off as usize + stride] {
                    let idx = sl + 1;
                    if w_group[idx as usize] == 0.0 {
                        group_touched.push(idx);
                    }
                    w_group[idx as usize] += v.weight;
                }
                group_total += v.weight;
                gj += 1;
            }

            while hoi < hero_count && hero_rank[hero_order[hoi]] == r {
                let hi = hero_order[hoi];
                if hero_blocked[hi] {
                    hoi += 1;
                    continue;
                }
                let (mut blk_lt, mut blk_eq, mut blk_all) = (0.0f32, 0.0f32, 0.0f32);
                let terms = &hero_slots[hi * stride..hi * stride + stride];
                for &sl in &terms[..odd_terms] {
                    let s = sl as usize;
                    blk_lt += w_run[s];
                    blk_eq += w_group[s];
                    blk_all += a_all[s];
                }
                for &sl in &terms[odd_terms..] {
                    let s = sl as usize;
                    blk_lt -= w_run[s];
                    blk_eq -= w_group[s];
                    blk_all -= a_all[s];
                }
                let win = weaker_total - blk_lt;
                let tie = group_total - blk_eq;
                let lose = (total_all - blk_all) - win - tie;
                equities[hi] = Equity { win, tie, lose };
                hoi += 1;
            }

            for &sl in &group_touched {
                w_group[sl as usize] = 0.0;
            }
        }
        times.sweep += t.elapsed();

        (equities, times)
    }

    #[test]
    #[ignore = "stage-level profiler; run manually with --ignored --nocapture"]
    fn profile_range_equity_2000_stages() {
        let board = [34, 21, 46, 8, 17];
        let board_mask = cards_to_mask(&board);
        let hero = deterministic_range(2000, board_mask, 0x4845 + 2000);
        let vs = deterministic_range(2000, board_mask, 0x5653 + 2000);

        let got = calculate_omaha_leaf_equity_range(&hero, &vs, &board);
        let (profiled, _) = timed_range_equity_plo4(&hero, &vs, &board);
        assert_eq!(got.len(), profiled.len());
        for (a, b) in got.iter().zip(&profiled) {
            assert!((a.win - b.win).abs() <= 0.001);
            assert!((a.tie - b.tie).abs() <= 0.001);
            assert!((a.lose - b.lose).abs() <= 0.001);
        }

        let mut total = StageTimes::default();
        let iters = 200;
        for _ in 0..iters {
            let (_equities, t) =
                timed_range_equity_plo4(black_box(&hero), black_box(&vs), black_box(&board));
            total.board += t.board;
            total.villain_build += t.villain_build;
            total.all_weights += t.all_weights;
            total.hero_build += t.hero_build;
            total.sort += t.sort;
            total.sweep += t.sweep;
        }

        let total_ns = total.board.as_nanos()
            + total.villain_build.as_nanos()
            + total.all_weights.as_nanos()
            + total.hero_build.as_nanos()
            + total.sort.as_nanos()
            + total.sweep.as_nanos();
        let print = |label: &str, d: Duration| {
            let ns = d.as_nanos();
            eprintln!(
                "{label:>14}: {:>8.2} us/iter ({:>5.1}%)",
                ns as f64 / iters as f64 / 1000.0,
                ns as f64 * 100.0 / total_ns as f64
            );
        };
        eprintln!("profile_range_equity_2000_stages ({iters} iterations)");
        print("board", total.board);
        print("villain_build", total.villain_build);
        print("all_weights", total.all_weights);
        print("hero_build", total.hero_build);
        print("sort", total.sort);
        print("sweep", total.sweep);

        let board_ps = compute_board_ps(&board);

        let t = Instant::now();
        let mut rank_acc = 0i32;
        for _ in 0..iters {
            for (_hand, _w, _mask, pairs) in hero.iter_eval_ready() {
                rank_acc ^= eval_omaha_pair_states(black_box(pairs), black_box(&board_ps));
            }
            for (_hand, _w, _mask, pairs) in vs.iter_eval_ready() {
                rank_acc ^= eval_omaha_pair_states(black_box(pairs), black_box(&board_ps));
            }
        }
        let rank_eval = t.elapsed();

        let t = Instant::now();
        let mut copied_slots = 0usize;
        for _ in 0..iters {
            for (_hand, _w, _mask, _pairs, _subset_masks, subset_slots) in
                vs.iter_eval_and_subsets()
            {
                for &slot in subset_slots {
                    copied_slots += slot as usize;
                }
            }
        }
        let cached_slot_copy = t.elapsed();

        let t = Instant::now();
        let mut lookup_slots = 0usize;
        for _ in 0..iters {
            for (_hand, _w, _mask, _pairs, subset_masks, _subset_slots) in
                hero.iter_eval_and_subsets()
            {
                for &subset in subset_masks {
                    lookup_slots += vs.subset_slot(subset).unwrap_or(0) as usize;
                }
            }
        }
        let subset_lookup = t.elapsed();

        eprintln!("micro chunks ({iters} iterations)");
        eprintln!(
            "{:>14}: {:>8.2} us/iter",
            "rank_eval_4k",
            rank_eval.as_nanos() as f64 / iters as f64 / 1000.0
        );
        eprintln!(
            "{:>14}: {:>8.2} us/iter",
            "slot_copy",
            cached_slot_copy.as_nanos() as f64 / iters as f64 / 1000.0
        );
        eprintln!(
            "{:>14}: {:>8.2} us/iter",
            "subset_lookup",
            subset_lookup.as_nanos() as f64 / iters as f64 / 1000.0
        );
        black_box((rank_acc, copied_slots, lookup_slots));
    }

    /// Full leaf-equity pipeline must match the brute-force reference exactly,
    /// across many random scenarios.
    #[test]
    fn leaf_equity_matches_reference() {
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

            let got = calculate_omaha_leaf_equity(&hero, &range, &board);
            let (w, t, l) = reference_leaf(&hero, &range, &board);

            assert_eq!(got.equity.win, w, "win mismatch");
            assert_eq!(got.equity.tie, t, "tie mismatch");
            assert_eq!(got.equity.lose, l, "lose mismatch");
        }
    }

    /// The flop-shared evaluator must produce bit-identical results to calling
    /// the trusted single-leaf path on each enumerated runout.
    ///
    /// This re-derives a full per-leaf reference for every one of the ~1980
    /// runouts, so the scenario count and range size are kept modest — the goal
    /// is to validate the shared-vs-per-leaf invariant, not to stress throughput.
    #[test]
    fn flop_enumeration_matches_per_leaf() {
        let mut rng = rand::rng();

        for _ in 0..4 {
            // Random distinct flop + hero.
            let mut used = 0u64;
            let mut flop = [0u8; 3];
            for slot in flop.iter_mut() {
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

            let mut range = OmahaRange::new(4);
            for _ in 0..40 {
                let base_used = if rng.random_bool(0.5) { used } else { 0 };
                let (vh, _) = random_plo4(base_used, &mut rng);
                let weight: f32 = rng.random_range(0.1..1.0);
                range.add_hand(&vh, weight);
            }

            let shared = calculate_omaha_equity_from_flop(&hero, &range, &flop);
            assert!(!shared.is_empty());
            for runout in &shared {
                let expected = calculate_omaha_leaf_equity(&hero, &range, &runout.board);
                assert_eq!(runout.equity, expected.equity, "board {:?}", runout.board);
            }
        }
    }

    #[test]
    fn flop_precompute_ignores_impossible_villain_cards() {
        let flop = [0, 6, 12];
        let hero = [35, 34, 31, 30];
        let mut range = OmahaRange::new(4);

        // This hand is impossible on the flop but used to be evaluated during
        // shared flop precompute, creating duplicate-card evaluator state.
        range.add_hand(&[0, 1, 2, 3], 1.0);
        range.add_hand(&[51, 50, 47, 46], 1.0);

        let shared = calculate_omaha_equity_from_flop(&hero, &range, &flop);
        assert!(!shared.is_empty());
        for runout in &shared {
            let expected = calculate_omaha_leaf_equity(&hero, &range, &runout.board);
            assert_eq!(runout.equity, expected.equity, "board {:?}", runout.board);
        }
    }

    #[test]
    fn single_hero_paths_skip_board_conflicting_hero() {
        let hero = [0, 34, 31, 30];
        let mut range = OmahaRange::new(4);
        range.add_hand(&[51, 50, 47, 46], 1.0);

        let leaf = calculate_omaha_leaf_equity(&hero, &range, &[0, 6, 12, 19, 43]);
        assert_eq!(leaf.equity, Equity::default());
        assert!(
            calculate_omaha_equity_vs_range(&hero, &range, &[0, 6, 12])
                .unwrap()
                .is_empty()
        );
        assert!(
            calculate_omaha_equity_vs_range(&hero, &range, &[0, 6, 12, 19])
                .unwrap()
                .is_empty()
        );
        assert!(
            calculate_omaha_equity_monte_carlo_flop(&hero, &range, &[0, 6, 12], 100).is_empty()
        );
    }

    #[test]
    fn runout_api_preserves_turn_cardinality_by_hand_size() {
        let board = [0, 1, 2, 3];
        for (hand_size, expected_len) in [(4, 44), (5, 43), (6, 42)] {
            let hero: Vec<u8> = (4..4 + hand_size as u8).collect();
            let villain: Vec<u8> = (20..20 + hand_size as u8).collect();
            let mut range = OmahaRange::new(hand_size);
            range.add_hand(&villain, 1.0);

            let runouts =
                calculate_omaha_runout_equity_vs_range(&hero, &range, &board, None, None).unwrap();
            assert_eq!(runouts.len(), expected_len);
            assert_eq!(runouts[0].board, [0, 1, 2, 3, 4 + hand_size as u8]);
            assert_eq!(runouts.last().unwrap().board, [0, 1, 2, 3, 51]);
        }
    }

    #[test]
    fn runout_api_samples_seeded_flops_deterministically() {
        let hero = [4, 5, 6, 7];
        let mut range = OmahaRange::new(4);
        range.add_hand(&[20, 21, 22, 23], 1.0);
        range.add_hand(&[24, 25, 26, 27], 0.5);
        let board = [0, 1, 2];

        let first =
            calculate_omaha_runout_equity_vs_range(&hero, &range, &board, Some(8), Some(1234))
                .unwrap();
        let second =
            calculate_omaha_runout_equity_vs_range(&hero, &range, &board, Some(8), Some(1234))
                .unwrap();

        assert_eq!(first.len(), 8);
        assert_eq!(first, second);
    }

    #[test]
    fn single_hero_aggregate_matches_runout_sum() {
        let hero = [4, 5, 6, 7];
        let mut hero_range = OmahaRange::new(4);
        hero_range.add_hand(&hero, 1.0);
        let mut vs_range = OmahaRange::new(4);
        vs_range.add_hand(&[20, 21, 22, 23], 1.0);
        vs_range.add_hand(&[24, 25, 26, 27], 0.5);
        let board = [0, 1, 2, 3];

        let aggregate = calculate_omaha_range_equity(&hero_range, &vs_range, &board, None).unwrap();
        let runouts =
            calculate_omaha_runout_equity_vs_range(&hero, &vs_range, &board, None, None).unwrap();
        let summed = runouts.iter().fold(Equity::default(), |mut acc, runout| {
            acc.win += runout.equity.win;
            acc.tie += runout.equity.tie;
            acc.lose += runout.equity.lose;
            acc
        });

        assert_eq!(aggregate.len(), 1);
        assert_eq!(aggregate[0].equity, summed);
    }

    /// The range primitive must match single-hero leaf equity computed separately
    /// for each hero, within float tolerance (the inclusion-exclusion sweep sums
    /// weights in a different order than the direct loop).
    #[test]
    fn range_equity_matches_per_hero() {
        let mut rng = rand::rng();
        let mut max_err = 0.0f32;

        for _ in 0..40 {
            // Random board.
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

            // Hero and villain ranges of board-disjoint PLO4 hands. Overlap between
            // the two ranges (and within) exercises the blocker IE thoroughly.
            let mut hero_range = OmahaRange::new(4);
            let mut vs_range = OmahaRange::new(4);
            for _ in 0..80 {
                let (h, _) = random_plo4(used, &mut rng);
                hero_range.add_hand(&h, rng.random_range(0.1..1.0));
            }
            for _ in 0..120 {
                let (v, _) = random_plo4(used, &mut rng);
                vs_range.add_hand(&v, rng.random_range(0.1..1.0));
            }

            let got = calculate_omaha_leaf_equity_range(&hero_range, &vs_range, &board);

            for (hi, (hand, _w)) in hero_range.iter().enumerate() {
                let h: Vec<u8> = hand.to_vec();
                let expect = calculate_omaha_leaf_equity(&h, &vs_range, &board);
                let mut chk = |a: f32, b: f32, label: &str| {
                    let err = (a - b).abs();
                    if err > max_err {
                        max_err = err;
                    }
                    assert!(
                        err <= 0.05 + 1e-3 * b.abs(),
                        "{label} mismatch hero {h:?}: {a} vs {b}"
                    );
                };
                chk(got[hi].win, expect.equity.win, "win");
                chk(got[hi].tie, expect.equity.tie, "tie");
                chk(got[hi].lose, expect.equity.lose, "lose");
            }
        }
        eprintln!("range_equity_matches_per_hero: max abs error = {max_err}");
    }

    /// The multi-hand range enumeration (`calculate_omaha_range_equity`) must agree
    /// with summing the single-hero engine over the same runouts, for every hero.
    #[test]
    fn range_equity_enumeration_matches_single_hero() {
        let mut rng = rand::rng();
        let mut max_err = 0.0f32;

        for &board_len in &[3usize, 4] {
            for _ in 0..6 {
                let mut used = 0u64;
                let mut board = vec![0u8; board_len];
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

                let mut hero_range = OmahaRange::new(4);
                for _ in 0..8 {
                    let (h, _) = random_plo4(used, &mut rng);
                    hero_range.add_hand(&h, 1.0);
                }
                let mut vs_range = OmahaRange::new(4);
                for _ in 0..60 {
                    let (v, _) = random_plo4(used, &mut rng);
                    vs_range.add_hand(&v, rng.random_range(0.1..1.0));
                }

                let got =
                    calculate_omaha_range_equity(&hero_range, &vs_range, &board, None).unwrap();

                for res in &got {
                    // Sum the single-hero engine over the same runouts as the oracle.
                    let runouts =
                        calculate_omaha_equity_vs_range(&res.hand, &vs_range, &board).unwrap();
                    let (mut w, mut t, mut l) = (0.0f32, 0.0f32, 0.0f32);
                    for r in &runouts {
                        w += r.equity.win;
                        t += r.equity.tie;
                        l += r.equity.lose;
                    }
                    for (a, b) in [
                        (res.equity.win, w),
                        (res.equity.tie, t),
                        (res.equity.lose, l),
                    ] {
                        let err = (a - b).abs();
                        if err > max_err {
                            max_err = err;
                        }
                        assert!(err <= 0.1 + 1e-3 * b.abs(), "{:?}: {a} vs {b}", res.hand);
                    }
                }
            }
        }
        eprintln!("range_equity_enumeration_matches_single_hero: max abs error = {max_err}");
    }
}
