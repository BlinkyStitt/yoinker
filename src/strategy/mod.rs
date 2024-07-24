mod blue_shell;
mod mostly_nice;
mod red_shell;

#[allow(unused_imports)]
pub use blue_shell::BlueShellStrategy;

#[allow(unused_imports)]
pub use mostly_nice::MostlyNiceStrategy;

#[allow(unused_imports)]
pub use red_shell::RedShellStrategy;

use crate::{yoinker::Stats, Config};
use im::HashMap;
use tokio_util::sync::CancellationToken;

pub trait YoinkStrategy {
    async fn should_yoink(
        &self,
        cancellation_token: &CancellationToken,
        config: &Config,
        stats: &Stats,
        user_times_diff: &HashMap<String, u64>,
    ) -> anyhow::Result<bool>;
}
