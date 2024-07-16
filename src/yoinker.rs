use std::collections::HashMap;

use anyhow::Context;
use bimap::BiMap;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tokio::{
    select,
    time::{sleep, Duration},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::Config;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsFlag {
    /// TODO: this is "yoinkedAt" in the json. do we need to tell serde that?
    yoinked_at: u64,
    holder_id: String,
    holder_name: String,
    holder_platform: String,
}

#[derive(Deserialize)]
pub struct Stats {
    flag: StatsFlag,
    // yoinks: u64,
    // user_yoinks: HashMap<String, u64>,
    // user_times: HashMap<String, u64>,
    // users: BiMap<String, String>,
}

pub async fn main_loop(
    cancellation_token: CancellationToken,
    client: &Client,
    config: &Config,
) -> anyhow::Result<()> {
    while !cancellation_token.is_cancelled() {
        if let Err(err) = main(&cancellation_token, &client, config).await {
            warn!(?err, "yoinker main failed");
            sleep(Duration::from_secs(2)).await;
        };
    }

    Ok(())
}

pub async fn sleep_with_cancel(cancellation_token: &CancellationToken, duration: Duration) {
    select! {
        _ = cancellation_token.cancelled() => {},
        _ = sleep(duration) => {}
    };
}

pub async fn main(
    cancellation_token: &CancellationToken,
    client: &Client,
    config: &Config,
) -> anyhow::Result<()> {
    let stats = current_stats(client)
        .await
        .context("getting current stats")?;

    if stats.flag.holder_name == config.user_id {
        // we already have the flag. no need to do anything. don't waste our cooldown timer!
        debug!("we have the flag");

        // we don't have to sleep here, but i think it makes sense to. we don't want to DOS the current flag holder check. need a websocket!
        sleep_with_cancel(cancellation_token, Duration::from_secs(2)).await;

        return Ok(());
    }

    // TODO: if we don't have the flag, but the person who has the flag has a lower score than us, leave them alone. we don't want to be jerks

    // we do NOT have the flag. try to yoink it
    // TODO: smarter strategy here. delay up to 5 seconds if other people are yoinking. maybe learn the fids of the other bots?

    yoink_flag(client, config).await.context("yoinking")?;

    // we've yoinked. the current cooldown timer is 60 seconds. no point in returning before then
    // TODO: count how many times the flag changes?
    sleep_with_cancel(cancellation_token, Duration::from_secs(60)).await;

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

pub async fn yoink_flag(client: &Client, config: &Config) -> anyhow::Result<()> {
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

    Ok(())
}
