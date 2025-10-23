#[derive(Clone, Copy)]
pub struct ComboInfo {
    pub p: i32,
    pub idx: u16,
    pub self_weight: f32,
    pub vs_weight: f32,
    pub combo: [u8; 2],
}
