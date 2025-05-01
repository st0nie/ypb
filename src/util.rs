mod args;
pub use args::Args;
pub mod cleaner;
pub mod handler;

pub struct AppState {
    pub args: Args,
}
