use anyhow::Context;
use im::HashMap;
use nanorand::Rng;
use std::time::Duration;
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
        let target_id = user_times_diff
            .iter()
            .filter(|(id, _)| {
                id.as_str() != "platform:farcaster" && id.as_str() != config.user_id.as_str()
            })
            .max_by_key(|(_, &time)| time)
            .map(|(id, _)| id)
            .context("there should always be someone in first")?;

        let target_time = stats
            .user_times
            .get(target_id)
            .copied()
            .context("no target_time")?;

        let holder_id = &stats.flag.holder_id;
        let holder_time = stats.user_times.get(holder_id).copied().unwrap_or(0);

        let our_time = stats.user_times.get(&config.user_id).copied().unwrap_or(0);

        let mut rng = nanorand::tls_rng();

        if target_id == holder_id {
            info!(%target_id, %target_time, %our_time, "fire!");

            let wait_ms = rng.generate_range(0..=1_000);

            sleep_with_cancel(cancellation_token, Duration::from_millis(wait_ms)).await;

            Ok(true)
        } else {
            debug!(%target_id, %target_time, %holder_id, %holder_time, %our_time, "waiting to fire the shell");

            // TODO: look at the stats to see the next time that they are able to yoink. wait until then. if they don't yoink within some time after that, yoink anyways
            let wait_ms = rng.generate_range(500..=2_000);

            sleep_with_cancel(cancellation_token, Duration::from_millis(wait_ms)).await;

            Ok(false)
        }
    }
}
