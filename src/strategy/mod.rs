mod blue_shell;
mod mostly_nice;

pub use blue_shell::BlueShellStrategy;
pub use mostly_nice::MostlyNiceStrategy;

use crate::{yoinker::Stats, Config};
use tokio_util::sync::CancellationToken;

pub trait YoinkStrategy {
    async fn should_yoink(
        &self,
        cancellation_token: &CancellationToken,
        config: &Config,
        stats: &Stats,
    ) -> anyhow::Result<bool>;
}
