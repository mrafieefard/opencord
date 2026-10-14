//! No way to lower other apps on this system.

pub const SUPPORTED: bool = false;

pub struct Attenuator;

impl Attenuator {
    pub fn start() -> Option<Self> {
        None
    }

    pub fn set(&self, _level: Option<f32>) {}
}
