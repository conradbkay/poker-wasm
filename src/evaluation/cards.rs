// Card representation constants
pub const SUITS: &str = "cdhs";
pub const RANKS: &str = "23456789TJQKA";

// Lookup table from hand index to combo
pub static IDX2HAND: [[u8; 2]; 1326] = {
    let mut hands = [[0u8; 2]; 1326];
    let mut idx = 0;
    let mut i = 0;
    while i < 52 {
        let mut j = 0;
        while j < i {
            hands[idx] = [j, i];
            idx += 1;
            j += 1;
        }
        i += 1;
    }
    hands
};

// Card string formatting functions
pub fn card_to_string(card: u8) -> String {
    if card >= 52 {
        return "??".to_string();
    }

    let rank_idx = (card / 4) as usize;
    let suit_idx = (card % 4) as usize;

    let rank_char = RANKS.chars().nth(rank_idx).unwrap_or('?');
    let suit_char = SUITS.chars().nth(suit_idx).unwrap_or('?');

    format!("{rank_char}{suit_char}")
}

pub fn string_to_card(card_str: &str) -> Option<u8> {
    if card_str.len() != 2 {
        return None;
    }

    let chars: Vec<char> = card_str.chars().collect();
    let rank_char = chars[0];
    let suit_char = chars[1];

    let rank_idx = RANKS.find(rank_char)?;
    let suit_idx = SUITS.find(suit_char)?;

    Some((rank_idx * 4 + suit_idx) as u8)
}

pub fn hand_to_string(hand: &[u8]) -> String {
    if hand.len() < 2 {
        return "??".to_string();
    }
    format!("{}{}", card_to_string(hand[0]), card_to_string(hand[1]))
}

pub fn cards_to_mask(cards: &[u8]) -> u64 {
    let mut mask = 0u64;
    for &card in cards {
        mask |= 1u64 << card;
    }
    mask
}

pub fn board_to_mask(board: &[u8]) -> u64 {
    cards_to_mask(board)
}
