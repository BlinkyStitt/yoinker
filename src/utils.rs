use crate::APP_USER_AGENT;
use anyhow::Context;
use im::HashMap;
use reqwest::Client;
use std::fmt::Debug;
use std::hash::Hash;
use std::{env, ops::SubAssign, time::Duration};
use tracing::warn;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// create a new HTTPS-only client with our app's user agent.
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

    // TODO: tokio-console
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

/// helper function to subtract two hashmaps and return the diference.
pub fn subtract_hashmaps<K, V>(newer: &HashMap<K, V>, older: &HashMap<K, V>) -> HashMap<K, V>
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
