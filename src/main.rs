mod yoinker;

use anyhow::Context;
use futures::pin_mut;
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

    init_logging().context("init logging")?;

    let config = envy::from_env::<Config>().context("loading config")?;

    info!("Hello, {}! Ready to yoink their flags?!", config.user_id);

    let cancellation_token = CancellationToken::new();

    let client = https_client().await?;

    // create all the cancellation tokens that we need to shut down the app gracefully
    let main_loop_cancellation = cancellation_token.clone();
    let cancellation_guard = cancellation_token.drop_guard();

    // create the futures for the app
    let yoinker_main_loop_f = yoinker::main_loop(main_loop_cancellation, &client, &config);

    // rust futures magic
    pin_mut!(yoinker_main_loop_f);

    // if there is an error, we care about it above any later errors. but we do want a graceful shutdown process
    let first_result: anyhow::Result<()> = select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl-C. Shutting down.");
            Ok(())
        }
        x = &mut yoinker_main_loop_f => {
            info!("Yoinker main loop finished.");
            x
        }
    };

    drop(cancellation_guard);

    // graceful shutdown
    // TODO: do something with any errors here
    yoinker_main_loop_f.await.ok();

    // return an error if we got one
    first_result?;

    Ok(())
}

pub async fn https_client() -> anyhow::Result<Client> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .user_agent(APP_USER_AGENT)
        .https_only(true)
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

pub async fn sleep_with_cancel(cancellation_token: &CancellationToken, duration: Duration) {
    select! {
        _ = cancellation_token.cancelled() => {},
        _ = sleep(duration) => {}
    };
}
