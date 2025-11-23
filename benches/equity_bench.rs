use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use rand::Rng;
use rand::seq::SliceRandom;
use poker_wasm::{HoldemRange, OmahaRange, EquityCalculator};
use std::hint::black_box;
use std::collections::HashSet;

// --- Hold'em Helpers ---

fn create_random_board(num_cards: usize) -> Vec<u8> {
    let mut rng = rand::rng();
    let mut deck: Vec<u8> = (0..52).collect();
    deck.shuffle(&mut rng);
    deck.into_iter().take(num_cards).collect()
}

fn create_random_range(num_hands: usize, board: &[u8]) -> HoldemRange {
    let mut rng = rand::rng();
    let mut range = HoldemRange::new();
    let board_cards: HashSet<u8> = board.iter().cloned().collect();

    let mut possible_hands: Vec<usize> = (0..1326)
        .filter(|&i| {
            let hand = HoldemRange::from_hand_idx(i);
            !board_cards.contains(&hand[0]) && !board_cards.contains(&hand[1])
        })
        .collect();

    // Ensure we don't try to take more hands than are possible
    let num_to_take = num_hands.min(possible_hands.len());

    possible_hands.shuffle(&mut rng);

    for &hand_idx in possible_hands.iter().take(num_to_take) {
        let weight: f32 = rng.random_range(0.1..1.0);
        range.set(hand_idx, weight);
    }

    range
}

// --- Omaha Helpers ---

fn create_random_omaha_hand(used_cards: &HashSet<u8>, hand_size: usize) -> Option<Vec<u8>> {
    let mut rng = rand::rng();
    let mut available: Vec<u8> = (0..52u8)
        .filter(|c| !used_cards.contains(c))
        .collect();

    if available.len() < hand_size {
        return None;
    }

    available.shuffle(&mut rng);
    Some(available.into_iter().take(hand_size).collect())
}

fn create_random_omaha_range(num_hands: usize, used_cards: &HashSet<u8>, hand_size: usize) -> OmahaRange {
    let mut range = OmahaRange::new(hand_size);
    let mut rng = rand::rng();
    let mut added_hands = HashSet::new();

    for _ in 0..num_hands {
        if let Some(hand) = create_random_omaha_hand(used_cards, hand_size) {
            // Avoid duplicates
            let hand_key = format!("{:?}", hand);
            if added_hands.insert(hand_key) {
                let weight: f32 = rng.random_range(0.1..1.0);
                range.add_hand(&hand, weight);
            }
        }
    }

    range
}

// --- Hold'em Benchmarks ---

fn bench_holdem_leaf_equity(c: &mut Criterion) {
    let hand_sizes = vec![ 25, 50, 100, 250, 500, 1000];
    let hand_ranks_data = include_bytes!("../HandRanks.dat").to_vec();
    let mut group = c.benchmark_group("holdem_leaf_equity");

    for &hand_size in &hand_sizes {
        let board = create_random_board(5);
        let mut calculator = EquityCalculator::new(hand_ranks_data.clone());
        calculator.set_hero_range(create_random_range(hand_size, &board));
        calculator.set_vs_range(create_random_range(hand_size, &board));

        group.bench_function(BenchmarkId::from_parameter(hand_size), |b| {
            b.iter(|| {
                calculator.leaf_equity_vs_range(black_box(&board)).unwrap()
            })
        });
    }

    group.finish();
}

// --- Omaha Benchmarks ---

fn bench_omaha_leaf_equity(c: &mut Criterion) {
    let range_sizes = vec![25, 50, 100, 250];
    let hand_ranks_data = include_bytes!("../HandRanks.dat").to_vec();
    let mut group = c.benchmark_group("omaha_leaf_equity");

    for &range_size in &range_sizes {
        let board = create_random_board(5);
        let board_set: HashSet<u8> = board.iter().cloned().collect();
        let hero_hand = create_random_omaha_hand(&board_set, 4).unwrap();

        let mut hero_and_board = board_set.clone();
        for &c in &hero_hand {
            hero_and_board.insert(c);
        }

        let mut calculator = EquityCalculator::new(hand_ranks_data.clone());
        calculator.set_omaha_range(create_random_omaha_range(range_size, &hero_and_board, 4));

        group.bench_function(BenchmarkId::from_parameter(range_size), |b| {
            b.iter(|| {
                calculator.omaha_leaf_equity_vs_range(black_box(&hero_hand), black_box(&board)).unwrap()
            })
        });
    }

    group.finish();
}

fn bench_omaha_flop_monte_carlo(c: &mut Criterion) {
    let configs = vec![
        (50, 100),
        (100, 100),
        (500, 100),
        (1000, 25),
        (1000, 100),
    ];

    let hand_ranks_data = include_bytes!("../HandRanks.dat").to_vec();
    let mut group = c.benchmark_group("omaha_flop_monte_carlo");

    for &(range_size, num_runouts) in &configs {
        let flop = create_random_board(3);
        let flop_set: HashSet<u8> = flop.iter().cloned().collect();
        let hero_hand = create_random_omaha_hand(&flop_set, 4).unwrap();

        let mut hero_and_flop = flop_set.clone();
        for &c in &hero_hand {
            hero_and_flop.insert(c);
        }

        let mut calculator = EquityCalculator::new(hand_ranks_data.clone());
        calculator.set_omaha_range(create_random_omaha_range(range_size, &hero_and_flop, 4));

        group.bench_function(
            BenchmarkId::new(format!("range{}", range_size), format!("runouts{}", num_runouts)),
            |b| b.iter(|| {
                calculator.omaha_monte_carlo_flop(
                    black_box(&hero_hand),
                    black_box(&flop),
                    black_box(num_runouts),
                ).unwrap()
            })
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_holdem_leaf_equity,
    bench_omaha_leaf_equity,
    bench_omaha_flop_monte_carlo
);
criterion_main!(benches); 