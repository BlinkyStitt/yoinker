use crate::{
    sleep_with_cancel,
    strategy::{self, YoinkStrategy},
    Config,
};
use anyhow::Context;
use im::HashMap;
use moka::future::Cache;
use nanorand::Rng;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::fmt::Debug;
use std::hash::Hash;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use url::Url;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatsFlag {
    // TODO: `yoinked_at` is a String in one place but a u64 in another. need a custom deserializer
    // pub yoinked_at: String,
    pub holder_id: String,
    pub holder_name: String,
    pub holder_platform: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub flag: StatsFlag,
    // pub yoinks: u64,
    // pub user_yoinks: HashMap<String, u64>,
    pub user_times: HashMap<String, u64>,
    pub users: HashMap<String, String>,
}

pub async fn main_loop(
    cancellation_token: CancellationToken,
    client: &Client,
    config: &Config,
) -> anyhow::Result<()> {
    let stats_cache: Cache<(), Stats> = Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_secs(30 * 60))
        .build();

    let mut old_stats = None;

    while !cancellation_token.is_cancelled() {
        if let Err(err) = main(
            &cancellation_token,
            client,
            config,
            &mut old_stats,
            &stats_cache,
        )
        .await
        {
            warn!(?err, "yoinker main failed");
            sleep(Duration::from_secs(1)).await;
        };
    }

    Ok(())
}

fn subtract_hashmaps<K: Clone + Debug + Hash + PartialEq + Eq>(
    newer: &HashMap<K, u64>,
    older: &HashMap<K, u64>,
) -> HashMap<K, u64> {
    let mut result = newer.clone();

    for (key, older_value) in older {
        if let Some(new_value) = result.get_mut(key) {
            *new_value -= older_value;
        } else {
            warn!(?key, "missing key!")
        }
    }

    result.retain(|_, &v| v > 0);

    result
}

pub async fn main(
    cancellation_token: &CancellationToken,
    client: &Client,
    config: &Config,
    old_stats: &mut Option<(Stats, HashMap<String, u64>)>,
    stats_cache: &Cache<(), Stats>,
) -> anyhow::Result<()> {
    let mut rng = nanorand::tls_rng();

    let stats = current_stats(stats_cache, client)
        .await
        .context("getting current stats")?;

    if let Some((old_stats, new_diff)) = old_stats {
        if old_stats.user_times != stats.user_times {
            // the stats have changed. update the old stats
            *new_diff = subtract_hashmaps(&stats.user_times, &old_stats.user_times);

            *old_stats = stats.clone();

            info!(?new_diff, "user stats changed");
        }
    } else {
        // this is the first time we've gotten stats. set the old stats
        info!("first time getting stats. targeting first place yoinker");

        *old_stats = Some((stats.clone(), stats.user_times.clone()));
    }

    let user_times_diff = &old_stats.as_ref().unwrap().1;

    if stats.flag.holder_id == config.user_id {
        // we already have the flag. no need to do anything. don't waste our cooldown timer!
        debug!("we have the flag");

        // we don't have to sleep here, but i think it makes sense to. we don't want to DOS the current flag holder check. need a websocket!
        let x = rng.generate_range(1_000..5_000);
        sleep_with_cancel(cancellation_token, Duration::from_millis(x)).await;

        return Ok(());
    }

    // TODO: randomly pick a strategy to use. change on a randomized interval. where should the chosen strategy be stored?
    let active_strategy = strategy::RedShellStrategy;

    if active_strategy
        .should_yoink(cancellation_token, config, &stats, user_times_diff)
        .await?
    {
        yoink_flag(cancellation_token, client, config).await?;
    } else {
        debug!("not yoinking this time");
    }

    Ok(())
}

pub async fn current_stats(cache: &Cache<(), Stats>, client: &Client) -> anyhow::Result<Stats> {
    let client = client.clone();

    // TODO: put stats in a cache. they only refresh every 30 minutes. we set our timer to 10 minutes though just so that if things don't line up we aren't too stale
    let mut stats = cache
        .try_get_with(
            (),
            client
                .get("https://yoink.terminally.online/api/stats")
                .send()
                .await?
                .error_for_status()?
                .json::<Stats>(),
        )
        .await
        .with_context(|| "failed fetching stats")?;

    // get the current flag holder
    let flag = client
        .get("https://yoink.terminally.online/api/flag")
        .send()
        .await?
        .json::<StatsFlag>()
        .await?;

    // override the stats' old info with the current info
    stats.flag = flag;

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

    #[allow(dead_code)]
    #[derive(Debug, serde::Deserialize)]
    struct Response {
        version: String,
        title: String,
        image: Url,
        buttons: Vec<serde_json::Value>,
        input: serde_json::Value,
        state: serde_json::Value,
        frames_url: Url,
    }

    let response = response.json::<Response>().await?;

    // TODO: inspect the response. the image might be <https://yoink.terminally.online/api/images/ratelimit?date=1721150535550>

    info!(?response, "yoinked");

    // we've yoinked. the current cooldown timer is 60 seconds. no point in returning before then
    sleep_with_cancel(cancellation_token, Duration::from_secs(60)).await;

    Ok(())
}
