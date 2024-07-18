use crate::{sleep_with_cancel, Config};
use anyhow::Context;
use rand::Rng;
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
    // yoinked_at: u64,
    holder_id: String,
    // holder_name: String,
    // holder_platform: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    flag: StatsFlag,
    // yoinks: u64,
    // user_yoinks: HashMap<String, u64>,
    user_times: HashMap<String, u64>,
    // users: BiMap<String, String>,
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
    let mut rng = rand::thread_rng();

    let stats = current_stats(client)
        .await
        .context("getting current stats")?;

    if stats.flag.holder_id == config.user_id {
        // we already have the flag. no need to do anything. don't waste our cooldown timer!
        debug!("we have the flag");

        // we don't have to sleep here, but i think it makes sense to. we don't want to DOS the current flag holder check. need a websocket!
        let x = rng.gen_range(1_000..5_000);
        sleep_with_cancel(cancellation_token, Duration::from_millis(x)).await;

        return Ok(());
    }

    // TODO: if we don't have the flag, but the person who has the flag has a lower score than us, leave them alone. we don't want to be jerks
    // TODO: move this to a "should_not_yoink" function

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
    let nice_chance = 0.75; // TODO: dynamic nice_chance based on their time

    if holder_time < jerk_threshold {
        // the current flag holder has a lower score than us. don't be a jerk
        if holder_time < my_time {
            let x = rng.gen::<f32>();
            if x <= nice_chance {
                debug!(
                    holder_id = stats.flag.holder_id.as_str(),
                    "flag holder has too low of a score. not yoinking"
                );

                let x = rng.gen_range(1_000..10_000);
                sleep_with_cancel(cancellation_token, Duration::from_millis(x)).await;

                return Ok(());
            }

            info!("being a jerk");
        }
    }

    // we do NOT have the flag. try to yoink it
    info!(my_time, holder_time, "preparing to yoink the flag");

    // TODO: is this sleep a good idea? it wastes some of our cooldown timer, but i feel like giving other bots some time to play is a good idea
    let x = rng.gen_range(0..=3_000);
    sleep_with_cancel(cancellation_token, Duration::from_millis(x)).await;

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
