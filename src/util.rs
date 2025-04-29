mod args;
pub use args::Args;
pub mod handler;
pub mod cleaner;

pub struct AppState {
    pub args: Args,
}