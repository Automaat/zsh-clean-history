pub mod allowlist;
pub mod clean;
pub mod cleaner;
pub mod exits;
pub mod history;
pub mod log;
pub mod paths;
pub(crate) mod secrets;
pub mod settings;
pub mod similarity;

pub use clean::{CleanReport, LockedHistory, run_cleanup};
pub use cleaner::{Removal, identify_removals};
pub use exits::{compact_exits_file, load_exit_codes};
pub use history::{HistoryEntry, ParsedHistory, parse_history_file, parse_history_text};
pub use log::{DEFAULT_LOG_MAX_BYTES, write_log_entry};
pub use paths::Paths;
pub use settings::CleaningSettings;
