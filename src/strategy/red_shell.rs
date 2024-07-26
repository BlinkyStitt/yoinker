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
        // TODO: if we haven't yoinked in 2 minutes, yoink

        let mut targets = user_times_diff
            .iter()
            .filter(|(id, _)| {
                id.as_str() != "platform:farcaster" && id.as_str() != config.user_id.as_str()
            })
            .collect::<Vec<_>>();

        targets.sort_by_key(|(_, &time)| Reverse(time));

        let num_targets = if let Some((first_id, first_time)) = targets.first() {
            if Some(first_time) == stats.user_times.get(*first_id).as_ref() {
                // we haven't had enough time to actually calculate a diff
                // TODO: is this the best way to check this? I'm not sure. maybe checking if its > 30 minutes is better
                3
            } else {
                // we have actual diffs. target the top moving player
                // TODO: shoot the top 2 instead? I think only if they are close in time. if the first is really far ahead, stay on the first
                1
            }
        } else {
            anyhow::bail!("there should always be someone in first");
        };
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
            .take(num_targets)
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
