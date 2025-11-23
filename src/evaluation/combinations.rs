// Static lookup tables for Omaha hand evaluation combinations

/// All ways to choose 2 cards from 4 hole cards: C(4,2) = 6
/// Used for PLO4 (standard Pot-Limit Omaha)
pub const HOLE_COMBOS_2_FROM_4: [[usize; 2]; 6] = [
    [0, 1],
    [0, 2],
    [0, 3],
    [1, 2],
    [1, 3],
    [2, 3],
];

/// All ways to choose 2 cards from 5 hole cards: C(5,2) = 10
/// Used for PLO5 (5-card Pot-Limit Omaha)
pub const HOLE_COMBOS_2_FROM_5: [[usize; 2]; 10] = [
    [0, 1],
    [0, 2],
    [0, 3],
    [0, 4],
    [1, 2],
    [1, 3],
    [1, 4],
    [2, 3],
    [2, 4],
    [3, 4],
];

/// All ways to choose 2 cards from 6 hole cards: C(6,2) = 15
/// Used for PLO6 (6-card Pot-Limit Omaha)
pub const HOLE_COMBOS_2_FROM_6: [[usize; 2]; 15] = [
    [0, 1],
    [0, 2],
    [0, 3],
    [0, 4],
    [0, 5],
    [1, 2],
    [1, 3],
    [1, 4],
    [1, 5],
    [2, 3],
    [2, 4],
    [2, 5],
    [3, 4],
    [3, 5],
    [4, 5],
];

/// All ways to choose 3 cards from 5 board cards: C(5,3) = 10
pub const BOARD_COMBOS_3_FROM_5: [[usize; 3]; 10] = [
    [0, 1, 2],
    [0, 1, 3],
    [0, 1, 4],
    [0, 2, 3],
    [0, 2, 4],
    [0, 3, 4],
    [1, 2, 3],
    [1, 2, 4],
    [1, 3, 4],
    [2, 3, 4],
];