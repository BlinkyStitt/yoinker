mod blue_shell;
mod mostly_nice;
mod red_shell;

use crate::{stats::Stats, Config};
use im::HashMap;
use tokio_util::sync::CancellationToken;

pub use blue_shell::BlueShellStrategy;
pub use mostly_nice::MostlyNiceStrategy;
pub use red_shell::RedShellStrategy;

/// A strategy for playing the yoink game.
pub trait YoinkStrategy {
    /// Determine if we should yoink the flag.
    async fn should_yoink(
        &self,
        cancellation_token: &CancellationToken,
        config: &Config,
        stats: &Stats,
        user_times_diff: &HashMap<String, u64>,
    ) -> anyhow::Result<bool>;
}
