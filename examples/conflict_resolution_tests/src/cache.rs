use std::time::Duration;
use tokio::time;
use tracing::info;

pub async fn start_caches() -> anyhow::Result<u64> {
    let start = time::Instant::now();

    // Configure a cache
    let (tx_health_1, mut cache_config_1) = redhac::CacheConfig::new();
    let cache_name = "my_cache";
    let cache = redhac::SizedCache::with_size(16);

    // If you do not specify a buffer as the last argument, an unbounded channel will be used.
    // This would have no upper limit of course, but it has lower latency and is faster overall.
    cache_config_1.spawn_cache(cache_name.to_string(), cache.clone(), None);

    // start server
    redhac::start_cluster(
        tx_health_1,
        &mut cache_config_1,
        // optional notification channel: `Option<mpsc::Sender<CacheNotify>>`
        None,
        // We need to overwrite the hostname so we can start all nodes on the same host for this
        // example. Usually, this will be set to `None`
        Some("127.0.0.1:7001".to_string()),
    )
    .await?;
    info!("First cache node started");

    // Mimic the other 2 cache members. This should usually not be done in the same code - only
    // for this example to make it work.
    let (tx_health_2, mut cache_config_2) = redhac::CacheConfig::new();
    cache_config_2.spawn_cache(cache_name.to_string(), cache.clone(), None);
    redhac::start_cluster(
        tx_health_2,
        &mut cache_config_2,
        None,
        Some("127.0.0.1:7002".to_string()),
    )
    .await?;
    info!("2nd cache node started");
    // Now after the 2nd cache member has been started, we would already have quorum and a
    // working cache layer (as soon as the connection is established of course). As long as there
    // is no leader and / or quorum, the cache will not save any values to avoid inconsistencies.

    let (tx_health_3, mut cache_config_3) = redhac::CacheConfig::new();
    cache_config_3.spawn_cache(cache_name.to_string(), cache.clone(), None);
    redhac::start_cluster(
        tx_health_3,
        &mut cache_config_3,
        None,
        Some("127.0.0.1:7003".to_string()),
    )
    .await?;
    info!("3rd cache node started");

    // For the sake of this example again, we need to wait until the cache is in a healthy
    // state, before we can actually insert a value
    let caches = [&cache_config_1, &cache_config_2, &cache_config_3];
    loop {
        let mut leaders = 0;
        let mut followers = 0;
        for cache in caches {
            let health_borrow = cache.rx_health_state.borrow();
            let health = health_borrow.as_ref().unwrap();

            if health.health != redhac::quorum::QuorumHealth::Good {
                break;
            }
            match health.state {
                redhac::quorum::QuorumState::Leader => leaders += 1,
                redhac::quorum::QuorumState::Follower => followers += 1,
                _ => {}
            }
        }

        // Let's make 100% sure that we have 1 Leader and 2 Followers
        if leaders == 1 && followers == 2 {
            info!(">>> Each cache member is fully initialized <<<");
            break;
        }

        info!("Wait until all cache members have found each other");
        time::sleep(Duration::from_secs(1)).await;
    }

    let secs_until_healthy = start.elapsed().as_secs();
    info!("Cache is fully started up and healthy - shutting down now");

    // Graceful Shutdown
    // We should send a signal through the exit channel to execute a graceful shutdown of the cache.
    // The oneshot channel will send the ack back, when the shutdown has finished to not exit
    // too early.
    info!("Sending exit signal to cache 1");
    cache_config_1.shutdown().await?;

    // Now to the same for cache 2 and 3, just for this example
    info!("Sending exit signal to cache 2");
    cache_config_2.shutdown().await?;

    info!("Sending exit signal to cache 3");
    cache_config_3.shutdown().await?;

    Ok(secs_until_healthy)
}
