use std::cmp;
use std::env;
use std::time::Duration;
use tokio::{select, time};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

mod cache;

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
        "http://127.0.0.1:7001, http://127.0.0.1:7002, http://127.0.0.1:7003",
    );

    // Set a static token for cache authentication
    env::set_var("CACHE_AUTH_TOKEN", "SuperSecretToken1337");

    // Disable TLS for this example
    env::set_var("CACHE_TLS", "false");

    // This value controls how many runs should be done (successfully) until the conflict
    // resolution test can be considered successful.
    // This value is rather low for this example. It was tested with many thousands beforehand.
    let goal = 10;
    let mut success = 0;
    let mut exec_times = 0;
    let mut exec_min = 999;
    let mut exec_max = 0;

    for _ in 1..=goal {
        select! {
            res = cache::start_caches() => {
                match res {
                    Ok(secs_until_healthy) => {
                        success += 1;

                        exec_times += secs_until_healthy;
                        exec_min = cmp::min(exec_min, secs_until_healthy);
                        exec_max = cmp::max(exec_max, secs_until_healthy);

                        println!(r#"
##############################

Successful runs: {} / {}

##############################

"#, success, goal
                        );
                    }
                    Err(_) => break,
                }
            },

            _ = time::sleep(Duration::from_secs(30)) => {
                eprintln!("Timeout exceeded - aborting");
                break;
            }
        }
    }

    println!(
        r#"
    Successful runs: {} / {}
    Times takes until fully healthy cluster state:
        min:    {} s
        max:    {} s
        median: {} s
    "#,
        success,
        goal,
        exec_min,
        exec_max,
        // Yes, division by 0 if the first run fails - does not matter if it panics in that case
        exec_times / success
    );

    Ok(())
}
