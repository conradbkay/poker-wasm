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

/// Open-addressing `u64 -> dense-slot` map for the blocker inclusion-exclusion.
/// Subset bitmasks are always non-zero, so `0` doubles as the empty marker, and a
/// multiplicative (Fibonacci) hash keeps a lookup to a couple of loads — the std
/// `HashMap`'s SipHash dominated this hot path otherwise.
struct SubsetSlots {
    keys: Vec<u64>,
    slots: Vec<u32>,
    mask: usize,
    shift: u32,
    len: usize,
}

impl SubsetSlots {
    fn with_capacity(n: usize) -> Self {
        let cap = (n.max(8) * 2).next_power_of_two();
        Self {
            keys: vec![0u64; cap],
            slots: vec![0u32; cap],
            mask: cap - 1,
            shift: 64 - cap.trailing_zeros(),
            len: 0,
        }
    }

    #[inline]
    fn index(&self, key: u64) -> usize {
        (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> self.shift) as usize
    }

    #[inline]
    fn get(&self, key: u64) -> Option<u32> {
        let mut i = self.index(key);
        loop {
            let k = self.keys[i];
            if k == 0 {
                return None;
            }
            if k == key {
                return Some(self.slots[i]);
            }
            i = (i + 1) & self.mask;
        }
    }

    /// Slot for `key`, inserting a fresh dense id (`*next`, then bumped) if absent.
    #[inline]
    fn get_or_insert(&mut self, key: u64, next: &mut u32) -> u32 {
        if (self.len + 1) * 2 > self.keys.len() {
            self.grow();
        }
        let mut i = self.index(key);
        loop {
            let k = self.keys[i];
            if k == 0 {
                self.keys[i] = key;
                let s = *next;
                self.slots[i] = s;
                *next += 1;
                self.len += 1;
                return s;
            }
            if k == key {
                return self.slots[i];
            }
            i = (i + 1) & self.mask;
        }
    }

    fn grow(&mut self) {
        let new_cap = self.keys.len() * 2;
        let mut new = SubsetSlots {
            keys: vec![0u64; new_cap],
            slots: vec![0u32; new_cap],
            mask: new_cap - 1,
            shift: 64 - new_cap.trailing_zeros(),
            len: self.len,
        };
        for (i, &k) in self.keys.iter().enumerate() {
            if k != 0 {
                let mut j = new.index(k);
                while new.keys[j] != 0 {
                    j = (j + 1) & new.mask;
                }
                new.keys[j] = k;
                new.slots[j] = self.slots[i];
            }
        }
        *self = new;
    }
}

/// Per board-subset state after adding a single hole card, for every card. This is
/// board-only, so it's built once and shared by the hero and the whole villain
/// range; indexing it turns each 2-hole-card evaluation into two table loads
/// (one `next_p` for the second card + one `final_p`) instead of three.
fn compute_board_s1(ranks: &[i32], board_ps: &[usize; 10]) -> [[u32; 52]; 10] {
    let mut s1 = [[0u32; 52]; 10];
    for (sub, &bp) in board_ps.iter().enumerate() {
        for (card, slot) in s1[sub].iter_mut().enumerate() {
            *slot = next_p(ranks, bp + card);
        }
    }
    s1
}

/// Best 5-card rank over every legal 2-hole-card combination, using the shared
/// single-card continuation table. Equivalent to [`eval_omaha_hand`] but cheaper.
#[inline]
fn eval_omaha_hand_s1(
    ranks: &[i32],
    hole: &[u8],
    s1: &[[u32; 52]; 10],
    combos: &'static [[usize; 2]],
) -> i32 {
    let mut best = i32::MIN;
    for row in s1.iter() {
        for &[a, b] in combos {
            let combined = next_p(ranks, row[hole[a] as usize] as usize + hole[b] as usize);
            best = best.max(final_p(ranks, combined as usize) as i32);
        }
    }
    best
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
    ranks_data: &[i32],
    hero_range: &OmahaRange,
    vs_range: &OmahaRange,
    board: &[u8; 5],
) -> Vec<Equity> {
    let board_mask = cards_to_mask(board);
    let board_ps = compute_board_ps(ranks_data, board);
    let s1 = compute_board_s1(ranks_data, &board_ps);

    let k = vs_range.hand_size();
    debug_assert_eq!(k, hero_range.hand_size(), "hero/villain hand sizes must match");
    let stride = (1usize << k) - 1; // non-empty subsets of a board-disjoint k-card hand
    let combos = hole_combos(k);

    // --- Villains: evaluate once; give every distinct subset a dense slot. ---
    let m = vs_range.len();
    let mut map = SubsetSlots::with_capacity(m * stride);
    let mut next_slot: u32 = 0;

    struct Villain {
        rank: i32,
        weight: f32,
        off: u32, // start of this villain's `stride` slots in `villain_slots`
    }
    let mut villains: Vec<Villain> = Vec::with_capacity(m);
    let mut villain_slots: Vec<u32> = Vec::with_capacity(m * stride);
    let mut total_all = 0.0f32;

    for (hand, weight, mask) in vs_range.iter_masked() {
        if mask & board_mask != 0 {
            continue; // villain shares a board card -> impossible on this board
        }
        let rank = eval_omaha_hand_s1(ranks_data, hand, &s1, combos);
        let off = villain_slots.len() as u32;
        let mut s = mask;
        while s != 0 {
            villain_slots.push(map.get_or_insert(s, &mut next_slot));
            s = (s - 1) & mask;
        }
        total_all += weight;
        villains.push(Villain { rank, weight, off });
    }

    let n_slots = next_slot as usize;
    let sentinel = n_slots as u32; // trailing always-zero slot for hero-only subsets
    let arr_len = n_slots + 1;

    // A_all[slot] = total villain weight whose cards ⊇ subset(slot).
    let mut a_all = vec![0.0f32; arr_len];
    for v in &villains {
        for &sl in &villain_slots[v.off as usize..v.off as usize + stride] {
            a_all[sl as usize] += v.weight;
        }
    }

    // Per-hero rank and the signed subset terms for the IE sum, stored flat at the
    // same `stride`. A hero hand sharing a board card is impossible on this board
    // and stays at zero equity (matching the board-blocked handling for villains).
    let hero_count = hero_range.len();
    let mut hero_rank = vec![0i32; hero_count];
    let mut hero_blocked = vec![false; hero_count];
    let mut hero_terms: Vec<(u32, f32)> = Vec::with_capacity(hero_count * stride);
    for (hi, (hand, _w, mask)) in hero_range.iter_masked().enumerate() {
        if mask & board_mask != 0 {
            hero_blocked[hi] = true;
            hero_terms.resize(hero_terms.len() + stride, (sentinel, 0.0));
            continue;
        }
        hero_rank[hi] = eval_omaha_hand_s1(ranks_data, hand, &s1, combos);
        let mut s = mask;
        while s != 0 {
            let slot = map.get(s).unwrap_or(sentinel);
            let sign = if s.count_ones() & 1 == 1 { 1.0 } else { -1.0 };
            hero_terms.push((slot, sign));
            s = (s - 1) & mask;
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
            for &sl in &villain_slots[v.off as usize..v.off as usize + stride] {
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
            if hero_blocked[hi] {
                hoi += 1;
                continue; // impossible on this board; leaves zero equity
            }
            let (mut blk_lt, mut blk_eq, mut blk_all) = (0.0f32, 0.0f32, 0.0f32);
            for &(sl, sign) in &hero_terms[hi * stride..hi * stride + stride] {
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

/// Equity of every hand in `hero_range` vs `vs_range`, aggregated over all board
/// runouts. The board may be 3, 4, or 5 cards. When `max_runouts` is set on a
/// 3-card board, that many random turn/river runouts are sampled (Monte Carlo)
/// instead of enumerating all of them. Results are aligned to `hero_range` order;
/// each hero's win/tie/lose is the sum of its per-runout villain weight (ratios
/// are meaningful; absolute magnitude scales with the runout count).
pub fn calculate_omaha_range_equity(
    ranks_data: &[i32],
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
                calculate_omaha_equity_monte_carlo_flop(ranks_data, &hero_hand, vs_range, &flop, n)
            }
            _ => calculate_omaha_equity_vs_range(ranks_data, &hero_hand, vs_range, board)?,
        };
        for r in &runouts {
            agg[0].win += r.equity.win;
            agg[0].tie += r.equity.tie;
            agg[0].lose += r.equity.lose;
        }
    } else {
        // A hero range uses the sorted + inclusion-exclusion primitive per runout.
        let accumulate = |board5: &[u8; 5], agg: &mut [Equity]| {
            for (a, e) in agg
                .iter_mut()
                .zip(calculate_omaha_leaf_equity_range(ranks_data, hero_range, vs_range, board5))
            {
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

    /// The multi-hand range enumeration (`calculate_omaha_range_equity`) must agree
    /// with summing the single-hero engine over the same runouts, for every hero.
    #[test]
    fn range_equity_enumeration_matches_single_hero() {
        let ranks = load_ranks();
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
                    calculate_omaha_range_equity(&ranks, &hero_range, &vs_range, &board, None)
                        .unwrap();

                for res in &got {
                    // Sum the single-hero engine over the same runouts as the oracle.
                    let runouts =
                        calculate_omaha_equity_vs_range(&ranks, &res.hand, &vs_range, &board)
                            .unwrap();
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