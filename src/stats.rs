use crate::{Config, State};
use anyhow::Context;
use im::HashMap;
use moka::future::Cache;
use reqwest::Client;
use serde::Deserialize;
use std::{fmt::Debug, sync::Arc};
use tokio::{
    sync::mpsc,
    time::{sleep, Duration},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, trace, warn};

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

/// Update the stats.
pub async fn stats_loop<const N: usize>(
    app_state_tx: mpsc::UnboundedSender<State<N>>,
    cancellation_token: CancellationToken,
    client: Client,
    config: Config,
) -> anyhow::Result<()> {
    // stats update every 30 minutes, but we don't know where in the refresh window we are. so only cache for 5 minutes
    // TODO: Duration from config
    let stats_cache: Cache<(), Stats> = Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_secs(5 * 60))
        .build();

    let mut app_state = State::<N>::default();

    while !cancellation_token.is_cancelled() {
        if let Err(err) = stats_to_state(&mut app_state, &app_state_tx, &client, &stats_cache).await
        {
            warn!(?err, "stats_to_state failure");
            sleep(Duration::from_millis(500)).await;
        };
    }

    Ok(())
}

pub async fn stats_to_state<const N: usize>(
    app_state: &mut State<N>,
    app_state_tx: &mpsc::UnboundedSender<State<N>>,
    client: &Client,
    stats_cache: &Cache<(), Stats>,
) -> anyhow::Result<()> {
    let stats: Stats = fetch_stats(stats_cache, client)
        .await
        .context("getting current stats")?;

    let stats = Arc::new(stats);

    let changed = app_state.push_stats(stats.clone());

    if changed {
        let app_state = app_state.clone();

        debug!(?stats.flag, "updated");

        app_state_tx.send(app_state)?;
    } else {
        trace!(?stats.flag, "not changed");
    }

    Ok(())
}

/// The current state of the game (with some caching). Parts of this only update every 30 minutes.
pub async fn fetch_stats(cache: &Cache<(), Stats>, client: &Client) -> anyhow::Result<Stats> {
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
