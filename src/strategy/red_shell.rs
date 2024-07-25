use im::HashMap;
use nanorand::Rng;
use std::{cmp::Reverse, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::{sleep_with_cancel, yoinker::Stats, Config};

use super::YoinkStrategy;

/// target the recent top 3 yoinkers.
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

        #[allow(dead_code)]
        #[derive(Debug)]
        struct Target<'a> {
            id: &'a str,
            time_diff: u64,
            time: u64,
        }

        // TODO: only take 3 if user_times_diff is empty? take 1 otherwise?
        let targets = targets
            .iter()
            .take(3)
            .map(|(id, x)| Target {
                id: id.as_str(),
                time_diff: **x,
                time: stats
                    .user_times
                    .get(id.as_str())
                    .copied()
                    .unwrap_or_default(),
            })
            .collect::<Vec<_>>();

        let holder_id = &stats.flag.holder_id;

        let holder_time = stats.user_times.get(holder_id).copied().unwrap_or(0);
        let holder_diff = user_times_diff.get(holder_id).copied().unwrap_or(0);

        let our_time = stats.user_times.get(&config.user_id).copied().unwrap_or(0);
        let our_diff = user_times_diff.get(&config.user_id).copied().unwrap_or(0);

        let mut rng = nanorand::tls_rng();

        if targets.iter().any(|t| t.id == holder_id) {
            let wait_ms = rng.generate_range(1_000..=3_000);

            info!(
                holder_id,
                holder_diff,
                holder_time,
                our_diff,
                our_time,
                wait_ms,
                ?targets,
                "fire!"
            );

            sleep_with_cancel(cancellation_token, Duration::from_millis(wait_ms)).await;

            Ok(true)
        } else {
            debug!(
                holder_id,
                holder_diff,
                holder_time,
                our_diff,
                our_time,
                ?targets,
                "waiting to fire the shell"
            );

            // TODO: look at the stats to see the next time that they are able to yoink. wait until then. if they don't yoink within some time after that, yoink anyways

            sleep_with_cancel(cancellation_token, Duration::from_secs(1)).await;

            Ok(false)
        }
    }
}
