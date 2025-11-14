mod args;
use std::sync::Arc;

pub use args::Args;
pub mod cleaner;
pub mod handler;

#[derive(Debug, Clone)]
pub struct AppState {
    pub args: Arc<Args>,
}
