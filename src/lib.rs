//! `redhac` is derived from **Rust Embedded Distributed Highly Available Cache**
//!
//! The keywords **embedded** and **distributed** are a bit strange at the same time.<br>
//! The idea of `redhac` is to provide a caching library which can be embedded into any Rust
//! application, while still providing the ability to build a distributed HA caching layer.
//!
//! It can be used as a cache for a single instance too, of course, but then it will not be the
//! most performant, since it needs clones of most values to be able to send them over the network
//! concurrently. If you need a distributed cache however, you might give `redhac` a try.
//!
//! The underlying system works with quorum and will elect a leader dynamically on startup. Each
//! node will start a server and a client part for bi-directional gRPC streaming. Each client will
//! connect to each of the other servers. This is the reason why the performance will degrade, if
//! you scale up replicas without further optimization. You can go higher, but the optimal amount
//! of HA nodes is 3.
//!
//! # Single Instance
//!
//! ```rust
//! // These 3 are needed for the `cache_get` macro
//! use redhac::{cache_get, cache_get_from, cache_get_value};
//! use redhac::{cache_put, CacheConfig, SizedCache};
//!
//! #[tokio::main]
//! async fn main() {
//!     let (_, mut cache_config) = CacheConfig::new();
//!
//!     // The cache name is used to reference the cache later on
//!     let cache_name = "my_cache";
//!     // We need to spawn a global handler for each cache instance.
//!     // Communication is done over channels.
//!     cache_config.spawn_cache(cache_name.to_string(), SizedCache::with_size(16), None);
//!
//!     // Cache keys can only be `String`s at the time of writing.
//!     let key = "myKey";
//!     // The value you want to cache must implement `serde::Serialize`.
//!     // The serialization of the values is done with `bincode`.
//!     let value = "myCacheValue".to_string();
//!
//!     // At this point, we need cloned values to make everything work nicely with networked
//!     // connections. If you only ever need a local cache, you might be better off with using the
//!     // `cached` crate directly and use references whenever possible.
//!     cache_put(cache_name.to_string(), key.to_string(), &cache_config, &value)
//!         .await
//!         .unwrap();
//!
//!     let res = cache_get!(
//!         // The type of the value we want to deserialize the value into
//!         String,
//!         // The cache name from above. We can start as many for our application as we like
//!         cache_name.to_string(),
//!         // For retrieving values, the same as above is true - we need real `String`s
//!         key.to_string(),
//!         // All our caches have the necessary information added to the cache config. Here a
//!         // reference is totally fine.
//!         &cache_config,
//!         // This does not really apply to this single instance example. If we would have started
//!         // a HA cache layer we could do remote lookups for a value we cannot find locally, if
//!         // this would be set to `true`
//!         false
//!     )
//!         .await
//!         .unwrap();
//!
//!     assert!(res.is_some());
//!     assert_eq!(res.unwrap(), value);
//! }
//! ```
//!
//! # High Availability
//!
//! The High Availability (HA) works in a way, that each cache member connects to each other. When
//! eventually quorum is reached, a leader will be elected, which then is responsible for all cache
//! modifications to prevent collisions (if you do not decide against it with a direct `cache_put`).
//! Since each node connects to each other, it means that you cannot just scale up the cache layer
//! infinitely. The ideal number of nodes is 3. You can scale this number up for instance to 5 or 7
//! if you like, but this has not been tested in greater detail so far.
//! **Write performance** will degrade the more nodes you add to the cluster, since you simply need
//! to wait for more Ack's from the other members.
//! **Read performance** however should stay the same.
//! Each node will keep a local copy of each value inside the cache (if it has not lost connection
//! or joined the cluster at some later point), which means in most cases reads do not require any
//! remote network access.
//!
//! ## Configuration
//!
//! The way to configure the `HA_MODE` is optimized for a Kubernetes deployment but may seem a bit
//! odd at the same time, if you deploy somewhere else. You can either provide the `.env` file and
//! use it as a config file, or just set these variables for the environment directly. You need
//! to set the following values:
//!
//! ### `HA_MODE`
//!
//! The first one is easy, just set `HA_MODE=true`
//!
//! ### `HA_HOSTS`
//!
//! **NOTE:**<br>
//! In a few examples down below, the name for deployments may be `rauthy`. The reason is, that this
//! crate was written originally to complement another project of mine, Rauthy (link will follow),
//! which is an OIDC Provider and Single Sign-On Solution written in Rust.
//!
//! The `HA_HOSTS` is working in a way, that it is really easy inside Kubernetes to configure it,
//! as long as a `StatefulSet` is used for the deployment.
//!
//! The way a cache node finds its members is by the `HA_HOSTS` and its own `HOSTNAME`.
//! In the `HA_HOSTS`, add every cache member. For instance, if you want to use 3 replicas in HA
//! mode which are running and deployed as a `StatefulSet` with the name `rauthy` again:
//!
//! ```text
//! HA_HOSTS="http://rauthy-0:8000, http://rauthy-1:8000 ,http://rauthy-2:8000"
//! ```
//!
//! The way it works:
//!
//! 1. **A node gets its own hostname from the OS**<br>
//! This is the reason, why you use a StatefulSet for the deployment, even without any volumes
//! attached. For a `StatefulSet` called `rauthy`, the replicas will always have the names `rauthy-0`,
//! `rauthy-1`, ..., which are at the same time the hostnames inside the pod.
//! 2. **Find "me" inside the `HA_HOSTS` variable**<br>
//! If the hostname cannot be found in the `HA_HOSTS`, the application will panic and exit because
//! of a misconfiguration.
//! 3. **Use the port from the "me"-Entry that was found for the server part**<br>
//! This means you do not need to specify the port in another variable which eliminates the risk of
//! having inconsistencies
//! or a bad config in that case.
//! 4. **Extract "me" from the `HA_HOSTS`**<br>
//! then take the leftover nodes as all cache members and connect to them
//! 5. **Once a quorum has been reached, a leader will be elected**<br>
//! From that point on, the cache will start accepting requests
//! 6. **If the leader is lost - elect a new one - No values will be lost**
//! 7. **If quorum is lost, the cache will be invalidated**<br>
//! This happens for security reasons to provide cache inconsistencies. Better invalidate the cache
//! and fetch the values fresh from the DB or other cache members than working with possibly invalid
//! values, which is especially true in an authn / authz situation.
//!
//! **NOTE:**<br>
//! If you are in an environment where the described mechanism with extracting the hostname would
//! not work, you can set the `HOSTNAME_OVERWRITE` for each instance to match one of the `HA_HOSTS`
//! entries, or you can overwrite the name when using the `redhac::start_cluster`.
//!
//! ### `CACHE_AUTH_TOKEN`
//!
//! You need to set a secret for the `CACHE_AUTH_TOKEN`, which is then used for authenticating
//! cache members.
//!
//! ### TLS
//!
//! For the sake of this example, we will not dig into TLS and disable it in the example, which
//! can be done with `CACHE_TLS=false`.
//!
//! You can add your TLS certificates in PEM format and an optional Root CA. This is true for the
//! Server and the Client part separately. This means you can configure the cache layer to use mTLS
//! connections.
//!
//! ### Reference Config
//!
//! The following variables are the ones you can use to configure `redhac` via env vars.<br>
//! At the time of writing, the configuration can only be done via the env.
//!
//! ```text
//! # If the cache should start in HA mode or standalone
//! # accepts 'true|false', defaults to 'false'
//! HA_MODE=true
//!
//! # The connection strings (with hostnames) of the HA instances as a CSV
//! # Format: 'scheme://hostname:port'
//! HA_HOSTS="http://redhac.redhac:8080, http://redhac.redhac:8180 ,http://redhac.redhac:8280"
//!
//! # This can overwrite the hostname which is used to identify each cache member.
//! # Useful in scenarios, where all members are on the same host or for testing.
//! # You need to add the port, since `redhac` will do an exact match to find "me".
//! #HOSTNAME_OVERWRITE="127.0.0.1:8080"
//!
//! # Enable / disable TLS for the cache communication (default: true)
//! CACHE_TLS=true
//!
//! # The path to the server TLS certificate PEM file (default: tls/redhac.cert-chain.pem)
//! CACHE_TLS_SERVER_CERT=tls/redhac.cert-chain.pem
//! # The path to the server TLS key PEM file (default: tls/redhac.key.pem)
//! CACHE_TLS_SERVER_KEY=tls/redhac.key.pem
//!
//! # The path to the client mTLS certificate PEM file. This is optional.
//! CACHE_TLS_CLIENT_CERT=tls/redhac.local.cert.pem
//! # The path to the client mTLS key PEM file. This is optional.
//! CACHE_TLS_CLIENT_KEY=tls/redhac.local.key.pem
//!
//! # If not empty, the PEM file from the specified location will be added as the CA certificate chain for validating
//! # the servers TLS certificate. This is optional.
//! CACHE_TLS_CA_SERVER=tls/ca-chain.cert.pem
//! # If not empty, the PEM file from the specified location will be added as the CA certificate chain for validating
//! # the clients mTLS certificate. This is optional.
//! CACHE_TLS_CA_CLIENT=tls/ca-chain.cert.pem
//!
//! # The domain / CN the client should validate the certificate against. This domain MUST be inside the
//! # 'X509v3 Subject Alternative Name' when you take a look at the servers certificate with the openssl tool.
//! # default: redhac.local
//! CACHE_TLS_CLIENT_VALIDATE_DOMAIN=redhac.local
//!
//! # Can be used if you need to overwrite the SNI when the client connects to the server, for
//! # instance if you are behind a loadbalancer which combines multiple certificates. (default: "")
//! #CACHE_TLS_SNI_OVERWRITE=
//!
//! # Define different buffer sizes for channels between the components
//! # Buffer for client request on the incoming stream - server side (default: 128)
//! # Makes sense to have the CACHE_BUF_SERVER roughly set to:
//! # `(number of total HA cache hosts - 1) * CACHE_BUF_CLIENT`
//! CACHE_BUF_SERVER=128
//! # Buffer for client requests to remote servers for all cache operations (default: 64)
//! CACHE_BUF_CLIENT=64
//!
//! # Secret token, which is used to authenticate the cache members
//! CACHE_AUTH_TOKEN=SuperSafeSecretToken1337
//!
//! # Connections Timeouts
//! # The Server sends out keepalive pings with configured timeouts
//!
//! # The keepalive ping interval in seconds (default: 5)
//! CACHE_KEEPALIVE_INTERVAL=5
//!
//! # The keepalive ping timeout in seconds (default: 5)
//! CACHE_KEEPALIVE_TIMEOUT=5
//!
//! # The timeout for the leader election. If a newly saved leader request has not reached quorum
//! # after the timeout, the leader will be reset and a new request will be sent out.
//! # CAUTION: This should not be below CACHE_RECONNECT_TIMEOUT_UPPER, since cold starts and
//! # elections will be problematic in that case.
//! # value in seconds, default: 5
//! CACHE_ELECTION_TIMEOUT=5
//!
//! # These 2 values define the reconnect timeout for the HA Cache Clients.
//! # The values are in ms and a random between these 2 will be chosen each time to avoid conflicts
//! # and race conditions (default: 2500)
//! CACHE_RECONNECT_TIMEOUT_LOWER=2500
//! # (default: 5000)
//! CACHE_RECONNECT_TIMEOUT_UPPER=5000
//! ```
//!
//! ## Example
//!
//! ```rust
//! use std::env;
//! use redhac::*;
//! use redhac::quorum::{AckLevel, QuorumState};
//! use std::time::Duration;
//! use tokio::time;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // `redhac` is configured via env variables.
//!     // For the sake of this example, we set the env vars directly inside the code. Usually you
//!     // would want to configure them on the outside of course.
//!
//!     // Enable the HA_MODE
//!     env::set_var("HA_MODE", "true");
//!
//!     // Configure the HA Cache members. You need an uneven number for quorum.
//!     // For this example, we will have all of them on the same host on different ports to make
//!     // it work.
//!     env::set_var(
//!         "HA_HOSTS",
//!         "http://127.0.0.1:7001, http://127.0.0.1:7002, http://127.0.0.1:7003",
//!      );
//!
//!     // Disable TLS for this example
//!     env::set_var("CACHE_TLS", "false");
//!
//!     // Configure a cache
//!     let (tx_health_1, mut cache_config_1) = CacheConfig::new();
//!     let cache_name = "my_cache";
//!     let cache = SizedCache::with_size(16);
//!     cache_config_1.spawn_cache(cache_name.to_string(), cache.clone(), None);
//!
//!     // start server
//!     start_cluster(
//!         tx_health_1,
//!         &mut cache_config_1,
//!         // optional notification channel: `Option<mpsc::Sender<CacheNotify>>`
//!         None,
//!         // We need to overwrite the hostname, so we can start all nodes on the same host for this
//!         // example. Usually, this will be set to `None`
//!         Some("127.0.0.1:7001".to_string()),
//!     )
//!     .await?;
//!     time::sleep(Duration::from_millis(100)).await;
//!     println!("First cache node started");
//!
//!     // Mimic the other 2 cache members. This should usually not be done in the same code - only
//!     // for this example to make it work.
//!     let (tx_health_2, mut cache_config_2) = CacheConfig::new();
//!     cache_config_2.spawn_cache(cache_name.to_string(), cache.clone(), None);
//!     start_cluster(
//!         tx_health_2,
//!         &mut cache_config_2,
//!         None,
//!         Some("127.0.0.1:7002".to_string()),
//!     )
//!     .await?;
//!     time::sleep(Duration::from_millis(100)).await;
//!     println!("2nd cache node started");
//!     // Now after the 2nd cache member has been started, we would already have quorum and a
//!     // working cache layer. As long as there is no leader and / or quorum, the cache will not
//!     // save any values to avoid inconsistencies.
//!
//!     let (tx_health_3, mut cache_config_3) = CacheConfig::new();
//!     cache_config_3.spawn_cache(cache_name.to_string(), cache.clone(), None);
//!     start_cluster(
//!         tx_health_3,
//!         &mut cache_config_3,
//!         None,
//!         Some("127.0.0.1:7003".to_string()),
//!     )
//!     .await?;
//!     time::sleep(Duration::from_millis(100)).await;
//!     println!("3rd cache node started");
//!
//!     // For the sake of this example again, we need to wait until the cache is in a healthy
//!     // state, before we can actually insert a value
//!     loop {
//!         let health_borrow = cache_config_1.rx_health_state.borrow();
//!         let health = health_borrow.as_ref().unwrap();
//!         if health.state == QuorumState::Leader || health.state == QuorumState::Follower {
//!             break;
//!         }
//!         time::sleep(Duration::from_secs(1)).await;
//!     }
//!
//!     println!("Cache 1 State: {:?}", cache_config_1.rx_health_state.borrow());
//!     println!("Cache 2 State: {:?}", cache_config_2.rx_health_state.borrow());
//!     println!("Cache 3 State: {:?}", cache_config_3.rx_health_state.borrow());
//!
//!     println!("We are ready to go - insert and get a value");
//!
//!     let entry = "myKey".to_string();
//!     // `cache_insert` is the HA version of `cache_put` from the single instance example.
//!     //
//!     // `cache_put` does a direct push in HA situations, ignoring the leader. This is way more
//!     // performant, but may lead to collisions. However, if you are inserting a completely new
//!     // value with a unique new key, for instance a newly generated UUID, it should be favored,
//!     // since the same UUID will not be inserted from someone else most likely.
//!     //
//!     // `cache_insert` can be used if you want to avoid conflicts. With the `AckLevel`, you can
//!     // specify the level of safety you want. For instance, if we use `AckLevel::Quorum`, the
//!     // Result will not be `Ok(())` until the leader has at least the ack for the insert from at
//!     // least "quorum" amount of nodes.
//!     // Different levels provide different levels of safety and performance - it is always a
//!     // tradeoff.
//!     cache_insert(
//!         cache_name.to_string(),
//!         entry.clone(),
//!         &cache_config_1,
//!         // This is a reference to the value, which must implement `serde::Serialize`
//!         &1337i32,
//!         // The `AckLevel` defines the level of safety we want
//!         AckLevel::Quorum
//!     )
//!         .await?;
//!     // check the value
//!     let one = cache_get!(
//!         i32,
//!         cache_name.to_string(),
//!         entry,
//!         &cache_config_1,
//!         // Nn a HA situation, we could set this to `true` if we wanted to do a lookup on another
//!         // cache member in case we do not find the value locally, for instance if a node joined
//!         // at a later time or when there was a network partition.
//!         false
//!     )
//!         .await?;
//!     assert_eq!(one, Some(1337));
//!
//!     Ok(())
//! }
//! ```

use crate::client::{cache_clients, RpcRequest};
use crate::quorum::{quorum_handler, QuorumReq};
use crate::server::{CacheMap, RpcCacheService};
use bincode::ErrorKind;
use cached::Cached;
pub use cached::SizedCache;
pub use cached::TimedCache;
pub use cached::TimedSizedCache;
use flume::{RecvError, SendError};
use gethostname::gethostname;
use lazy_static::lazy_static;
use nanoid::nanoid;
use rand::Rng;
use std::collections::HashMap;
use std::env;
use std::fmt::{Debug, Display, Formatter};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time;
use tracing::{debug, error, info, warn};

pub use crate::quorum::{AckLevel, QuorumHealth, QuorumHealthState, QuorumState};

mod client;
pub mod quorum;
#[allow(clippy::enum_variant_names)]
mod rpc;
mod server;

lazy_static! {
    /// 'HA_MODE' as environment variable must be set to 'true' to enable the ha mode. Otherwise,
    /// the cache will start in standalone mode and not sync / lookup on remote hosts.
    pub(crate) static ref HA_MODE: bool = {
        let ha_mode = env::var("HA_MODE").unwrap_or_else(|_| String::from("false"));
        if ha_mode != "true" {
            info!("redhac is starting in standalone mode");
            false
        } else {
            info!("redhac is starting in HA mode");
            true
        }
    };

    pub(crate) static ref QUORUM: u8 = {
        let ha_hosts = env::var("HA_HOSTS").expect("HA_HOSTS is not set");
        let len = ha_hosts.split(',').count();
        (len / 2) as u8
    };

    pub(crate) static ref TLS: bool = env::var("CACHE_TLS")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .expect("Cannot parse CACHE_TLS to bool");
    pub(crate) static ref MTLS: bool = env::var("CACHE_MTLS")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .expect("Cannot parse CACHE_MTLS to bool");
}

/**
This is a simple macro to get values from the cache and deserialize them properly at the same
time.

## Example usage:
```ignore
use redhac::{cache_get, cache_get_value, cache_get_from};
// cache_get!(<ReturnType>, <CacheNameAsString>, <KeyAsString>, &CacheConfig, <IFNotFoundLocally - DoRemoteLookip?>).await?;
cache_get!(String, "cache_name".to_string(), "key_name".to_string(), &cache_config, false).await?;
```

This would look for the entry `key_name` in the cache with the name `cache_name` and will try to
deserialize the value, if it is `Some(_)`, into a `String`. If it does not find it in
the local cache, it will not try to do a remote lookup on the other ha cache instances.<br />
The `&cache_config` is of type `&CacheConfig`, which is being created, when the caches are created.
 */
#[macro_export]
macro_rules! cache_get {
    ($type:ty, $name:expr, $entry:expr, $config:expr, $lookup:expr) => {
        async {
            if let Some(v) = cache_get_value($name, $entry, $config, $lookup)
                .await
                .unwrap_or(None)
            {
                cache_get_from::<$type>(&v).await
            } else {
                Ok(None)
            }
        }
    };
}

/// The `CacheNotify` will be sent over the optional `tx_notify` for the server, so clients are
/// able to subscribe to updates inside their cache.
pub struct CacheNotify {
    pub cache_name: String,
    pub entry: String,
    pub method: CacheMethod,
}

/// These `CacheMethod`s should be self explainatory.
/// However, `Insert` and `Remove` are only executed over the leader and routed internally, whereas
/// `Put` and `Del` are direct methods, which ignore the leader and trade conflict safety for faster
/// execution speed.
pub enum CacheMethod {
    Put,
    Insert(AckLevel),
    Del,
    Remove(AckLevel),
}

/// The CacheConfig needs to be initialized at the very beginning of the application and must be
/// given to the [start_cluster](start_cluster) function, when running with `HA_MODE` == true.
///
/// Most values are only used internally, but you can retrieve the current health state of the
/// HA cluster via the `health_state`, which is the only `pub` in here.<br>
/// It is a watch channel which may hold a `QuorumHealthState` value, if the cache is running in
/// `HA_MODE`.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    cache_map: CacheMap,
    pub rx_health_state: watch::Receiver<Option<QuorumHealthState>>,
    tx_remote: Option<flume::Sender<RpcRequest>>,
    rx_remote: Option<flume::Receiver<RpcRequest>>,
    tx_quorum: Option<flume::Sender<QuorumReq>>,
    rx_quorum: Option<flume::Receiver<QuorumReq>>,
    tx_exit: Option<flume::Sender<oneshot::Sender<()>>>,
}

impl CacheConfig {
    /**
    This returns a tuple with the first value being the watch receiver channel, which returns the
    current `QuorumHealthState`.

    This function looks for the `HA_MODE` environment variable and if it is `true`, will setup the
    channels for HA communication, otherwise they will be `None`.
     */
    pub fn new() -> (watch::Sender<Option<QuorumHealthState>>, Self) {
        let cache_map = HashMap::new();
        let (tx_watch, rx_watch) = watch::channel::<Option<QuorumHealthState>>(None);

        if *HA_MODE {
            // initialize the tx_watch with default values
            tx_watch.send(Some(QuorumHealthState::default())).unwrap();

            let (tx_remote, rx_remote) = flume::unbounded::<RpcRequest>();
            let (tx_quorum, rx_quorum) = flume::unbounded::<QuorumReq>();
            let cfg = Self {
                cache_map,
                rx_health_state: rx_watch,
                tx_remote: Some(tx_remote),
                rx_remote: Some(rx_remote),
                tx_quorum: Some(tx_quorum),
                rx_quorum: Some(rx_quorum),
                tx_exit: None,
            };
            (tx_watch, cfg)
        } else {
            let cfg = Self {
                cache_map,
                rx_health_state: rx_watch,
                tx_remote: None,
                rx_remote: None,
                tx_quorum: None,
                rx_quorum: None,
                tx_exit: None,
            };
            (tx_watch, cfg)
        }
    }

    /// Adds / spawns a new cache, which will run internally as an independent tokio task.
    /// The channel will be unbounded if `buffer == None`.
    pub fn spawn_cache<C: Cached<String, Vec<u8>> + Send + 'static>(
        &mut self,
        cache_name: String,
        cache: C,
        buffer: Option<usize>,
    ) {
        let (tx, rx) = if let Some(buf) = buffer {
            flume::bounded::<CacheReq>(buf)
        } else {
            flume::unbounded()
        };

        self.cache_map.insert(cache_name.clone(), tx);
        tokio::spawn(cache_recv(cache, cache_name, rx));
    }

    pub async fn shutdown(&self) -> Result<(), CacheError> {
        if let Some(tx) = &self.tx_exit {
            let (tx_exit_ack, rx_exit_ack) = oneshot::channel();
            tx.send_async(tx_exit_ack)
                .await
                .expect("cache shutdown receiver to not be closed");
            rx_exit_ack
                .await
                .expect("cache shutdown receiver to not be closed");
        }
        Ok(())
    }
}

/// The general CacheError which is returned by most of the cache wrapper functions.
#[derive(Debug)]
pub struct CacheError {
    pub error: String,
}

impl Display for CacheError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.error)
    }
}

impl std::error::Error for CacheError {}

impl From<std::boxed::Box<bincode::ErrorKind>> for CacheError {
    fn from(value: Box<ErrorKind>) -> Self {
        Self {
            error: format!("bincode serializing error: {}", value),
        }
    }
}

impl From<flume::SendError<RpcRequest>> for CacheError {
    fn from(value: SendError<RpcRequest>) -> Self {
        match value {
            SendError(err) => Self {
                error: format!("Flume Cache Error: {:?}", err),
            },
        }
    }
}

impl From<&flume::SendError<RpcRequest>> for CacheError {
    fn from(value: &SendError<RpcRequest>) -> Self {
        match value {
            SendError(err) => Self {
                error: format!("Flume Cache Error: {:?}", err),
            },
        }
    }
}

impl From<flume::RecvError> for CacheError {
    fn from(value: RecvError) -> Self {
        match value {
            RecvError::Disconnected => Self {
                error: "Flume Cache Error: RecvError::Disconnected".to_string(),
            },
        }
    }
}

impl From<flume::SendError<QuorumReq>> for CacheError {
    fn from(value: SendError<QuorumReq>) -> Self {
        match value {
            SendError(err) => Self {
                error: format!("Flume Cache Error: {:?}", err),
            },
        }
    }
}

impl From<&flume::SendError<QuorumReq>> for CacheError {
    fn from(value: &SendError<QuorumReq>) -> Self {
        match value {
            SendError(err) => Self {
                error: format!("Flume Cache Error: {:?}", err),
            },
        }
    }
}

impl From<tokio::sync::mpsc::error::SendError<CacheReq>> for CacheError {
    fn from(value: mpsc::error::SendError<CacheReq>) -> Self {
        match value {
            mpsc::error::SendError(err) => Self {
                error: format!("Flume Cache Error: {:?}", err),
            },
        }
    }
}

impl From<flume::SendError<CacheReq>> for CacheError {
    fn from(err: SendError<CacheReq>) -> Self {
        match err {
            SendError(err) => Self {
                error: format!("Flume Cache Error: {:?}", err),
            },
        }
    }
}

/// The enum which will be sent to the "real" cache handler loop in the end.
#[derive(Debug)]
pub enum CacheReq {
    Get {
        entry: String,
        resp: flume::Sender<Option<Vec<u8>>>,
    },
    Put {
        entry: String,
        value: Vec<u8>,
    },
    Del {
        entry: String,
    },
    Reset,
}

/**
Gets values out of the cache.

This should not be used directly, but the `cache_get!` macro instead, which needs lees
boilerplate code. The [cache_get_from](cache_get_from), which deserializes values from the
cache, needs to exist on its own for different reasons.<br>
Everything should be pretty straight forward. If a value does not exist in the local cache,
for instance after a restart or late join, when `remote_lookup` is `true`, the function will try
to get the value from any other instance. This is useful, if you have values which only live
inside the cache.
 */
pub async fn cache_get_value(
    cache_name: String,
    entry: String,
    cache_config: &CacheConfig,
    remote_lookup: bool,
) -> Result<Option<Vec<u8>>, CacheError> {
    let health_state = if let Some(s) = cache_config.rx_health_state.borrow().clone() {
        s.is_quorum_good()?;
        Some(s)
    } else {
        None
    };

    let (tx_resp, rx_resp) = flume::unbounded::<Option<Vec<u8>>>();
    let req = CacheReq::Get {
        entry: entry.clone(),
        resp: tx_resp,
    };
    let tx = cache_config.cache_map.get(&cache_name).ok_or_else(|| {
        let err = format!(
            "CacheMap misconfiguration - could not find expected cache_name '{}'",
            &cache_name
        );
        error!("{err}");
        CacheError { error: err }
    })?;

    tx.send_async(req).await.map_err(|e| {
        let err = format!("Error sending local cache value over the channel: {:?}", e);
        error!("{err}");
        CacheError { error: err }
    })?;

    // always check the local cache first
    let local_res = rx_resp.recv_async().await.map_err(|e| {
        let err = format!("Error receiving cache value through channel: {:?}", e);
        error!("{err}");
        CacheError { error: err }
    })?;

    // only if it does not exist locally and we should lookup remotely
    if local_res.is_none() && health_state.is_some() && remote_lookup {
        let (resp_tx, resp_rx) = flume::unbounded();
        let req = RpcRequest::Get {
            cache_name,
            entry,
            resp: resp_tx,
        };
        cache_config
            .tx_remote
            .as_ref()
            .unwrap()
            .send_async(req)
            .await
            .map_err(|e| {
                let err = format!("Error sending request to RPC Clients: {:?}", e);
                error!("{err}");
                CacheError { error: err }
            })?;

        // wait for the results
        let mut res;
        let mut connected_hosts = health_state.unwrap().connected_hosts;
        loop {
            res = resp_rx.recv_async().await?;
            if res.is_some() {
                break; // found a value - exit early
            }
            if connected_hosts == 1 {
                break; // value was not found
            }
            connected_hosts -= 1;
        }

        Ok(res)
    } else {
        Ok(local_res)
    }
}

/**
This is the deserializer function for cached values. If you do not need deserialization, you can
skip using the `cache_get!` macro and only use [cache_get_value](cache_get_value) to not
deserialize.
 */
pub async fn cache_get_from<'a, T>(value: &'a [u8]) -> Result<Option<T>, CacheError>
where
    T: Debug + serde::Deserialize<'a>,
{
    let res = bincode::deserialize::<T>(value).map_err(|e| CacheError {
        error: format!("Error deserializing cache result: {:?}", e),
    })?;
    Ok(Some(res))
}

/**
Put values into the cache.

This is a direct put, which means that if the cache is running in a HA cluster, the leader
will be ignored and the update will be sent to all participants directly. This could produce conflicts
of course, if 2 hosts modify the same key in the same cache at the exact same time.

However, if you are inserting a newly generated random key for instance, which cannot conflict with
another host, or if you are sure, that this cannot happen, it gives a huge performance and latency
boost compared to `cache_insert`.
 */
pub async fn cache_put<T>(
    cache_name: String,
    entry: String,
    cache_config: &CacheConfig,
    value: &T,
) -> Result<(), CacheError>
where
    T: Debug + serde::Serialize,
{
    let health_state = if let Some(s) = cache_config.rx_health_state.borrow().clone() {
        s.is_quorum_good()?;
        Some(s)
    } else {
        None
    };

    let val = bincode::serialize(value)?;
    let req = CacheReq::Put {
        entry: entry.clone(),
        value: val.clone(),
    };
    let tx = cache_config.cache_map.get(&cache_name).ok_or_else(|| {
        let err = format!(
            "CacheMap misconfiguration - could not find expected cache_name '{}'",
            &cache_name
        );
        error!("{err}");
        CacheError { error: err }
    })?;
    tx.send_async(req).await.map_err(|e| {
        let err = format!("Error sending local cache value over the channel: {:?}", e);
        error!("{err}");
        CacheError { error: err }
    })?;

    if health_state.is_some() {
        let remote_req = RpcRequest::Put {
            cache_name,
            entry,
            value: val,
            resp: None,
        };
        cache_config
            .tx_remote
            .as_ref()
            .unwrap()
            .send_async(remote_req)
            .await?;
    }

    Ok(())
}

/**
The HA pendant to [cache_put](cache_put) - defaults to [cache_put](cache_put) in non-HA mode

This makes Put's safe and conflict free. If the current host is the leader, it will push the cache
operation to all nodes, if it is a Follower, the request will be forwarded to the current leader to
avoid any conflicts, if it could happen, that 2 hosts modify the same key in the same cache at the
exact same time.

The different [AckLevel](AckLevel)'s will provide different levels of safety vs performance.<br>
For instance, if you request an ack from at least "quorum" nodes, it means that the value will be
persisted, even if the cache will end up in split brain mode. However, this will provide the least
amount of performance of course.

If `HA_MODE` is not active, this function will default back to the faster [cache_put](cache_put).
This makes it possible to use `cache_insert` in a HA context with the given [AckLevel](AckLevel)
and just have the direct insert otherwise.
 */
pub async fn cache_insert<T>(
    cache_name: String,
    entry: String,
    cache_config: &CacheConfig,
    value: &T,
    ack_level: AckLevel,
) -> Result<(), CacheError>
where
    T: Debug + serde::Serialize,
{
    if !*HA_MODE {
        return cache_put(cache_name, entry, cache_config, value).await;
    }

    let health_state = if let Some(s) = cache_config.rx_health_state.borrow().clone() {
        s.is_quorum_good()?;
        s
    } else {
        return Err(CacheError {
            error: "'cache_insert' does only work with an active HA_MODE".to_string(),
        });
    };
    let val = bincode::serialize(value)?;
    let mut callback_rx = None;

    // Are we the leader, or is it a remote one?
    match health_state.state {
        QuorumState::Leader => {
            insert_from_leader(
                cache_name,
                entry,
                val,
                cache_config,
                ack_level,
                Some(health_state),
            )
            .await?;
        }
        QuorumState::LeaderDead => {
            // TODO maybe do not return an Err here?
            return Err(CacheError {
                error: "HA Cache is in QuorumState::LeaderDead - cache insert not possible"
                    .to_string(),
            });
        }
        QuorumState::LeaderSwitch => {
            insert_from_leader(
                cache_name,
                entry,
                val,
                cache_config,
                ack_level,
                Some(health_state),
            )
            .await?;
        }
        QuorumState::LeaderTxAwait(_) | QuorumState::LeadershipRequested(_) => {
            // TODO maybe do not return an Err here?
            return Err(CacheError {
                error: "HA Cache has no leader yet - cache insert not possible".to_string(),
            });
        }
        QuorumState::Follower => {
            let (tx, rx) = flume::unbounded();
            callback_rx = Some(rx);

            let req = RpcRequest::Insert {
                cache_name,
                entry,
                value: val,
                ack_level,
                resp: tx,
            };
            health_state
                .tx_leader
                .as_ref()
                .expect(
                    "'health_state.tx_leader' is None in 'cache_insert' when it should never be",
                )
                .send_async(req)
                .await?;
        }
        QuorumState::Undefined => unreachable!(),
        QuorumState::Retry => unreachable!(),
    }

    if let Some(rx) = callback_rx {
        let res = rx.recv_async().await?;
        if res {
            Ok(())
        } else {
            Err(CacheError {
                error: "Could not execute the 'cache_insert'".to_string(),
            })
        }
    } else {
        Ok(())
    }
}

/// The main insert function in case of a configured HA_MODE. Respects the `AckLevel` and and
/// pushes the modifications out to all Followers. Returns a `Result` for the given
/// [AckLevel](AckLevel).
pub(crate) async fn insert_from_leader(
    cache_name: String,
    entry: String,
    value: Vec<u8>,
    cache_config: &CacheConfig,
    ack_level: AckLevel,
    health_state: Option<QuorumHealthState>,
) -> Result<bool, CacheError> {
    debug!("'insert_from_leader' for {}/{}", cache_name, entry);

    if !*HA_MODE {
        let error = "'insert_from_leader' is only available with an active HA_MODE".to_string();
        error!("{error}");
        return Err(CacheError { error });
    }

    // let mut is_leader = false;
    // This is always none, if the request is coming from a remote host and some otherwise
    let health_state = if let Some(health_state) = health_state {
        health_state
    } else {
        let health_state = if let Some(s) = cache_config.rx_health_state.borrow().clone() {
            s.is_quorum_good()?;
            s
        } else {
            return Err(CacheError {
                error: "'insert_from_leader' does only work with an active HA_MODE".to_string(),
            });
        };
        health_state
    };

    // double check, that we are really the leader
    if health_state.state != QuorumState::Leader && health_state.state != QuorumState::LeaderSwitch
    {
        let error = format!("Execution of 'insert_from_leader' is not allowed on a non-leader: {:?}", health_state.state);
        error!("{}", error);
        // TODO we once ended up here during conflict resolution -> try to reproduce
        // rather panic than have an inconsistent state
        panic!("{}", error);
    }

    let tx_cache = cache_config.cache_map.get(&cache_name);
    if tx_cache.is_none() {
        let error = format!("'cache_map' misconfiguration in 'insert_from_leader': The tx for the given cache_name '{}' does not exist", cache_name);
        error!("{error}");
        return Err(CacheError { error });
    }
    let tx_cache = tx_cache.unwrap();

    let await_acks = match ack_level {
        AckLevel::Leader => None,
        AckLevel::Quorum => Some(*QUORUM),
        AckLevel::Once => Some(1),
    };

    if let Some(mut acks) = await_acks {
        let (tx, rx) = flume::unbounded();
        cache_config
            .tx_remote
            .as_ref()
            .unwrap()
            .send_async(RpcRequest::Put {
                cache_name,
                entry: entry.clone(),
                value: value.clone(),
                resp: Some(tx),
            })
            .await?;

        tx_cache.send_async(CacheReq::Put { entry, value }).await?;

        debug!("health_state in 'insert_from_leader': {:?}", health_state);
        let mut clients = health_state.connected_hosts;
        while acks > 0 && clients > 0 {
            match rx.recv_async().await {
                Ok(val) => {
                    if val {
                        acks -= 1;
                    }
                }
                Err(err) => {
                    error!("Error in 'insert_from_leader': {}", err);
                }
            }

            if clients == 1 && acks > 0 {
                // At this point, we have no connected clients left but are still missing acks
                return Ok(false);
            }
            clients -= 1;
        }
    } else {
        let remote = cache_config
            .tx_remote
            .as_ref()
            .unwrap()
            .send_async(RpcRequest::Put {
                cache_name,
                entry: entry.clone(),
                value: value.clone(),
                resp: None,
            });

        let local = tx_cache.send_async(CacheReq::Put { entry, value });

        remote.await?;
        local.await?;
    }

    Ok(true)
}

/**
Deletes a value from the cache. Will return immediately with `Ok(())` if quorum is Bad in `HA_MODE`.
Does ignore the quorum health state, if the cache is running in HA mode.
 */
pub async fn cache_del(
    cache_name: String,
    entry: String,
    cache_config: &CacheConfig,
) -> Result<(), CacheError> {
    let tx = cache_config.cache_map.get(&cache_name).ok_or_else(|| {
        let err = format!(
            "CacheMap misconfiguration - could not find expected cache_name '{}'",
            &cache_name
        );
        error!("{err}");
        CacheError { error: err }
    })?;

    let req = CacheReq::Del {
        entry: entry.clone(),
    };
    tx.send_async(req).await.map_err(|e| {
        let err = format!("Error sending local cache value over the channel: {:?}", e);
        error!("{err}");
        CacheError { error: err }
    })?;

    if *HA_MODE {
        let remote_req = RpcRequest::Del {
            cache_name,
            entry,
            resp: None,
        };
        cache_config
            .tx_remote
            .as_ref()
            .unwrap()
            .send_async(remote_req)
            .await?;
    }

    Ok(())
}

/**
The HA pendant to [cache_del](cache_del) - defaults to [cache_del](cache_del) in non-HA mode

This is the HA version of [cache_del](cache_del). It works like [cache_insert](cache_insert), just
for deletions. Values are removed from the cache and requests are routed over the current leader to
avoid possible conflicts. This is of course less performing than the direct [cache_del](cache_del),
which should be favored, if this works out for you.

If `HA_MODE` is not active, this function will default back to the faster [cache_del](cache_del).
This makes it possible to use `cache_remove` in a HA context with the given [AckLevel](AckLevel)
and just have the direct delete otherwise.
 */
pub async fn cache_remove(
    cache_name: String,
    entry: String,
    cache_config: &CacheConfig,
    ack_level: AckLevel,
) -> Result<(), CacheError> {
    if !*HA_MODE {
        return cache_del(cache_name, entry, cache_config).await;
    }

    let health_state = if let Some(s) = cache_config.rx_health_state.borrow().clone() {
        s.is_quorum_good()?;
        s
    } else {
        return Err(CacheError {
            error: "'cache_remove' does only work with an active HA_MODE".to_string(),
        });
    };

    let mut callback_rx = None;

    // Are we the leader, or is it a remote one?
    match health_state.state {
        QuorumState::Leader => {
            remove_from_leader(
                cache_name,
                entry,
                cache_config,
                ack_level,
                Some(health_state),
            )
            .await?;
        }
        QuorumState::LeaderDead => {
            // TODO maybe do not return an Err here and rename to try_...?
            return Err(CacheError {
                error: "HA Cache is in QuorumState::LeaderDead - cache insert not possible"
                    .to_string(),
            });
        }
        QuorumState::LeaderSwitch => {
            remove_from_leader(
                cache_name,
                entry,
                cache_config,
                ack_level,
                Some(health_state),
            )
            .await?;
        }
        QuorumState::LeaderTxAwait(_) | QuorumState::LeadershipRequested(_) => {
            // TODO maybe do not return an Err here and rename to try_...?
            return Err(CacheError {
                error: "HA Cache has no leader yet - cache insert not possible".to_string(),
            });
        }
        QuorumState::Follower => {
            let (tx, rx) = flume::unbounded();
            callback_rx = Some(rx);

            let req = RpcRequest::Remove {
                cache_name,
                entry,
                ack_level,
                resp: tx,
            };
            health_state
                .tx_leader
                .as_ref()
                .expect(
                    "'health_state.tx_leader' is None in 'cache_insert' when it should never be",
                )
                .send_async(req)
                .await?;
        }
        QuorumState::Undefined => unreachable!(),
        QuorumState::Retry => unreachable!(),
    }

    if let Some(rx) = callback_rx {
        let res = rx.recv_async().await?;
        if res {
            Ok(())
        } else {
            Err(CacheError {
                error: "Could not execute the 'cache_insert'".to_string(),
            })
        }
    } else {
        Ok(())
    }
}

/// The main remove function in case of a configured HA_MODE. Respects the `AckLevel` and and
/// pushes the modifications out to all Followers. Returns a `Result` for the given `AckLevel`.
pub(crate) async fn remove_from_leader(
    cache_name: String,
    entry: String,
    cache_config: &CacheConfig,
    ack_level: AckLevel,
    health_state: Option<QuorumHealthState>,
) -> Result<bool, CacheError> {
    debug!("'remove_from_leader' for {}/{}", cache_name, entry);

    if !*HA_MODE {
        let error = "'remove_from_leader' is only available with an active HA_MODE".to_string();
        error!("{error}");
        return Err(CacheError { error });
    }

    // let mut is_leader = false;
    // This is always none, if the request is coming from a remote host and some otherwise
    let health_state = if let Some(health_state) = health_state {
        health_state
    } else {
        let health_state = if let Some(s) = cache_config.rx_health_state.borrow().clone() {
            s.is_quorum_good()?;
            s
        } else {
            return Err(CacheError {
                error: "'remove_from_leader' does only work with an active HA_MODE".to_string(),
            });
        };
        health_state
    };

    // double check, that we are really the leader
    // this might get removed after enough testing, if it provides a performance benefit
    if health_state.state != QuorumState::Leader && health_state.state != QuorumState::LeaderSwitch
    {
        let error = "Execution of 'remove_from_leader' is not allowed on a non-leader".to_string();
        warn!("is_leader state: {:?}", health_state.state);
        // TODO remove this panic after enough testing
        panic!("{}", error);
    }

    let tx_cache = cache_config.cache_map.get(&cache_name);
    if tx_cache.is_none() {
        let error = format!("'cache_map' misconfiguration in 'remove_from_leader': The tx for the given cache_name '{}' does not exist", cache_name);
        error!("{error}");
        return Err(CacheError { error });
    }
    let tx_cache = tx_cache.unwrap();

    let await_acks = match ack_level {
        AckLevel::Leader => None,
        AckLevel::Quorum => Some(*QUORUM),
        AckLevel::Once => Some(1),
    };

    if let Some(mut acks) = await_acks {
        let (tx, rx) = flume::unbounded();
        cache_config
            .tx_remote
            .as_ref()
            .unwrap()
            .send_async(RpcRequest::Del {
                cache_name,
                entry: entry.clone(),
                resp: Some(tx),
            })
            .await?;

        tx_cache.send_async(CacheReq::Del { entry }).await?;

        debug!("health_state in 'remove_from_leader': {:?}", health_state);
        let mut clients = health_state.connected_hosts;
        while acks > 0 && clients > 0 {
            match rx.recv_async().await {
                Ok(val) => {
                    if val {
                        acks -= 1;
                    }
                }
                Err(err) => {
                    error!("Error in 'remove_from_leader': {}", err);
                }
            }

            if clients == 1 && acks > 0 {
                // At this point, we have no connected clients left but are still missing acks
                return Ok(false);
            }
            clients -= 1;
        }
    } else {
        let remote = cache_config
            .tx_remote
            .as_ref()
            .unwrap()
            .send_async(RpcRequest::Del {
                cache_name,
                entry: entry.clone(),
                resp: None,
            });

        let local = tx_cache.send_async(CacheReq::Del { entry });

        remote.await?;
        local.await?;
    }

    Ok(true)
}

fn get_local_hostname() -> String {
    match env::var("HOSTNAME_OVERWRITE") {
        Ok(hostname) => hostname,
        Err(_) => {
            let hostname_os = gethostname();
            hostname_os
                .to_str()
                .expect("Error getting the hostname from the OS")
                .to_string()
        }
    }
}

/// Used internally to generate request ids for cache operations in HA mode.
/// Currently, only 10 characters, since conflicts are extremely unlikely. These ids will not live
/// much longer than a millisecond mostly.
pub(crate) fn get_cache_req_id() -> String {
    // 10 characters is probably way more than enough if taken into account, that these IDs do live
    // for only a few ms each time at most.
    // TODO observe and maybe tune this value - maybe add config var for slow networks
    // TODO We could change to ULIDs or even an incrementing counter too, which could be way faster
    nanoid!(10)
}

pub(crate) fn get_rand_between(start: u64, end: u64) -> u64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(start..end)
}

/// Clears all cache entries of each existing cache.
/// This full reset is local and will not be pushed to possible remote HA cache members.
pub async fn clear_caches(cache_config: &CacheConfig) -> Result<(), CacheError> {
    for (name, tx) in &cache_config.cache_map {
        debug!("Clearing cache {}", name);
        tx.send_async(CacheReq::Reset).await?;
    }
    Ok(())
}

/**
The main function to start the whole backend when `HA_MODE` == true. It cares about starting the
server and one client for each remote server configured in `HA_HOSTS`, as well as the internal
'quorum_handler'.

# Panics
- If a bad [CacheConfig](CacheConfig) was given, in which the channels needed for HA communication
are `None`. This cannot happen, if [CacheConfig::new](CacheConfig::new) is used for the
initialization.
- If `HA_HOSTS` has a bad format and if the hostname of the current instance does not appear in it
and IP addresses are used instead. Currently, the cache instances must contain their hostnames
in the DNS address somehow. This function executes a `.contains` on the CSV to decide, which of
the hosts in the list this very instance is.<br>
You can overwrite the current hostname to make this check pass with the `HOSTNAME_OVERWRITE` env var.
 */
pub async fn start_cluster(
    tx_watch: watch::Sender<Option<QuorumHealthState>>,
    cache_config: &mut CacheConfig,
    tx_notify: Option<mpsc::Sender<CacheNotify>>,
    hostname_overwrite: Option<String>,
) -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    if !*HA_MODE {
        info!("HA_MODE is not set, starting in standalone mode");
        return Ok(());
    }

    // get all participating hosts
    let mut ha_clients = vec![];
    let mut host_srv_addr = String::default();

    let hostname = if let Some(name) = hostname_overwrite {
        name
    } else {
        get_local_hostname()
    };
    // the value is read in again and does hot reuse `HA_HOSTS` static ref because of easier testing
    let ha_hosts_csv = env::var("HA_HOSTS").expect("HA_HOSTS is not set");
    if !ha_hosts_csv.contains(&hostname) {
        panic!(
            "HA_HOSTS is not set up correctly. Current hostname '{}' does not appear in HA_HOSTS",
            hostname
        );
    }
    let ha_hosts = ha_hosts_csv
        .split(',')
        .map(|h| h.trim().to_string())
        .collect::<Vec<String>>();

    ha_hosts.iter().for_each(|h| {
        if !h.contains(&hostname) {
            ha_clients.push(h.trim().to_owned())
        } else {
            h.trim().clone_into(&mut host_srv_addr);
        }
    });
    info!(
        "Starting HA cache for hostname '{}' and cache members: {:?}",
        hostname, ha_clients
    );

    let tx_quorum = cache_config.tx_quorum.as_ref().unwrap().to_owned();

    // start up quorum handler
    let cache_map = cache_config.cache_map.clone();
    let quorum_handle = tokio::spawn(quorum_handler(
        tx_quorum.clone(),
        tx_watch,
        cache_config.rx_quorum.as_ref().unwrap().clone(),
        cache_config.tx_remote.as_ref().unwrap().clone(),
        ha_clients.len(),
        cache_map,
        host_srv_addr.clone(),
    ));

    // start up the server
    let srv_handle = tokio::spawn(RpcCacheService::serve(
        host_srv_addr,
        cache_config.clone(),
        tx_quorum.clone(),
        tx_notify,
    ));

    // start up all the clients
    let rx_remote = cache_config.rx_remote.as_ref().unwrap().clone();
    let clients_handle = tokio::spawn(cache_clients(ha_clients, tx_quorum, rx_remote));

    // start a ping handler to keep up to date RTT's for each connection
    // this ping handler prevents broken pipe's on some K8s setups
    let tx_remote = cache_config.tx_remote.as_ref().unwrap().clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(err) = tx_remote.send_async(RpcRequest::Ping).await {
                error!("rpc stream ping handler: {:?}", err);
            }
        }
    });

    let (tx_exit, rx_exit) = flume::unbounded();
    cache_config.tx_exit = Some(tx_exit);

    // exit signal handler
    let tx_quorum = cache_config
        .tx_quorum
        .clone()
        .expect("cache_config.tx_quorum to never be empty here");
    tokio::spawn(async move {
        // wait for the shutdown signal
        let tx_ack = rx_exit
            .recv_async()
            .await
            .expect("No tx_ack given for cache exit channel");

        // send LeaderLeave, if we are the current cluster leader
        tx_quorum.send_async(QuorumReq::HostShutdown).await.unwrap();

        // Do a short sleep to make sure messages were sent out
        time::sleep(Duration::from_millis(2000)).await;

        srv_handle.abort();
        clients_handle.abort();
        quorum_handle.abort();

        tx_ack.send(()).unwrap();
        // Do a short sleep to make sure messages were sent out
        time::sleep(Duration::from_millis(10)).await;
    });

    Ok(())
}

/**
The main function to start up a new cache handler.<br />
- `cache` accepts any of the Cache structs from the [Cached](https://crates.io/crates/cached) crate.
This means, if you need caches with different configs, just start up a new `cache_recv`.<br />
- `name` is only used for logging and debugging.
- `rx` for [CacheReq](CacheReq)s
 */
pub(crate) async fn cache_recv<C>(mut cache: C, name: String, rx: flume::Receiver<CacheReq>)
where
    C: Cached<String, Vec<u8>>,
{
    info!("Started cache {}", name);
    loop {
        let req_opt = rx.recv_async().await;
        if req_opt.is_err() {
            warn!(
                "Received None in cache_recv {} - Cache Sender has been dropped - exiting cache",
                name
            );
            break;
        }

        match req_opt.unwrap() {
            CacheReq::Get { entry, resp } => {
                let cache_entry = cache.cache_get(&entry).cloned();
                resp.send_async(cache_entry)
                    .await
                    .map_err(|_| error!("Error sending cache entry '{}' back", entry))
                    .unwrap();
            }
            CacheReq::Put { entry, value } => {
                cache.cache_set(entry, value);
            }
            CacheReq::Del { entry } => {
                cache.cache_remove(&entry);
            }
            CacheReq::Reset => {
                debug!("Received a full cache reset request");
                cache.cache_reset();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use pretty_assertions::assert_eq;

    use super::*;

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct Cert {
        key: String,
        crt: String,
        id: i64,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct BincodeTester {
        id: i64,
        name: String,
        opt_name: Option<String>,
        some_vec: Vec<u32>,
        some_opt_vec: Option<Vec<String>>,
    }

    /// Sets up the logging / tracing depending on the env var `LOG_LEVEL`
    pub fn setup_logging() {
        use tracing::Level;

        let log_level = Level::INFO;
        let filter = format!("{},async_nats=info,hyper=info", log_level.as_str());
        env::set_var("RUST_LOG", &filter);

        let subscriber = tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(log_level)
            .with_env_filter(filter)
            .finish();

        let _ = tracing::subscriber::set_default(subscriber);
    }

    #[tokio::test]
    async fn test_cache_simple() -> Result<(), Box<dyn std::error::Error>> {
        setup_logging();

        let (_rx_quorum, mut cache_config) = CacheConfig::new();

        let cache_name_1 = "one".to_string();
        cache_config.spawn_cache(cache_name_1.clone(), SizedCache::with_size(5), None);

        let s1 = BincodeTester {
            id: 123,
            name: "SuperTester".to_string(),
            opt_name: None,
            some_vec: vec![1, 33, 20098],
            some_opt_vec: None,
        };

        let entry = "e1".to_string();
        cache_put(cache_name_1.clone(), entry.clone(), &cache_config, &s1)
            .await
            .unwrap();

        let res_opt = cache_get!(
            BincodeTester,
            cache_name_1.clone(),
            entry.clone(),
            &cache_config,
            false
        )
        .await
        .expect("Cache error");
        assert!(res_opt.is_some());
        let res = res_opt.unwrap();

        assert_eq!(res.id, s1.id);
        assert_eq!(res.name, s1.name);
        assert_eq!(res.opt_name, s1.opt_name);
        assert_eq!(res.some_vec, s1.some_vec);
        assert_eq!(res.some_opt_vec, s1.some_opt_vec);

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_recv_sized() -> Result<(), Box<dyn std::error::Error>> {
        setup_logging();

        let (_rx_quorum, mut cache_config) = CacheConfig::new();

        let cache_name_1 = "two".to_string();
        cache_config.spawn_cache(cache_name_1.clone(), SizedCache::with_size(5), Some(16));

        cache_put(cache_name_1.clone(), "1".to_string(), &cache_config, &1i32)
            .await
            .unwrap();
        cache_put(
            cache_name_1.clone(),
            "2".to_string(),
            &cache_config,
            &999i32,
        )
        .await
        .unwrap();
        // "2" should be overridden now
        cache_put(cache_name_1.clone(), "2".to_string(), &cache_config, &17i32)
            .await
            .unwrap();
        cache_put(
            cache_name_1.clone(),
            "3".to_string(),
            &cache_config,
            &1337i64,
        )
        .await
        .unwrap();
        cache_put(
            cache_name_1.clone(),
            "4".to_string(),
            &cache_config,
            &2887398i64,
        )
        .await
        .unwrap();

        let crt1 = Cert {
            key: "SomeKey1".to_string(),
            crt: "SomeVerySecretCert1".to_string(),
            id: 1,
        };
        let crt2 = Cert {
            key: "SomeKey2WhichIsVeeeeeeeeeeryyyyyyLooooooong".to_string(),
            crt: "SomeVerySecretCert2".to_string(),
            id: 2,
        };
        cache_put(
            cache_name_1.clone(),
            "Cert1".to_string(),
            &cache_config,
            &crt1,
        )
        .await
        .unwrap();
        cache_put(
            cache_name_1.clone(),
            "Cert2".to_string(),
            &cache_config,
            &crt2,
        )
        .await
        .unwrap();

        let one = cache_get!(
            i32,
            cache_name_1.clone(),
            "1".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let two = cache_get!(
            i32,
            cache_name_1.clone(),
            "2".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let three = cache_get!(
            i64,
            cache_name_1.clone(),
            "3".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let four = cache_get!(
            i64,
            cache_name_1.clone(),
            "4".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();

        let crt_recv1 = cache_get!(
            Cert,
            cache_name_1.clone(),
            "Cert1".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let crt_recv2 = cache_get!(
            Cert,
            cache_name_1.clone(),
            "Cert2".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();

        assert!(one.is_none());
        assert_eq!(two.unwrap(), 17);
        assert_eq!(three.unwrap(), 1337);
        assert_eq!(four.unwrap(), 2887398);

        assert_eq!(crt_recv1.unwrap().crt, crt1.crt);
        assert_eq!(crt_recv2.unwrap().crt, crt2.crt);

        cache_del(cache_name_1.clone(), "2".to_string(), &cache_config)
            .await
            .unwrap();
        cache_del(cache_name_1.clone(), "Cert2".to_string(), &cache_config)
            .await
            .unwrap();
        let two = cache_get!(
            i32,
            cache_name_1.clone(),
            "2".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let crt_recv2 = cache_get!(
            Cert,
            cache_name_1.clone(),
            "Cert2".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        assert!(two.is_none());
        assert!(crt_recv2.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_recv_sized_timed() -> Result<(), Box<dyn std::error::Error>> {
        setup_logging();

        let (_rx_quorum, mut cache_config) = CacheConfig::new();

        let cache_name_1 = "three".to_string();
        cache_config.spawn_cache(
            cache_name_1.clone(),
            TimedSizedCache::with_size_and_lifespan(2, 1),
            None,
        );

        cache_put(cache_name_1.clone(), "1".to_string(), &cache_config, &1i64)
            .await
            .unwrap();
        cache_put(cache_name_1.clone(), "2".to_string(), &cache_config, &2i64)
            .await
            .unwrap();
        cache_put(cache_name_1.clone(), "3".to_string(), &cache_config, &3i64)
            .await
            .unwrap();

        let one = cache_get!(
            i64,
            cache_name_1.clone(),
            "1".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let two = cache_get!(
            i64,
            cache_name_1.clone(),
            "2".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap()
        .unwrap();
        let three = cache_get!(
            i64,
            cache_name_1.clone(),
            "3".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap()
        .unwrap();

        assert!(one.is_none());
        assert_eq!(two, 2);
        assert_eq!(three, 3);

        // after 1 second, the values should be gone
        time::sleep(Duration::from_secs(1)).await;

        let one = cache_get!(
            Cert,
            cache_name_1.clone(),
            "1".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let two = cache_get!(
            Cert,
            cache_name_1.clone(),
            "2".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let three = cache_get!(
            Cert,
            cache_name_1.clone(),
            "3".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();

        assert!(one.is_none());
        assert!(two.is_none());
        assert!(three.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_recv_timed() -> Result<(), Box<dyn std::error::Error>> {
        setup_logging();

        let (_rx_quorum, mut cache_config) = CacheConfig::new();

        let cache_name_1 = "four".to_string();
        cache_config.spawn_cache(cache_name_1.clone(), TimedCache::with_lifespan(1), None);

        cache_put(cache_name_1.clone(), "1".to_string(), &cache_config, &1i32)
            .await
            .unwrap();
        cache_put(cache_name_1.clone(), "2".to_string(), &cache_config, &2i32)
            .await
            .unwrap();
        cache_put(cache_name_1.clone(), "3".to_string(), &cache_config, &3i32)
            .await
            .unwrap();

        let one = cache_get!(
            i32,
            cache_name_1.clone(),
            "1".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let two = cache_get!(
            i32,
            cache_name_1.clone(),
            "2".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let three = cache_get!(
            i32,
            cache_name_1.clone(),
            "3".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();

        assert_eq!(one, Some(1));
        assert_eq!(two, Some(2));
        assert_eq!(three, Some(3));

        // after 1 second, the values should be gone
        time::sleep(Duration::from_secs(1)).await;

        let one = cache_get!(
            i32,
            cache_name_1.clone(),
            "1".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let two = cache_get!(
            i32,
            cache_name_1.clone(),
            "2".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();
        let three = cache_get!(
            i32,
            cache_name_1.clone(),
            "3".to_string(),
            &cache_config,
            false
        )
        .await
        .unwrap();

        assert!(one.is_none());
        assert!(two.is_none());
        assert!(three.is_none());

        Ok(())
    }

    // IMPORTANT:
    // This test cannot run together with the others, since it sets HA_MODE env var, which will make
    // the others fail. This means 'cargo test' just all does not work, but you need 2 targets
    // instead and target all HA_MODE and non-HA_MODE tests together.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn test_ha_cache() -> anyhow::Result<()> {
        setup_logging();

        env::set_var("HA_MODE", "true");
        env::set_var(
            "HA_HOSTS",
            "http://127.0.0.1:7001, http://127.0.0.1:7002, http://127.0.0.1:7003",
        );
        env::set_var("CACHE_AUTH_TOKEN", "SuperSecretToken1337");
        env::set_var("CACHE_TLS", "false");

        let (tx_health_1, mut cache_config_1) = CacheConfig::new();
        let (tx_health_2, mut cache_config_2) = CacheConfig::new();
        let (tx_health_3, mut cache_config_3) = CacheConfig::new();

        let cache_name = "c_one".to_string();
        let cache = SizedCache::with_size(16);
        cache_config_1.spawn_cache(cache_name.clone(), cache.clone(), Some(16));
        cache_config_2.spawn_cache(cache_name.clone(), cache.clone(), None);
        cache_config_3.spawn_cache(cache_name.clone(), cache, None);

        // start server 1
        start_cluster(
            tx_health_1,
            &mut cache_config_1,
            None,
            Some("127.0.0.1:7001".to_string()),
        )
        .await?;

        // sleep shortly to have a faster leader election and always the same one
        time::sleep(Duration::from_millis(100)).await;

        // test PUT on cache 1 -> should fail because of no quorum
        let entry = "one".to_string();
        let res = cache_put(cache_name.clone(), entry.clone(), &cache_config_1, &1i32).await;
        match res {
            Ok(_) => panic!("This should not be Ok"),
            Err(err) => {
                assert!(err.error.contains("QuorumHealth::Bad"));
            }
        }
        // double check that not value has been inserted
        let one = cache_get!(
            i32,
            cache_name.clone(),
            entry.clone(),
            &cache_config_1,
            false
        )
        .await
        .unwrap();
        assert_eq!(one, None);

        // start server 2
        start_cluster(
            tx_health_2,
            &mut cache_config_2,
            None,
            Some("127.0.0.1:7002".to_string()),
        )
        .await?;
        // with the short sleep from before, server 2 should now always be the leader

        // wait for the quorum to be good
        let mut loops = 0;
        while cache_config_1
            .rx_health_state
            .borrow()
            .as_ref()
            .unwrap()
            .health
            == QuorumHealth::Bad
        {
            loops += 1;
            time::sleep(Duration::from_secs(1)).await;
            if loops > 20 {
                panic!("QuorumHealth did no reach Good state when it should have");
            }
        }

        // test PUT on cache 2 -> should succeed because of good quorum
        let entry = "one".to_string();
        cache_put(cache_name.clone(), entry.clone(), &cache_config_1, &1337i32)
            .await
            .unwrap();
        // check the value
        let one = cache_get!(
            i32,
            cache_name.clone(),
            entry.clone(),
            &cache_config_1,
            false
        )
        .await
        .unwrap();
        assert_eq!(one, Some(1337));
        // should be the same for cache 2 - give it a moment and check
        time::sleep(Duration::from_millis(5)).await;
        let two = cache_get!(
            i32,
            cache_name.clone(),
            entry.clone(),
            &cache_config_2,
            false
        )
        .await
        .unwrap();
        assert_eq!(two, Some(1337));

        // start server 3
        start_cluster(
            tx_health_3,
            &mut cache_config_3,
            None,
            Some("127.0.0.1:7003".to_string()),
        )
        .await?;

        // wait for the quorum to be good
        let mut loops = 0;
        while cache_config_3
            .rx_health_state
            .borrow()
            .as_ref()
            .unwrap()
            .health
            == QuorumHealth::Bad
        {
            loops += 1;
            time::sleep(Duration::from_secs(1)).await;
            if loops > 20 {
                panic!("QuorumHealth did no reach Good state when it should have");
            }
        }
        // The 3rd member can only be a follow at this point, since the other 2 have already established
        // quorum and elected a leader (most possibly host 2)
        assert_eq!(
            cache_config_3
                .rx_health_state
                .borrow()
                .as_ref()
                .unwrap()
                .state,
            QuorumState::Follower
        );

        // make sure all clients have 2 connections
        while cache_config_1
            .rx_health_state
            .borrow()
            .as_ref()
            .unwrap()
            .connected_hosts
            < 2
            || cache_config_2
                .rx_health_state
                .borrow()
                .as_ref()
                .unwrap()
                .connected_hosts
                < 2
            || cache_config_3
                .rx_health_state
                .borrow()
                .as_ref()
                .unwrap()
                .connected_hosts
                < 2
        {
            time::sleep(Duration::from_secs(1)).await;
        }

        // test GET on cache 3 without remote lookup, which should not have the value, since it was
        // started after the PUT
        let three = cache_get!(
            i32,
            cache_name.clone(),
            entry.clone(),
            &cache_config_3,
            false
        )
        .await
        .unwrap();
        assert_eq!(three, None);
        // now with remote lookup
        let three = cache_get!(
            i32,
            cache_name.clone(),
            entry.clone(),
            &cache_config_3,
            true
        )
        .await
        .unwrap();
        assert_eq!(three, Some(1337));

        // test DEL
        cache_del(cache_name.clone(), entry.clone(), &cache_config_1)
            .await
            .unwrap();
        // verify the value is gone
        let one = cache_get!(
            i32,
            cache_name.clone(),
            entry.clone(),
            &cache_config_1,
            false
        )
        .await
        .unwrap();
        assert_eq!(one, None);
        // give it a moment and check the other caches too
        time::sleep(Duration::from_millis(1)).await;
        let two = cache_get!(
            i32,
            cache_name.clone(),
            entry.clone(),
            &cache_config_2,
            false
        )
        .await
        .unwrap();
        assert_eq!(two, None);
        let three = cache_get!(
            i32,
            cache_name.clone(),
            entry.clone(),
            &cache_config_3,
            false
        )
        .await
        .unwrap();
        assert_eq!(three, None);

        // test HA insert
        let ha_entry = "ha_entry".to_string();
        let ha_val = "HaVal1337".to_string();
        cache_insert(
            cache_name.clone(),
            ha_entry.clone(),
            &cache_config_1,
            &ha_val,
            AckLevel::Quorum,
        )
        .await
        .unwrap();
        // verify the value exists
        time::sleep(Duration::from_millis(20)).await;
        let one = cache_get!(
            String,
            cache_name.clone(),
            ha_entry.clone(),
            &cache_config_1,
            false
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(one, ha_val);
        let two = cache_get!(
            String,
            cache_name.clone(),
            ha_entry.clone(),
            &cache_config_2,
            false
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(two, ha_val);
        let three = cache_get!(
            String,
            cache_name.clone(),
            ha_entry.clone(),
            &cache_config_3,
            false
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(three, ha_val);

        // test HA remove
        cache_remove(
            cache_name.clone(),
            entry.clone(),
            &cache_config_1,
            AckLevel::Quorum,
        )
        .await
        .unwrap();
        // verify the value exists
        time::sleep(Duration::from_millis(20)).await;
        let one = cache_get!(
            String,
            cache_name.clone(),
            entry.clone(),
            &cache_config_1,
            false
        )
        .await
        .unwrap();
        assert_eq!(one, None);
        let two = cache_get!(
            String,
            cache_name.clone(),
            entry.clone(),
            &cache_config_2,
            false
        )
        .await
        .unwrap();
        assert_eq!(two, None);
        let three = cache_get!(
            String,
            cache_name.clone(),
            entry.clone(),
            &cache_config_3,
            false
        )
        .await
        .unwrap();
        assert_eq!(three, None);

        Ok(())
    }
}
