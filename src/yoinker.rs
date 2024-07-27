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
use std::{fmt::Debug, ops::SubAssign};
use std::{hash::Hash, time::Instant};
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};
use url::Url;

/// TODO: this might change! we should get this from stats or something like that. maybe just wait 1 second and then let a rate limit on the next run give us the exact timing
const COOLDOWN_TIME: Duration = Duration::from_secs(10 * 60);

/// Information about the current flag holder.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatsFlag {
    // TODO: `yoinked_at` is a String in one place but a u64 in another. need a custom deserializer
    // pub yoinked_at: String,
    pub holder_id: String,
    pub holder_name: String,
    pub holder_platform: String,
}

/// Information about the current state of the game. Updated every 30 minutes.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub flag: StatsFlag,
    // pub yoinks: u64,
    // pub user_yoinks: HashMap<String, u64>,
    pub user_times: HashMap<String, u64>,
    pub users: HashMap<String, String>,
}

/// Rruns the yoink bot until cancelled.
pub async fn main_loop(
    cancellation_token: CancellationToken,
    client: &Client,
    config: &Config,
) -> anyhow::Result<()> {
    // stats update every 30 minutes, but we don't know where in the refresh window we are. so only cache for 5 minutes
    let stats_cache: Cache<(), Stats> = Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_secs(5 * 60))
        .build();

    let mut old_stats = None;
    let mut impatient_fire = Instant::now() + COOLDOWN_TIME / 2;

    while !cancellation_token.is_cancelled() {
        if let Err(err) = main(
            &cancellation_token,
            client,
            config,
            &mut impatient_fire,
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

/// helper function to subtract two hashmaps and return the diference.
fn subtract_hashmaps<K, V>(newer: &HashMap<K, V>, older: &HashMap<K, V>) -> HashMap<K, V>
where
    K: Clone + Debug + Hash + PartialEq + Eq,
    V: Copy + Clone + SubAssign + PartialOrd<u64>,
{
    let mut result = newer.clone();

    for (key, older_value) in older {
        if let Some(new_value) = result.get_mut(key) {
            *new_value -= *older_value;
        } else {
            warn!(?key, "missing key!")
        }
    }

    result.retain(|_, &v| v > 0);

    result
}

/// The main logic for the yoink bot.
pub async fn main(
    cancellation_token: &CancellationToken,
    client: &Client,
    config: &Config,
    impatient_fire: &mut Instant,
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
        info!("first time getting stats");

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

    // TODO: pass next_fire to this function so that it can alter its strategy based on how long we've been waiting
    if Instant::now() > *impatient_fire {
        warn!("its been too long! I must yoink!");
        yoink_flag(cancellation_token, client, config).await?;

        // TODO: save earliest fire time here? we've already slept for the cooldown time so it should be ready. but i'm seeing rate limits

        *impatient_fire = Instant::now() + COOLDOWN_TIME;
    } else if active_strategy
        .should_yoink(cancellation_token, config, &stats, user_times_diff)
        .await?
    {
        yoink_flag(cancellation_token, client, config).await?;

        // TODO: save earliest fire time here? we've already slept for the cooldown time so it should be ready. but i'm seeing rate limits

        *impatient_fire = Instant::now() + COOLDOWN_TIME;
    } else {
        // TODO: include next_fire in a human readable format
        trace!("not yoinking this time");
    }

    Ok(())
}

/// The current state of the game (with some caching). Parts of this only update every 30 minutes.
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

/// Use [Neynar's](https://neynar.com/) API to yoink the flag.
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
        version: Option<String>,
        title: Option<String>,
        image: Url,
        // buttons: Vec<serde_json::Value>,
        // input: serde_json::Value,
        // state: serde_json::Value,
        // frames_url: String,
    }

    let response = response.json::<Response>().await?;

    // inspect the response. the image might be <https://yoink.terminally.online/api/images/ratelimit?date=1721150535550>
    let duration = if response.image.path() == "/api/images/ratelimit" {
        if let Some((_, until)) = response.image.query_pairs().find(|(k, _)| k == "date") {
            let now_ms = chrono::Utc::now().timestamp_millis();

            let last_yoinked_ms = until.parse::<i64>().context("parsing rate limit date")?;

            // TODO: get the rate limit time from stats or similar
            let until_ms = last_yoinked_ms + COOLDOWN_TIME.as_millis() as i64;

            if until_ms > now_ms {
                let duration_ms = (until_ms - now_ms) as u64;

                warn!(duration_ms, "we've been rate limited");

                Duration::from_millis(duration_ms)
            } else {
                anyhow::bail!(
                    "rate limit date is in the past. {} should be > {}",
                    until_ms,
                    now_ms
                );
            }
        } else {
            warn!(?response, "failed to read rate limit query data");
            COOLDOWN_TIME
        }
    } else {
        // we've yoinked. the current cooldown timer is 60 seconds. no point in returning before then
        info!(?response, "yoinked!");

        COOLDOWN_TIME
    };

    sleep_with_cancel(cancellation_token, duration).await;

    Ok(())
}
