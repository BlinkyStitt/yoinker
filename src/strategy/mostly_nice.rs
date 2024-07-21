use super::YoinkStrategy;
use crate::{sleep_with_cancel, yoinker::Stats, Config};
use nanorand::Rng;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

pub struct MostlyNiceStrategy;

impl YoinkStrategy for MostlyNiceStrategy {
    async fn should_yoink(
        &self,
        cancellation_token: &CancellationToken,
        config: &Config,
        stats: &Stats,
    ) -> anyhow::Result<bool> {
        // TODO: if we don't have the flag, but the person who has the flag has a lower score than us, leave them alone. we don't want to be jerks
        // TODO: move this to a "should_not_yoink" function

        let mut rng = nanorand::tls_rng();

        // TODO: stats only update every 30 minutes! we need to get the current holder a different way!
        let holder_time = stats
            .user_times
            .get(&stats.flag.holder_id)
            .copied()
            .unwrap_or(0);

        let my_time = stats.user_times.get(&config.user_id).copied().unwrap_or(0);

        // the stats only refresh every 30 minutes
        // TODO: get stats from our own hub
        // let jerk_threshold = my_time.saturating_sub(30 * 60);
        let jerk_threshold = 6 * 3600;
        let nice_chance: u16 = 75; // TODO: dynamic nice_chance based on their time

        if holder_time < jerk_threshold {
            // the current flag holder has a lower score than us. don't be a jerk
            if holder_time < my_time {
                let x = rng.generate_range(0..100);
                if x <= nice_chance {
                    debug!(
                        holder_id = stats.flag.holder_id.as_str(),
                        "flag holder has too low of a score. not yoinking"
                    );

                    let x = rng.generate_range(1_000..10_000);
                    sleep_with_cancel(cancellation_token, Duration::from_millis(x)).await;

                    return Ok(false);
                }

                info!("being a jerk");
            }
        }

        // we do NOT have the flag. try to yoink it
        let wait_ms = rng.generate_range(0..=3_000);

        info!(my_time, holder_time, wait_ms, "preparing to yoink the flag");

        // TODO: is this sleep a good idea? it wastes some of our cooldown timer, but i feel like giving other bots some time to play is a good idea
        sleep_with_cancel(cancellation_token, Duration::from_millis(wait_ms)).await;

        Ok(true)
    }
}
