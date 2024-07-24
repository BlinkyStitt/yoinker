use im::HashMap;
use nanorand::Rng;
use std::{cmp::Reverse, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::{sleep_with_cancel, yoinker::Stats, Config};

use super::YoinkStrategy;

/// target the recent top yoinker.
pub struct RedShellStrategy;

impl YoinkStrategy for RedShellStrategy {
    async fn should_yoink(
        &self,
        cancellation_token: &CancellationToken,
        config: &Config,
        stats: &Stats,
        user_times_diff: &HashMap<String, u64>,
    ) -> anyhow::Result<bool> {
        let mut targets = user_times_diff
            .iter()
            .filter(|(id, _)| {
                id.as_str() != "platform:farcaster" && id.as_str() != config.user_id.as_str()
            })
            .collect::<Vec<_>>();

        targets.sort_by_key(|(_, &time)| Reverse(time));

        let targets = targets
            .iter()
            .take(3)
            .map(|(id, x)| (id.as_str(), **x, stats.user_times.get(id.as_str()).copied()))
            .collect::<Vec<_>>();

        let holder_id = &stats.flag.holder_id;
        let holder_time = stats.user_times.get(holder_id).copied().unwrap_or(0);

        let our_time = stats.user_times.get(&config.user_id).copied().unwrap_or(0);

        let mut rng = nanorand::tls_rng();

        if targets.iter().any(|(id, _, _)| id == holder_id) {
            info!(%holder_id, %holder_time, %our_time, ?targets, "fire!");

            let wait_ms = rng.generate_range(0..=1_000);

            sleep_with_cancel(cancellation_token, Duration::from_millis(wait_ms)).await;

            Ok(true)
        } else {
            debug!(%holder_id, %holder_time, %our_time, ?targets, "waiting to fire the shell");

            // TODO: look at the stats to see the next time that they are able to yoink. wait until then. if they don't yoink within some time after that, yoink anyways
            let wait_ms = rng.generate_range(500..=2_000);

            sleep_with_cancel(cancellation_token, Duration::from_millis(wait_ms)).await;

            Ok(false)
        }
    }
}
