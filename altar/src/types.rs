pub type Float = f32;

#[derive(Debug, Clone, Copy)]
pub struct GlobalTick(pub u64);

#[derive(Debug, Clone, Copy)]
pub struct GlobalDt(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct UpdateDt(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct RemainingUpdateDt(pub f32);

#[derive(Debug, Clone, Copy)]
pub struct UpdateTick(pub u64);
