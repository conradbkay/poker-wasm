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

fn create_random_omaha_hand(used_cards: &HashSet<u8>) -> Option<[u8; 4]> {
    let mut rng = rand::rng();
    let mut available: Vec<u8> = (0..52u8)
        .filter(|c| !used_cards.contains(c))
        .collect();

    if available.len() < 4 {
        return None;
    }

    available.shuffle(&mut rng);
    Some([available[0], available[1], available[2], available[3]])
}

fn create_random_omaha_range(num_hands: usize, used_cards: &HashSet<u8>) -> OmahaRange {
    let mut range = OmahaRange::new();
    let mut rng = rand::rng();
    let mut added_hands = HashSet::new();

    for _ in 0..num_hands {
        if let Some(hand) = create_random_omaha_hand(used_cards) {
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
    let hand_sizes = vec![1, 2, 5, 10, 50, 100, 250, 500, 1000];

    // Load hand ranks data once
    let hand_ranks_data = include_bytes!("../HandRanks.dat").to_vec();
    let calculator = EquityCalculator::new(hand_ranks_data);

    let mut group = c.benchmark_group("holdem_leaf_equity");

    for &hand_size in &hand_sizes {
        group.bench_with_input(
            BenchmarkId::from_parameter(hand_size),
            &hand_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let board = create_random_board(5);
                        let vs_range = create_random_range(size, &board);
                        let my_range = create_random_range(size, &board);
                        (board, my_range, vs_range)
                    },
                    |(board, my_range, vs_range)| {
                        calculator.leaf_equity_vs_range(
                            black_box(&my_range),
                            black_box(&vs_range),
                            black_box(&board),
                        )
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

// --- Omaha Benchmarks ---

fn bench_omaha_leaf_equity(c: &mut Criterion) {
    let range_sizes = vec![1, 5, 10, 25, 50, 100];

    let hand_ranks_data = include_bytes!("../HandRanks.dat").to_vec();
    let calculator = EquityCalculator::new(hand_ranks_data);

    let mut group = c.benchmark_group("omaha_leaf_equity");

    for &range_size in &range_sizes {
        group.bench_with_input(
            BenchmarkId::from_parameter(range_size),
            &range_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let board = create_random_board(5);
                        let board_set: HashSet<u8> = board.iter().cloned().collect();
                        let hero_hand = create_random_omaha_hand(&board_set).unwrap();

                        let mut hero_and_board = board_set.clone();
                        for &c in &hero_hand {
                            hero_and_board.insert(c);
                        }

                        let vs_range = create_random_omaha_range(size, &hero_and_board);
                        (board, hero_hand, vs_range)
                    },
                    |(board, hero_hand, vs_range)| {
                        calculator.omaha_leaf_equity_vs_range(
                            black_box(&hero_hand),
                            black_box(&vs_range),
                            black_box(&board),
                        )
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

fn bench_omaha_flop_monte_carlo(c: &mut Criterion) {
    let configs = vec![
        (50, 100),   // 50 hands, 100 runouts
        (100, 100),  // 100 hands, 100 runouts
        (500, 100),  // 500 hands, 100 runouts
        (1000, 25),
        (1000, 100),  // 1000 hands, 100 runouts
    ];

    let hand_ranks_data = include_bytes!("../HandRanks.dat").to_vec();
    let calculator = EquityCalculator::new(hand_ranks_data);

    let mut group = c.benchmark_group("omaha_flop_monte_carlo");

    for &(range_size, num_runouts) in &configs {
        group.bench_with_input(
            BenchmarkId::new(
                format!("range{}", range_size),
                format!("runouts{}", num_runouts)
            ),
            &(range_size, num_runouts),
            |b, &(size, runouts)| {
                b.iter_batched(
                    || {
                        let flop = create_random_board(3);
                        let flop_set: HashSet<u8> = flop.iter().cloned().collect();
                        let hero_hand = create_random_omaha_hand(&flop_set).unwrap();

                        let mut hero_and_flop = flop_set.clone();
                        for &c in &hero_hand {
                            hero_and_flop.insert(c);
                        }

                        let vs_range = create_random_omaha_range(size, &hero_and_flop);
                        (flop, hero_hand, vs_range)
                    },
                    |(flop, hero_hand, vs_range)| {
                        calculator.omaha_monte_carlo_flop(
                            black_box(&hero_hand),
                            black_box(&vs_range),
                            black_box(&flop),
                            black_box(runouts),
                        )
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
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