use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use rand::{Rng, thread_rng};
use rand::seq::SliceRandom;
use poker_wasm::{HoldemRange, EquityCalculator};
use std::hint::black_box;

fn create_random_board() -> Vec<u8> {
    let mut rng = thread_rng();
    let mut deck: Vec<u8> = (0..52).collect();
    deck.shuffle(&mut rng);
    deck.into_iter().take(5).collect()
}

fn create_random_range(num_hands: usize, board: &[u8]) -> HoldemRange {
    let mut rng = thread_rng();
    let mut range = HoldemRange::new();
    let board_cards: std::collections::HashSet<u8> = board.iter().cloned().collect();

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
        let weight: f32 = rng.gen_range(0.1..1.0);
        range.set(hand_idx, weight);
    }

    range
}

fn bench_equity_vs_range(c: &mut Criterion) {
    let hand_sizes = vec![1, 2, 5, 10, 50, 100, 250, 500, 1000, 1326];
    
    // Load hand ranks data once
    let hand_ranks_data = include_bytes!("../HandRanks.dat").to_vec();
    let calculator = EquityCalculator::new(hand_ranks_data);

    let mut group = c.benchmark_group("equity_vs_range");

    for &hand_size in &hand_sizes {
        group.bench_with_input(
            BenchmarkId::from_parameter(hand_size),
            &hand_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let board = create_random_board();
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

criterion_group!(benches, bench_equity_vs_range);
criterion_main!(benches); 