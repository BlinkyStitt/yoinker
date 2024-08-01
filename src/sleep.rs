use crate::COOLDOWN_TIME;
use nanorand::Rng;
use std::time::Duration;
use tokio::{select, time::sleep};
use tokio_util::sync::CancellationToken;
use tracing::info;

/// this keeps us from wasting time when we want to shut down
/// TODO: helper that sleeps a random duration?
pub async fn sleep_with_cancel(cancellation_token: &CancellationToken, duration: Duration) {
    select! {
        _ = cancellation_token.cancelled() => {},
        _ = sleep(duration) => {}
    };
}

#[tracing::instrument]
pub fn short_jitter() -> Duration {
    // TODO: percentage of the cooldown time

    let max_short = (COOLDOWN_TIME / 10).as_millis() as u64;

    let ms = nanorand::tls_rng().generate_range(0..=max_short);

    info!(ms);

    Duration::from_millis(ms)
}

#[tracing::instrument]
pub fn long_jitter() -> Duration {
    let max_long = (COOLDOWN_TIME / 2).as_millis() as u64;

    let ms = nanorand::tls_rng().generate_range(0..=max_long);

    info!(ms);

    Duration::from_millis(ms)
}

#[inline]
pub async fn sleep_short_jitter(cancellation_token: &CancellationToken) {
    sleep_with_cancel(cancellation_token, short_jitter()).await;
}

#[inline]
pub async fn sleep_long_jitter(cancellation_token: &CancellationToken) {
    sleep_with_cancel(cancellation_token, long_jitter()).await;
}

#[inline]
pub async fn sleep_cooldown_jitter(cancellation_token: &CancellationToken) {
    sleep_with_cancel(cancellation_token, COOLDOWN_TIME + short_jitter()).await;
}
