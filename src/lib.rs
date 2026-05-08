pub mod cleaner;
pub mod exits;
pub mod history;
pub mod log;
pub mod paths;
pub mod settings;
pub mod similarity;

pub use cleaner::{Removal, identify_removals};
pub use exits::{compact_exits_file, load_exit_codes};
pub use history::{HistoryEntry, ParsedHistory, parse_history_file, parse_history_text};
pub use log::write_log_entry;
pub use paths::Paths;
pub use settings::CleaningSettings;
