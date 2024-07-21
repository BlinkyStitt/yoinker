use std::time::Duration;

use anyhow::Context;
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

        // TODO: stats only changes every 30 minutes! we need to get the current holder another way
        let holder_id = &stats.flag.holder_id;

        info!(%first_place_id, %holder_id, "blue shell strategy");

        if first_place_id == holder_id {
            // TODO: some delay here?
            Ok(true)
        } else {
            sleep_with_cancel(cancellation_token, Duration::from_secs(1)).await;

            Ok(false)
        }
    }
}
