use crate::{
    sleep::{long_jitter, short_jitter, sleep_with_cancel},
    strategy::{self, YoinkStrategy},
    Config, State, COOLDOWN_TIME,
};
use anyhow::Context;
use reqwest::Client;
use serde_json::json;
use std::fmt::Debug;
use std::time::Instant;
use tokio::{
    select,
    sync::mpsc,
    time::{timeout, Duration},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};
use url::Url;

/// Runs the yoink bot until cancelled.
pub async fn main_loop<const N: usize>(
    mut app_state_rx: mpsc::UnboundedReceiver<State<N>>,
    cancellation_token: CancellationToken,
    client: &Client,
    config: &Config,
) -> anyhow::Result<()> {
    // TODO: store this in redis so that we can recover from a restart
    let mut impatient_fire = Instant::now() + COOLDOWN_TIME + COOLDOWN_TIME;

    loop {
        let state = select! {
            // TODO: what should this timeout be?
            x = timeout(Duration::from_secs(3), app_state_rx.recv()) => {
                match x {
                    Ok(Some(state)) => state,
                    Ok(None) => {
                        break;
                    }
                    Err(err) => {
                        warn!(?err, "app_state_rx timeout");
                        Default::default()
                    }
                }
            }
            _ = cancellation_token.cancelled() => break,
        };

        if let Err(err) = main(
            state,
            &cancellation_token,
            client,
            config,
            &mut impatient_fire,
        )
        .await
        {
            warn!(?err, "yoinker main failed");
        };
    }

    Ok(())
}

/// The main logic for the yoink bot.
/// TODO: instead of watching app_state_rx, maybe this should watch a channel that is updated by strategies? then blue shell and impatient can both be strategies?
pub async fn main<const N: usize>(
    state: State<N>,
    cancellation_token: &CancellationToken,
    client: &Client,
    config: &Config,
    impatient_fire: &mut Instant,
) -> anyhow::Result<()> {
    if let Some(stats) = state.stats.back() {
        let user_times_diff = &state.diff;

        if stats.flag.holder_id == config.user_id {
            // we already have the flag. no need to do anything. don't waste our cooldown timer!
            debug!("we have the flag");
            return Ok(());
        }

        // TODO: randomly pick a strategy to use. change on a randomized interval. where should the chosen strategy be stored?
        // TODO: if no stats, just loop over COOLDOWN_TIME
        let active_strategy = strategy::RedShellStrategy;

        // TODO: pass next_fire to this function so that it can alter its strategy based on how long we've been waiting
        if Instant::now() > *impatient_fire {
            warn!("its been too long! I must yoink!");
            if yoink_flag_and_sleep(cancellation_token, client, config).await? {
                // TODO: think about this more
                *impatient_fire = Instant::now() + long_jitter();
            } else {
                // yoinking failed. we got rate limited somehow. just retry soon
                *impatient_fire = Instant::now() + short_jitter();
            }
        } else if active_strategy
            .should_yoink(cancellation_token, config, stats, user_times_diff)
            .await?
        {
            yoink_flag_and_sleep(cancellation_token, client, config).await?;

            // TODO: save earliest fire time here? we've already slept for the cooldown time so it should be ready. but i'm seeing rate limits

            *impatient_fire = Instant::now() + long_jitter();
        } else {
            // TODO: include next_fire in a human readable format
            trace!("not yoinking this time");
        }
    } else {
        info!("i have no ~~~mouth~~~ stats and i must ~~~scream~~~ yoink");
        yoink_flag_and_sleep(cancellation_token, client, config).await?;
    }
    Ok(())
}

/// Use [Neynar's](https://neynar.com/) API to yoink the flag. Then sleep for the cooldown period.
///
/// TODO: maybe instead of returning a bool, we clear an AtomicBool?
pub async fn yoink_flag_and_sleep(
    cancellation_token: &CancellationToken,
    client: &Client,
    config: &Config,
) -> anyhow::Result<bool> {
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
    let (yoinked, duration) = if response.image.path() == "/api/images/ratelimit" {
        if let Some((_, until)) = response.image.query_pairs().find(|(k, _)| k == "date") {
            let now_ms = chrono::Utc::now().timestamp_millis();

            // TODO: i don't think this is right. i think the ratelimit is always just giving us the current time
            // TODO: we also want to mark that we still need to yoink! we need some way to remember to try again
            let last_yoinked_ms = until.parse::<i64>().context("parsing rate limit date")?;

            // TODO: get the rate limit time from stats or similar
            let until_ms = last_yoinked_ms + COOLDOWN_TIME.as_millis() as i64;

            if until_ms > now_ms {
                let duration_ms = (until_ms - now_ms) as u64;

                warn!(duration_ms, "we've been rate limited");

                (false, Duration::from_millis(duration_ms))
            } else {
                anyhow::bail!(
                    "rate limit date is in the past. {} should be > {}",
                    until_ms,
                    now_ms
                );
            }
        } else {
            warn!(?response, "failed to read rate limit query data");
            (false, COOLDOWN_TIME)
        }
    } else {
        // we've yoinked. the current cooldown timer is 60 seconds. no point in returning before then
        info!(?response, "yoinked!");

        (true, COOLDOWN_TIME)
    };

    sleep_with_cancel(cancellation_token, duration).await;

    Ok(yoinked)
}
