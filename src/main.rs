mod strategy;
mod yoinker;

use anyhow::Context;
use reqwest::Client;
use serde::Deserialize;
use std::{env, time::Duration};
use tokio::{select, time::sleep};
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

/// application configuration
/// TODO: add Debug once this doesn't have secrets in it
#[derive(Deserialize)]
struct Config {
    /// TODO: get this based on the signer/api key?
    user_id: String,
    cast_hash: String,
    /// TODO: change this to `nn_api_key_file`
    nn_api_key: String,
    /// TODO: change this to `nn_signer_uuid_file`
    nn_signer_uuid: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // create all the cancellation tokens that we need to shut down the app gracefully
    let cancellation_token = CancellationToken::new();

    let main_loop_cancellation = cancellation_token.clone();
    let ctrl_c_cancellation = cancellation_token.clone();
    let cancellation_guard = cancellation_token.drop_guard();

    // configure the app
    init_logging().context("init logging")?;

    let config = envy::from_env::<Config>().context("loading config")?;

    info!("Hello, {}! Ready to yoink their flags?!", config.user_id);

    // create app components
    let client = https_client().await?;

    // create app futures
    let yoinker_main_loop_f = yoinker::main_loop(main_loop_cancellation, &client, &config);

    // listen for ctrl+c in the background
    let _ctrl_c_handle = tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("[ctrl+c] received. shutting down");
        ctrl_c_cancellation.cancel();
    });

    // run the main app
    // TODO: join multiple futures here? don't try join because we want them all to have time to gracefully shut down
    let exit = yoinker_main_loop_f.await.context("yoinker main loop");

    drop(cancellation_guard);

    // TODO: wait for shutdown? we don't have any async cleanup to do

    exit?;

    info!("exited successfully");

    Ok(())
}

/// create a new HTTPS-only client with our app's user agent
pub async fn https_client() -> anyhow::Result<Client> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .http2_keep_alive_interval(Duration::from_secs(50))
        .http2_keep_alive_timeout(Duration::from_secs(60))
        .https_only(true)
        .user_agent(APP_USER_AGENT)
        .build()
        .context("http client error")?;

    Ok(client)
}

/// Log formatting is handled outside of the Config.
/// Choose by setting the LOG_FORMAT environment variable.
/// "pretty" logging for development or "json" logging for log collectors.
pub fn init_logging() -> anyhow::Result<()> {
    let log_format = env::var("LOG_FORMAT").ok();

    let subscriber = FmtSubscriber::builder().with_env_filter(EnvFilter::from_default_env());

    match log_format.as_deref() {
        Some("json") => {
            let subscriber = subscriber.json().finish();

            tracing::subscriber::set_global_default(subscriber)
                .context("Setting default subscriber failed")?;
        }
        Some("pretty") | None => {
            let subscriber = subscriber.pretty().finish();

            tracing::subscriber::set_global_default(subscriber)
                .context("Setting default subscriber failed")?;
        }
        Some(x) => {
            anyhow::bail!("Invalid log format: {}", x);
        }
    }

    Ok(())
}

/// this keeps us from wasting time when we want to shut down
/// TODO: helper that sleeps a random duration?
pub async fn sleep_with_cancel(cancellation_token: &CancellationToken, duration: Duration) {
    select! {
        _ = cancellation_token.cancelled() => {},
        _ = sleep(duration) => {}
    };
}
