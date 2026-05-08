use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CleaningSettings {
    pub similarity_threshold: f64,
    pub rare_threshold: usize,
    pub remove_rare: bool,
}

impl Default for CleaningSettings {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.8,
            rare_threshold: 3,
            remove_rare: false,
        }
    }
}
