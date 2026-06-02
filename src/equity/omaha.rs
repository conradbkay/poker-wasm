use wasm_bindgen::prelude::*;
use crate::evaluation::{fast_eval, final_p, next_p, combinations::{HOLE_COMBOS_2_FROM_4, HOLE_COMBOS_2_FROM_5, HOLE_COMBOS_2_FROM_6, BOARD_COMBOS_3_FROM_5}};
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

/// Best 5-card rank achievable by completing a 3-card board state (`board_p`)
/// with every legal 2-card hole combination. This is the per-board-subset inner
/// max that all the sharing below is built on.
#[inline]
fn best_over_holes(
    ranks_data: &[i32],
    board_p: usize,
    hole_cards: &[u8],
    combos: &[[usize; 2]],
) -> i32 {
    let mut best = i32::MIN;
    for &[a, b] in combos {
        let combined_p = fast_eval(ranks_data, &[hole_cards[a], hole_cards[b]], board_p);
        let rank = final_p(ranks_data, combined_p as usize) as i32;
        best = best.max(rank);
    }
    best
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
struct OmahaFlopEval<'a> {
    ranks: &'a [i32],
    combos: &'static [[usize; 2]],
    /// Board state after each single flop card (for the type-2 subsets).
    oneflop_p: [usize; 3],
    hero_a: i32,
    hero_d: [i32; 52],
    /// Per-villain type-0 scalar and type-1 table (`m` rows of 52).
    villain_a: Vec<i32>,
    villain_d: Vec<i32>,
}

impl<'a> OmahaFlopEval<'a> {
    fn build(
        ranks: &'a [i32],
        hero_hand: &[u8],
        vs_range: &OmahaRange,
        flop: &[u8; 3],
    ) -> Self {
        let combos = hole_combos(hero_hand.len());

        // Board-only partial states (shared by hero and every villain).
        let flop_p = fast_eval(ranks, flop, 53) as usize; // type-0: all three flop cards
        let twoflop_pairs = [[flop[0], flop[1]], [flop[0], flop[2]], [flop[1], flop[2]]];
        let twoflop_p: [usize; 3] =
            std::array::from_fn(|i| fast_eval(ranks, &twoflop_pairs[i], 53) as usize);
        let oneflop_p: [usize; 3] =
            std::array::from_fn(|i| next_p(ranks, 53 + flop[i] as usize) as usize);

        let flop_mask = cards_to_mask(flop);

        // type-1 board states bs1[pair][X] = state after {2 flop cards, X}, for
        // every card X not on the flop. Board-only, so shared across all actors.
        let mut bs1 = [[0usize; 52]; 3];
        for (p, &base) in twoflop_p.iter().enumerate() {
            for x in 0..52usize {
                if flop_mask & (1u64 << x) != 0 {
                    continue;
                }
                bs1[p][x] = next_p(ranks, base + x) as usize;
            }
        }

        // Fill an actor's type-0 scalar and type-1 table. `x` is a card index used
        // for both the bitmask skip and the `d`/`bs1` indexing, so a range loop is
        // the natural form here.
        #[allow(clippy::needless_range_loop)]
        let fill = |hole: &[u8], a: &mut i32, d: &mut [i32]| {
            *a = best_over_holes(ranks, flop_p, hole, combos);
            for x in 0..52usize {
                if flop_mask & (1u64 << x) != 0 {
                    continue;
                }
                let mut best = i32::MIN;
                for p in 0..3 {
                    best = best.max(best_over_holes(ranks, bs1[p][x], hole, combos));
                }
                d[x] = best;
            }
        };

        let mut hero_a = i32::MIN;
        let mut hero_d = [i32::MIN; 52];
        fill(hero_hand, &mut hero_a, &mut hero_d);

        let m = vs_range.len();
        let mut villain_a = vec![i32::MIN; m];
        let mut villain_d = vec![i32::MIN; m * 52];
        for (v, (hole, _w)) in vs_range.iter().enumerate() {
            let mut a = i32::MIN;
            fill(hole, &mut a, &mut villain_d[v * 52..v * 52 + 52]);
            villain_a[v] = a;
        }

        OmahaFlopEval {
            ranks,
            combos,
            oneflop_p,
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
    fn type2_best(&self, hole: &[u8], board3: &[usize; 3]) -> i32 {
        let mut e = i32::MIN;
        for &bp in board3 {
            e = e.max(best_over_holes(self.ranks, bp, hole, self.combos));
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
        let board3: [usize; 3] = std::array::from_fn(|i| {
            let after_turn = next_p(self.ranks, self.oneflop_p[i] + t) as usize;
            next_p(self.ranks, after_turn + r) as usize
        });

        let hero_e = self.type2_best(hero_hand, &board3);
        let hero_rank = self
            .hero_a
            .max(self.hero_d[t])
            .max(self.hero_d[r])
            .max(hero_e);

        let dead_mask =
            cards_to_mask(hero_hand) | cards_to_mask(flop) | (1u64 << t) | (1u64 << r);

        let (mut win, mut tie, mut lose) = (0.0f32, 0.0f32, 0.0f32);
        for (v, (hole, weight, mask)) in vs_range.iter_masked().enumerate() {
            if mask & dead_mask != 0 {
                continue;
            }
            let d = &self.villain_d[v * 52..v * 52 + 52];
            let e = self.type2_best(hole, &board3);
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
pub fn calculate_omaha_leaf_equity_range(
    ranks_data: &[i32],
    hero_range: &OmahaRange,
    vs_range: &OmahaRange,
    board: &[u8; 5],
) -> Vec<Equity> {
    use std::collections::HashMap;

    let board_mask = cards_to_mask(board);
    let board_ps = compute_board_ps(ranks_data, board);

    // Subset (card-bitmask) -> dense slot, built from the live villains. Every
    // non-empty sub-bitmask of a villain's hand gets a slot; W[S] lives at that slot.
    let mut slot_of: HashMap<u64, u32> = HashMap::new();

    struct Villain {
        rank: i32,
        weight: f32,
        slots: Vec<u32>, // slots of all non-empty subsets of this villain's cards
    }
    let mut villains: Vec<Villain> = Vec::new();
    let mut total_all = 0.0f32;

    for (hand, weight, mask) in vs_range.iter_masked() {
        if mask & board_mask != 0 {
            continue; // villain shares a board card -> impossible on this board
        }
        let rank = eval_omaha_hand(ranks_data, hand, &board_ps);
        let mut slots = Vec::new();
        let mut s = mask;
        while s != 0 {
            let next = slot_of.len() as u32;
            slots.push(*slot_of.entry(s).or_insert(next));
            s = (s - 1) & mask;
        }
        total_all += weight;
        villains.push(Villain { rank, weight, slots });
    }

    let n_slots = slot_of.len();

    // A_all[slot] = total villain weight whose cards ⊇ subset(slot).
    let mut a_all = vec![0.0f32; n_slots];
    for v in &villains {
        for &sl in &v.slots {
            a_all[sl as usize] += v.weight;
        }
    }

    // Per-hero rank and the signed subset terms for the IE sum (only subsets that
    // some villain actually has — others contribute 0 and are skipped).
    let hero_count = hero_range.len();
    let mut hero_rank = vec![0i32; hero_count];
    let mut hero_terms: Vec<Vec<(u32, f32)>> = Vec::with_capacity(hero_count);
    for (hand, _w, mask) in hero_range.iter_masked() {
        hero_rank[hero_terms.len()] = eval_omaha_hand(ranks_data, hand, &board_ps);
        let mut terms = Vec::new();
        let mut s = mask;
        while s != 0 {
            if let Some(&sl) = slot_of.get(&s) {
                let sign = if s.count_ones() & 1 == 1 { 1.0 } else { -1.0 };
                terms.push((sl, sign));
            }
            s = (s - 1) & mask;
        }
        hero_terms.push(terms);
    }

    villains.sort_unstable_by_key(|v| v.rank);
    let mut hero_order: Vec<usize> = (0..hero_count).collect();
    hero_order.sort_unstable_by_key(|&i| hero_rank[i]);

    let mut equities = vec![Equity::default(); hero_count];

    let mut w_run = vec![0.0f32; n_slots]; // sums over villains strictly weaker than current rank
    let mut weaker_total = 0.0f32;
    let mut w_group = vec![0.0f32; n_slots]; // sums over villains of exactly the current rank
    let mut group_touched: Vec<u32> = Vec::new();

    let m = villains.len();
    let mut vi = 0usize; // fold pointer into the sorted villains
    let mut hoi = 0usize;
    while hoi < hero_count {
        let r = hero_rank[hero_order[hoi]];

        // Fold all strictly-weaker villains into the running sums.
        while vi < m && villains[vi].rank < r {
            let v = &villains[vi];
            for &sl in &v.slots {
                w_run[sl as usize] += v.weight;
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
            for &sl in &v.slots {
                if w_group[sl as usize] == 0.0 {
                    group_touched.push(sl);
                }
                w_group[sl as usize] += v.weight;
            }
            group_total += v.weight;
            gj += 1;
        }

        // Emit every hero of this rank using the same running/group sums.
        while hoi < hero_count && hero_rank[hero_order[hoi]] == r {
            let hi = hero_order[hoi];
            let (mut blk_lt, mut blk_eq, mut blk_all) = (0.0f32, 0.0f32, 0.0f32);
            for &(sl, sign) in &hero_terms[hi] {
                let s = sl as usize;
                blk_lt += sign * w_run[s];
                blk_eq += sign * w_group[s];
                blk_all += sign * a_all[s];
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

    // Build the flop-shared evaluator once; every runout reuses its precomputed
    // board-subset tables and only re-evaluates the subsets that use both the turn
    // and the river.
    let eval = OmahaFlopEval::build(ranks_data, hero_hand, vs_range, board);

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

    // The flop-shared evaluator precomputes the type-1 `D[X]` table over every
    // live card, which only pays off once enough runouts are sampled to amortize
    // it. Below that crossover (few samples, large range) the per-leaf path is
    // cheaper, so gate on sampling at least as many runouts as live cards.
    if num_runouts >= available.len() {
        let eval = OmahaFlopEval::build(ranks_data, hero_hand, vs_range, flop);
        for _ in 0..num_runouts {
            if let Some([turn, river]) = sample_two_cards(&available, &mut rng) {
                results.push(eval.equity_for_runout(hero_hand, vs_range, flop, turn, river));
            }
        }
    } else {
        for _ in 0..num_runouts {
            if let Some([turn, river]) = sample_two_cards(&available, &mut rng) {
                let full_board = [flop[0], flop[1], flop[2], turn, river];
                results.push(calculate_omaha_leaf_equity(
                    ranks_data, hero_hand, vs_range, &full_board,
                ));
            }
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

    /// Independent, minimal reimplementation of leaf equity: a plain per-villain
    /// loop with a straightforward bit-by-bit card overlap check, deliberately not
    /// sharing the production helpers' structure. A correct
    /// `calculate_omaha_leaf_equity` must produce bit-identical results.
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

    /// The flop-shared evaluator must produce bit-identical results to calling
    /// the trusted single-leaf path on each enumerated runout.
    #[test]
    fn flop_enumeration_matches_per_leaf() {
        let ranks = load_ranks();
        let mut rng = rand::rng();

        for _ in 0..20 {
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
            for _ in 0..120 {
                let base_used = if rng.random_bool(0.5) { used } else { 0 };
                let (vh, _) = random_plo4(base_used, &mut rng);
                let weight: f32 = rng.random_range(0.1..1.0);
                range.add_hand(&vh, weight);
            }

            let shared = calculate_omaha_equity_from_flop(&ranks, &hero, &range, &flop);
            assert!(!shared.is_empty());
            for runout in &shared {
                let expected =
                    calculate_omaha_leaf_equity(&ranks, &hero, &range, &runout.board);
                assert_eq!(runout.equity, expected.equity, "board {:?}", runout.board);
            }
        }
    }

    /// The range primitive must match single-hero leaf equity computed separately
    /// for each hero, within float tolerance (the inclusion-exclusion sweep sums
    /// weights in a different order than the direct loop).
    #[test]
    fn range_equity_matches_per_hero() {
        let ranks = load_ranks();
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

            let got = calculate_omaha_leaf_equity_range(&ranks, &hero_range, &vs_range, &board);

            for (hi, (hand, _w)) in hero_range.iter().enumerate() {
                let h: Vec<u8> = hand.to_vec();
                let expect = calculate_omaha_leaf_equity(&ranks, &h, &vs_range, &board);
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
}