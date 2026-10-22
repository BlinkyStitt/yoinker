#[allow(async_fn_in_trait)]
mod sleep;
mod stats;
mod strategy;
mod utils;
mod yoinker;

use crate::stats::Stats;
use crate::utils::subtract_hashmaps;
use anyhow::Context;
use circular_buffer::CircularBuffer;
use imbl::HashMap;
use serde::Deserialize;
use std::{env, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use utils::{https_client, init_logging};

/// The application name and version.
pub static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

/// TODO: this might change! we should get this from stats or something like that
pub const COOLDOWN_TIME: Duration = Duration::from_secs(10 * 60);

/// application configuration
/// TODO: add Debug once this doesn't have secrets in it
#[derive(Clone, Deserialize)]
pub struct Config {
    /// TODO: get this based on the signer/api key?
    user_id: String,
    cast_hash: String,
    /// TODO: change this to `nn_api_key_file`
    nn_api_key: String,
    /// TODO: change this to `nn_signer_uuid_file`
    nn_signer_uuid: String,
}

#[derive(Clone, Debug, Default)]
pub struct State<const N: usize> {
    stats: CircularBuffer<N, Arc<Stats>>,
    diff: HashMap<String, u64>,
}

impl<const N: usize> State<N> {
    pub fn push_stats(&mut self, stats: Arc<Stats>) -> bool {
        let new_stats = if let Some(old_stats) = self.stats.back() {
            old_stats.user_times != stats.user_times
        } else {
            info!("first time getting stats");
            true
        };

        if new_stats {
            self.stats.push_back(stats);

            match self.stats.len() {
                0 => unimplemented!(),
                1 => {
                    let older = &Default::default();
                    let newer = &self.stats.back().unwrap().user_times;

                    self.diff = subtract_hashmaps(newer, older);
                }
                _ => {
                    let older = &self.stats.front().unwrap().user_times;
                    let newer = &self.stats.back().unwrap().user_times;

                    self.diff = subtract_hashmaps(newer, older);
                }
            }
        }

        new_stats
    }
}

/// The entry point for the yoink bot.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // create all the cancellation tokens that we need to shut down the app gracefully
    let cancellation_token = CancellationToken::new();

    let cancellation_guard = cancellation_token.clone().drop_guard();

    // configure the app
    init_logging().context("init logging")?;

    // listen for ctrl+c in the background
    let ctrl_c_cancellation = cancellation_token.clone();
    let _ctrl_c_handle = tokio::spawn(async move {
        if let Err(err) = tokio::signal::ctrl_c().await {
            error!(?err, "ctrl+c handler failed. shutting down");
        } else {
            info!("[ctrl+c] received. shutting down");
        }
        ctrl_c_cancellation.cancel();
    });

    let config = envy::from_env::<Config>().context("loading config")?;

    info!("Hello, {}! Ready to yoink their flags?!", config.user_id);

    // create app components
    let client = https_client().await?;

    let (app_state_tx, app_state_rx) = mpsc::unbounded_channel();

    // create app futures
    let yoinker_stat_loop_f = stats::stats_loop::<12>(
        app_state_tx,
        cancellation_token.clone(),
        client.clone(),
        config.clone(),
    );
    let yoinker_main_loop_f =
        yoinker::main_loop(app_state_rx, cancellation_token, &client, &config);

    // spawn background workers
    let yoinker_stats_handle = tokio::spawn(yoinker_stat_loop_f);

    // run the main app
    // TODO: join multiple futures here? don't try join because we want them all to have time to gracefully shut down
    let exit = yoinker_main_loop_f.await.context("yoinker main loop");

    // graceful shutdown
    drop(cancellation_guard);

    // TODO: we should probably just abort it. but i like checking for errors
    if let Err(err) = yoinker_stats_handle
        .await
        .expect("yoinker_stats_handle shouldn't ever have a join error")
    {
        error!(?err, "yoinker stats loop failed");
        // TODO: should this cause the main app to exit non-zero? maybe override exit if it isn't already an error. if it is an error, just log here
    }

    exit?;

    info!("exited successfully");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{State, Stats};
    use std::sync::Arc;

    fn stats(time: u64) -> Arc<Stats> {
        Arc::new(Stats {
            user_times: [("player".to_owned(), time)].into_iter().collect(),
            ..Stats::default()
        })
    }

    #[test]
    fn history_rolls_over_and_cloned_states_keep_their_snapshot() {
        let mut state = State::<2>::default();
        assert!(state.push_stats(stats(3)));
        assert_eq!(state.diff["player"], 3);
        let snapshot = state.clone();
        assert!(!state.push_stats(stats(3)));
        assert_eq!(state.stats.len(), 1);
        assert!(state.push_stats(stats(8)));
        assert_eq!(state.diff["player"], 5);
        assert!(state.push_stats(stats(15)));
        assert_eq!(state.stats.len(), 2);
        assert_eq!(state.diff["player"], 7);
        assert_eq!(snapshot.stats.len(), 1);
        assert_eq!(snapshot.diff["player"], 3);
        assert_eq!(snapshot.stats.back().unwrap().user_times["player"], 3);
    }
}
