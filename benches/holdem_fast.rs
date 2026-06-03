use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use poker_wasm::{HoldemRange, calculate_equity_vs_range};
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use std::hint::black_box;

fn create_random_board(num_cards: usize, seed: u64) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut deck: Vec<u8> = (0..52).collect();
    deck.shuffle(&mut rng);
    deck.into_iter().take(num_cards).collect()
}

fn create_random_range(num_hands: usize, board: &[u8], seed: u64) -> HoldemRange {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut range = HoldemRange::new();
    let mut board_mask = 0u64;
    for &card in board {
        board_mask |= 1u64 << card;
    }

    let mut possible_hands: Vec<usize> = (0..1326)
        .filter(|&idx| {
            let hand = HoldemRange::from_hand_idx(idx);
            board_mask & (1u64 << hand[0]) == 0 && board_mask & (1u64 << hand[1]) == 0
        })
        .collect();

    possible_hands.shuffle(&mut rng);

    for &hand_idx in possible_hands.iter().take(num_hands) {
        range.set(hand_idx, rng.random_range(0.1..1.0));
    }

    range
}

fn bench_holdem_range_vs_range_fast(c: &mut Criterion) {
    let mut group = c.benchmark_group("holdem_range_vs_range_fast");
    group.sample_size(20);

    for &range_size in &[100usize, 1000] {
        let board = create_random_board(5, 0xB0A12 + range_size as u64);
        let hero = create_random_range(range_size, &board, 0x4E40 + range_size as u64);
        let vs = create_random_range(range_size, &board, 0x5151 + range_size as u64);

        group.bench_function(BenchmarkId::from_parameter(range_size), |b| {
            b.iter(|| calculate_equity_vs_range(&hero, &vs, black_box(&board)).unwrap())
        });
    }

    group.finish();
}

criterion_group!(benches, bench_holdem_range_vs_range_fast);
criterion_main!(benches);
