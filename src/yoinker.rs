use crate::{
    sleep_with_cancel,
    strategy::{self, YoinkStrategy},
    Config,
};
use anyhow::Context;
use nanorand::Rng;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsFlag {
    // pub yoinked_at: u64,
    pub holder_id: String,
    // pub holder_name: String,
    // pub holder_platform: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub flag: StatsFlag,
    // pub yoinks: u64,
    // pub user_yoinks: HashMap<String, u64>,
    pub user_times: HashMap<String, u64>,
    // pub users: BiMap<String, String>,
}

pub async fn main_loop(
    cancellation_token: CancellationToken,
    client: &Client,
    config: &Config,
) -> anyhow::Result<()> {
    while !cancellation_token.is_cancelled() {
        if let Err(err) = main(&cancellation_token, client, config).await {
            warn!(?err, "yoinker main failed");
            sleep(Duration::from_secs(1)).await;
        };
    }

    Ok(())
}

pub async fn main(
    cancellation_token: &CancellationToken,
    client: &Client,
    config: &Config,
) -> anyhow::Result<()> {
    let stats = current_stats(client)
        .await
        .context("getting current stats")?;

    if stats.flag.holder_id == config.user_id {
        // we already have the flag. no need to do anything. don't waste our cooldown timer!
        debug!("we have the flag");

        let mut rng = nanorand::tls_rng();

        // we don't have to sleep here, but i think it makes sense to. we don't want to DOS the current flag holder check. need a websocket!
        let x = rng.generate_range(1_000..5_000);
        sleep_with_cancel(cancellation_token, Duration::from_millis(x)).await;

        return Ok(());
    }

    // TODO: randomly pick a strategy to use. change on a randomized interval. where should the chosen strategy be stored?
    let active_strategy = strategy::BlueShellStrategy;

    if active_strategy
        .should_yoink(cancellation_token, config, &stats)
        .await?
    {
        yoink_flag(cancellation_token, client, config).await?;
    }

    Ok(())
}

pub async fn current_stats(client: &Client) -> anyhow::Result<Stats> {
    let stats = client
        .get("https://yoink.terminally.online/api/stats")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(stats)
}

pub async fn yoink_flag(
    cancellation_token: &CancellationToken,
    client: &Client,
    config: &Config,
) -> anyhow::Result<()> {
    // TODO: re-use this. its the same payload every time
    let payload = json!({
        "action": {
            "button": {
            "index": 1
            },
            "frames_url": "https://yoink.terminally.online/",
            "post_url": "https://yoink.terminally.online/api/yoink"
        },
        "cast_hash": config.cast_hash,
        "signer_uuid": config.nn_signer_uuid
    });

    let response = client
        .post("https://api.neynar.com/v2/farcaster/frame/action")
        .header("api_key", config.nn_api_key.as_str())
        .json(&payload)
        .send()
        .await?;

    let response = response.text().await?;

    // TODO: inspect the response. the image might be <https://yoink.terminally.online/api/images/ratelimit?date=1721150535550>

    info!("yoinked: {:#?}", response);

    // we've yoinked. the current cooldown timer is 60 seconds. no point in returning before then
    sleep_with_cancel(cancellation_token, Duration::from_secs(60)).await;

    Ok(())
}
