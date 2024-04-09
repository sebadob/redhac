use std::env;
use std::time::Duration;
use tokio::time;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logging can be set up with the `tracing` crate
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // `redhac` is configured via env variables.
    // The idea behind it is that you can set up your application with HA capabilities while still
    // being able to switch modes via config file easily.
    // For the sake of this example, we set the env vars directly inside the code. Usually you
    // would want to configure them on the outside of course.

    // Enable the HA_MODE
    env::set_var("HA_MODE", "true");

    // Configure the HA Cache members. You need an uneven number for quorum.
    // For this example, we will have all of them on the same host on different ports to make
    // it work.
    env::set_var(
        "HA_HOSTS",
        "http://127.0.0.1:7071, http://127.0.0.1:7072, http://127.0.0.1:7073",
    );

    // Set a static token for cache authentication
    env::set_var("CACHE_AUTH_TOKEN", "SuperSecretToken1337");

    // Disable TLS for this example
    env::set_var("CACHE_TLS", "false");

    let args: Vec<String> = env::args().collect();
    let hostname_overwrite = if args.len() > 1 {
        args[1].clone()
    } else {
        return Err(anyhow::Error::msg(
            "Please provide the current node's HOSTNAME as the only argument",
        ));
    };

    // Configure a cache
    let (tx_health, mut cache_config) = redhac::CacheConfig::new();
    let cache_name = "my_cache";
    let cache = redhac::SizedCache::with_size(16);

    // If you do not specify a buffer as the last argument, an unbounded channel will be used.
    // This would have no upper limit of course, but it has lower latency and is faster overall.
    cache_config.spawn_cache(cache_name.to_string(), cache.clone(), None);

    // start server
    redhac::start_cluster(
        tx_health,
        &mut cache_config,
        // optional notification channel: `Option<mpsc::Sender<CacheNotify>>`
        None,
        // We need to overwrite the hostname so we can start all nodes on the same host for this
        // example. Usually, this will be set to `None`
        Some(hostname_overwrite),
    )
    .await?;
    info!("First cache node started");

    // Now just sleep until we ctrl + c, so we can start the other members and observe the behavior
    time::sleep(Duration::from_secs(6000)).await;

    // Let's simulate a graceful shutdown
    cache_config.shutdown().await?;

    Ok(())
}
