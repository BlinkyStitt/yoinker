use std::time::Duration;

use anyhow::Context;
use nanorand::Rng;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{sleep_with_cancel, yoinker::Stats, Config};

use super::YoinkStrategy;

pub struct BlueShellStrategy;

impl YoinkStrategy for BlueShellStrategy {
    async fn should_yoink(
        &self,
        cancellation_token: &CancellationToken,
        _config: &Config,
        stats: &Stats,
    ) -> anyhow::Result<bool> {
        let first_place_id = stats
            .user_times
            .iter()
            .filter(|(id, _)| id.as_str() != "platform:farcaster")
            .max_by_key(|(_, &time)| time)
            .map(|(id, _)| id)
            .context("there should always be someone in first")?;

        let holder_id = &stats.flag.holder_id;

        let mut rng = nanorand::tls_rng();

        if first_place_id == holder_id {
            info!(%first_place_id, %holder_id, "blue shell!");

            let wait_ms = rng.generate_range(0..=1_000);

            sleep_with_cancel(cancellation_token, Duration::from_millis(wait_ms)).await;

            Ok(true)
        } else {
            info!(%first_place_id, %holder_id, "waiting to fire the blue shell");

            // TODO: look at the stats to see the next time that they are able to yoink. wait until then. if they don't yoink within some time after that, yoink anyways
            let wait_ms = rng.generate_range(500..=2_000);

            sleep_with_cancel(cancellation_token, Duration::from_millis(wait_ms)).await;

            Ok(false)
        }
    }
}
