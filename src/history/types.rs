use serde::{Deserialize, Serialize};
use crate::cli::app_config::Cli;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub command: String,
    #[serde(skip)]
    pub cli: Option<Cli>,
}
