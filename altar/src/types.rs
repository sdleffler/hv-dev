pub type Float = f32;

#[derive(Debug, Clone, Copy)]
pub struct Dt(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct RemainingDt(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct Tick(pub u64);
