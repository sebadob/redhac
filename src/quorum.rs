use std::collections::HashMap;
use std::env;
use std::time::Duration;

use chrono::Utc;
use lazy_static::lazy_static;
use tokio::sync::{oneshot, watch};
use tokio::time;
use tracing::{debug, error, info, warn};

use crate::{CacheError, CacheReq, get_rand_between, rpc};
use crate::client::{RECONNECT_TIMEOUT_UPPER, RpcRequest};
use crate::rpc::cache;
use crate::server::CacheMap;

lazy_static! {
    static ref ELECTION_TIMEOUT: u64 = {
        let t = env::var("CACHE_ELECTION_TIMEOUT")
            .unwrap_or_else(|_| String::from("2"))
            .parse::<u64>()
            .expect("Error parsing 'CACHE_ELECTION_TIMEOUT' to u64");
        t * 1000
    };
}

/// The `AckLevel` is a QoS indicator for HA cache modifying operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AckLevel {
    Quorum,
    Once,
    Leader,
}

impl AckLevel {
    pub fn get_rpc_value(&self) -> rpc::cache::AckLevel {
        match self {
            AckLevel::Quorum => cache::AckLevel {
                ack_level: Some(cache::ack_level::AckLevel::LevelQuorum(cache::Empty {})),
            },
            AckLevel::Once => cache::AckLevel {
                ack_level: Some(cache::ack_level::AckLevel::LevelOnce(cache::Empty {})),
            },
            AckLevel::Leader => cache::AckLevel {
                ack_level: Some(cache::ack_level::AckLevel::LevelLeader(cache::Empty {})),
            },
        }
    }

    pub fn from_rpc_value(value: Option<cache::AckLevel>) -> Self {
        match value.unwrap().ack_level.unwrap() {
            cache::ack_level::AckLevel::LevelQuorum(_) => Self::Quorum,
            cache::ack_level::AckLevel::LevelOnce(_) => Self::Once,
            cache::ack_level::AckLevel::LevelLeader(_) => Self::Leader,
        }
    }
}

/// Only used with `HA_MODE` == true
#[derive(Debug, Clone, Default)]
pub struct QuorumHealthState {
    pub health: QuorumHealth,
    pub state: QuorumState,
    pub tx_leader: Option<flume::Sender<RpcRequest>>,
    pub connected_hosts: usize,
}

impl QuorumHealthState {
    pub fn is_quorum_good(&self) -> Result<(), CacheError> {
        match self.health {
            QuorumHealth::Good => Ok(()),
            QuorumHealth::Degraded => Ok(()),
            QuorumHealth::Bad => Err(CacheError {
                error: "QuorumHealth::Bad - cannot operate the HA cache".to_string(),
            }),
        }
    }
}

/// Indicator if the QuorumHealth is Good or Bad. Only used with `HA_MODE` == true
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QuorumHealth {
    Good,
    Degraded,
    Bad,
}

impl Default for QuorumHealth {
    fn default() -> Self {
        Self::Bad
    }
}

/// Returns a full health state. Only used with `HA_MODE` == true
#[derive(Debug, Clone, PartialEq)]
pub struct QuorumReport {
    pub health: QuorumHealth,
    pub state: QuorumState,
    pub leader: Option<RegisteredLeader>,
    pub hosts: Vec<RpcServer>,
}

/// If this host is a cache Leader, Follower, or in Undefined state
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QuorumState {
    Leader,
    LeaderDead,
    LeaderSwitch,
    LeaderTxAwait(String),
    LeadershipRequested(i64),
    Follower,
    Undefined,
    Retry,
}

impl Default for QuorumState {
    fn default() -> Self {
        Self::Undefined
    }
}

/// Enum for the communication between all necessary component when `HA_MODE` == true.
/// `Print` prints out information about the current quorum and connected clients / servers.
#[derive(Debug)]
pub(crate) enum QuorumReq {
    UpdateServer {
        server: RpcServer,
    },
    GetReport {
        tx: oneshot::Sender<QuorumReport>,
    },
    LeaderInfo {
        addr: String,
        election_ts: i64,
        has_quorum: bool,
    },
    LeaderReq {
        req: cache::LeaderReq,
        tx: oneshot::Sender<bool>,
    },
    LeaderReqAck {
        addr: String,
        election_ts: i64,
    },
    LeaderAck {
        ack: cache::LeaderAck,
    },
    LeaderSwitch {
        req: cache::LeaderSwitch,
    },
    LeaderSwitchDead {
        req: cache::LeaderDead,
    },
    LeaderSwitchAck {
        req: cache::LeaderSwitchAck,
    },
    LeaderSwitchPriority {
        req: cache::LeaderSwitchPriority,
    },
    CheckElectionTimeout,
    HostShutdown,
    // dead code inside this lib, but may be used from the outside to trigger a status log print
    #[allow(dead_code)]
    Print,
}

/// The registered Leader inside the quorum handler
#[derive(Debug, Clone, PartialEq)]
pub enum RegisteredLeader {
    Local(i64),
    Remote(RpcServer),
}

/// Is used for the communication with the internal 'quorum_handler' if a new client connects
/// to the server.
#[derive(Debug, Clone)]
pub struct RpcServer {
    pub address: String,
    pub state: RpcServerState,
    pub tx: Option<flume::Sender<RpcRequest>>,
    pub election_ts: i64,
}

impl PartialEq for RpcServer {
    fn eq(&self, other: &Self) -> bool {
        other.address == self.address
    }
}

/// State for new cache clients if they are `Alive` or `Dead`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcServerState {
    Alive,
    Dead,
}

/**
Handles all 'quorum related things'.
The 3 main cache functions [cache_get!](cache_get!), [cache_put](cache_put), [cache_del](cache_del)
check back with this handler every time, if quorum is Good or Bad.
Additionally, this handler can print out the current state of Quorum and connected clients.

The most important thing it does is probably the cache invalidation, if the Quorum went from `Good`
to `Bad`. Every value is deleted from the local cache, if the instance lost Quorum, to make sure
no cached values are used which were probably invalidated somewhere else already.
 */
pub(crate) async fn quorum_handler(
    tx_quorum: flume::Sender<QuorumReq>,
    tx_watch: watch::Sender<Option<QuorumHealthState>>,
    rx: flume::Receiver<QuorumReq>,
    tx_remote: flume::Sender<RpcRequest>,
    clients_count: usize,
    cache_map: CacheMap,
    whoami: String,
) -> anyhow::Result<()> {
    let quorum = (clients_count + 1) / 2;

    let mut hosts = HashMap::with_capacity(clients_count);
    let mut health = QuorumHealth::Bad;
    let mut state = QuorumState::Undefined;
    let mut health_state = QuorumHealthState {
        health: QuorumHealth::Bad,
        state: QuorumState::Undefined,
        tx_leader: None,
        connected_hosts: 0,
    };
    let mut leader: Option<RegisteredLeader> = None;
    let mut ack_count = 0;

    info!(
        "HA Cache Cluster quorum handler is running. Quorum needed for a healthy cluster: {}",
        quorum + 1
    );

    loop {
        debug!("loop iteration in quorum_handler");

        let req = rx.recv_async().await;
        if req.is_err() {
            warn!("Received 'None' in 'quorum_handler' - Exiting");
            break;
        }

        match req.unwrap() {
            // will be called if a client has established or lost the connection to a remote server
            QuorumReq::UpdateServer { server } => {
                match server.state {
                    RpcServerState::Alive => {
                        info!("Cache client connected successfully: {}", server.address);

                        if let QuorumState::LeaderTxAwait(addr) = &state {
                            if addr == &server.address {
                                let election_ts = leader
                                    .map(|l| match l {
                                        RegisteredLeader::Local(ts) => ts,
                                        RegisteredLeader::Remote(r) => r.election_ts,
                                    })
                                    .unwrap();
                                let ack = RpcRequest::LeaderReqAck {
                                    addr: addr.clone(),
                                    election_ts,
                                };
                                if let Some(tx) = &server.tx {
                                    if let Err(err) = tx.send_async(ack).await {
                                        error!("{:?}", CacheError::from(&err));
                                    }
                                }

                                leader = Some(RegisteredLeader::Remote(RpcServer {
                                    address: addr.clone(),
                                    state: RpcServerState::Alive,
                                    tx: server.tx.clone(),
                                    election_ts,
                                }));

                                state = QuorumState::Follower;
                                health_state.state = QuorumState::Follower;
                                health_state.tx_leader.clone_from(&server.tx);
                            }
                        }

                        if state == QuorumState::Retry && leader.is_some() {
                            let lead = leader.clone().unwrap();
                            match lead {
                                RegisteredLeader::Local(_) => {
                                    unreachable!(
                                        "QuorumReq::UpdateServer -> state == QuorumState::\
                                    Retry && leader.is_some() -> RegisteredLeader::Local"
                                    )
                                }
                                RegisteredLeader::Remote(l) => {
                                    if l.address == server.address {
                                        leader = Some(RegisteredLeader::Remote(RpcServer {
                                            address: server.address.clone(),
                                            state: RpcServerState::Alive,
                                            tx: server.tx.clone(),
                                            election_ts: l.election_ts,
                                        }));

                                        state = QuorumState::Follower;
                                        health_state.state = QuorumState::Follower;
                                        health_state.tx_leader.clone_from(&server.tx);

                                        tx_remote
                                            .send_async(RpcRequest::LeaderReqAck {
                                                addr: l.address,
                                                election_ts: l.election_ts,
                                            })
                                            .await
                                            .unwrap();
                                    }
                                }
                            }
                        }

                        hosts.insert(server.address.clone(), server);

                        let count = hosts.len();
                        if count >= quorum {
                            match health {
                                QuorumHealth::Good => {}
                                QuorumHealth::Bad | QuorumHealth::Degraded => {
                                    info!("QuorumHealth changed from 'Bad | Degraded' to 'Good'");

                                    // If our quorum switched from bad -> good, send out a leadership
                                    // request and set ourselves as leader directly, if we do not already
                                    // have one.
                                    if leader.is_none() {
                                        debug!("No registered leader - requesting the leadership");
                                        // safe to unwrap until `2262-04-11`
                                        let election_ts = Utc::now().timestamp_nanos_opt().unwrap();
                                        leader = Some(RegisteredLeader::Local(election_ts));
                                        state = QuorumState::LeadershipRequested(election_ts);
                                        health_state.state =
                                            QuorumState::LeadershipRequested(election_ts);
                                        health_state.tx_leader = None;

                                        ack_count = 0;

                                        if let Err(err) = tx_remote
                                            .send_async(RpcRequest::LeaderReq {
                                                addr: whoami.clone(),
                                                election_ts,
                                            })
                                            .await
                                        {
                                            error!("{:?}", CacheError::from(&err));
                                        }
                                        election_timeout_handler(tx_quorum.clone(), false);
                                    }
                                }
                            }

                            health = if count == clients_count {
                                QuorumHealth::Good
                            } else {
                                QuorumHealth::Degraded
                            };
                            health_state.health = health.clone();
                        }
                    }

                    RpcServerState::Dead => {
                        debug!("Lost connection with cache client: {}", server.address);

                        hosts.remove(&server.address);

                        let count = hosts.len();
                        let mut leader_died = false;
                        if let Some(l) = leader.as_ref() {
                            match l {
                                RegisteredLeader::Local(_) => {}
                                RegisteredLeader::Remote(lead) => {
                                    if lead.address == server.address && count >= quorum {
                                        ack_count = 0;
                                        leader_died = true;
                                    }
                                }
                            }
                        }

                        if count >= quorum {
                            health = QuorumHealth::Degraded;
                            health_state.health = health.clone();
                            warn!("QuorumHealth changed from 'Good' to 'Degraded'");

                            if leader_died {
                                // If this state is set, we have acked to switch to another leader already
                                if state == QuorumState::LeaderDead {
                                    continue;
                                }

                                // Short sleep to better avoid race conditions
                                time::sleep(Duration::from_millis(get_rand_between(0, 100))).await;

                                // safe to unwrap until `2262-04-11`
                                let election_ts = Utc::now().timestamp_nanos_opt().unwrap();
                                tx_remote
                                    .send_async(RpcRequest::LeaderSwitchDead {
                                        vote_host: whoami.clone(),
                                        election_ts,
                                    })
                                    .await
                                    .unwrap();

                                leader = Some(RegisteredLeader::Local(election_ts));
                                state = QuorumState::LeadershipRequested(election_ts);
                                health_state.state = QuorumState::LeadershipRequested(election_ts);
                                health_state.tx_leader = None;

                                warn!("The current leader died - starting a new election");

                                election_timeout_handler(tx_quorum.clone(), false);
                            }
                        } else {
                            match health {
                                QuorumHealth::Good | QuorumHealth::Degraded => {
                                    warn!("QuorumHealth changed from 'Good | Degraded' to 'Bad' - resetting local cache");
                                    leader = None;
                                    health = QuorumHealth::Bad;
                                    state = QuorumState::Undefined;
                                    health_state.health = QuorumHealth::Bad;
                                    health_state.state = QuorumState::Undefined;
                                    health_state.tx_leader = None;

                                    ack_count = 0;
                                    for tx in cache_map.values() {
                                        tx.send_async(CacheReq::Reset).await?;
                                    }
                                }
                                QuorumHealth::Bad => {}
                            }
                        }
                    }
                }

                health_state.connected_hosts = hosts.len();
                log_cluster_info(&leader, &hosts, &whoami, &health, &state);
            }

            QuorumReq::GetReport { tx } => {
                let mut hosts_vec = vec![];
                for host in hosts.values() {
                    hosts_vec.push(host.clone());
                }
                let report = QuorumReport {
                    health: health.clone(),
                    state: state.clone(),
                    leader: leader.clone(),
                    hosts: hosts_vec,
                };

                match tx.send(report) {
                    Ok(_) => {}
                    Err(err) => {
                        error!("Error sending back QuorumReport value: {:?}", err);
                    }
                }
            }

            QuorumReq::LeaderInfo {
                addr,
                election_ts,
                has_quorum,
            } => {
                debug!(
                    "{} received QuorumReq::LeaderInfo: {:?} - quorum: {}",
                    whoami, addr, has_quorum
                );

                check_leadership_conflict(
                    &leader,
                    &tx_quorum,
                    election_ts,
                    &addr,
                    &whoami,
                    &tx_remote,
                )
                .await;

                if (has_quorum
                    && (state == QuorumState::Undefined || state == QuorumState::Retry)
                    && addr != whoami)
                    || leader.is_none()
                {
                    match get_leader_tx(&hosts, &addr).await {
                        None => {
                            debug!("Received LeaderInfo with quorum, but have no TX yet");
                            leader = Some(RegisteredLeader::Remote(RpcServer {
                                address: addr.clone(),
                                state: RpcServerState::Alive,
                                tx: None,
                                election_ts,
                            }));
                            state = QuorumState::LeaderTxAwait(addr.clone());
                            health_state.state = QuorumState::LeaderTxAwait(addr);
                        }
                        Some(tx) => {
                            let ack = RpcRequest::LeaderReqAck {
                                addr: addr.clone(),
                                election_ts,
                            };
                            if let Err(err) = tx.send_async(ack).await {
                                error!("{:?}", CacheError::from(&err));
                            }

                            leader = Some(RegisteredLeader::Remote(RpcServer {
                                address: addr,
                                state: RpcServerState::Alive,
                                tx: Some(tx.clone()),
                                election_ts,
                            }));
                            state = QuorumState::Follower;
                            health_state.state = QuorumState::Follower;
                            health_state.tx_leader = Some(tx);
                        }
                    };

                    log_cluster_info(&leader, &hosts, &whoami, &health, &state);
                }
            }

            QuorumReq::LeaderReq { req, tx } => {
                let mut accepted = false;

                // If we received a LeaderReq while being in state LeadershipRequested, we will end
                // up in a conflicting situation with multiple nodes requesting leadership.
                // -> Check and resolve via request timestamp
                let is_conflict = check_leadership_conflict(
                    &leader,
                    &tx_quorum,
                    req.election_ts,
                    &req.addr,
                    &whoami,
                    &tx_remote,
                )
                .await;

                if !is_conflict {
                    let tx_leader = get_leader_tx(&hosts, &req.addr).await;
                    if tx_leader.is_some() {
                        leader = Some(RegisteredLeader::Remote(RpcServer {
                            address: req.addr,
                            state: RpcServerState::Alive,
                            tx: tx_leader.clone(),
                            election_ts: req.election_ts,
                        }));
                        health_state.tx_leader = tx_leader;

                        accepted = true;

                        debug!(
                            "Registered new leader in QuorumReq::LeaderReq: {:?}",
                            leader
                        );

                        // set a timeout task to double check and possible reset, if we do not receive
                        // a LeaderAck in time
                        election_timeout_handler(tx_quorum.clone(), false);
                    } else {
                        debug!(
                            "Received QuorumReq::LeaderReq for not yet connected client: {} - trying again",
                            req.addr
                        );

                        leader = Some(RegisteredLeader::Remote(RpcServer {
                            address: req.addr,
                            state: RpcServerState::Alive,
                            tx: None,
                            election_ts: req.election_ts,
                        }));

                        state = QuorumState::Retry;
                        health_state.state = QuorumState::Retry;
                        health_state.tx_leader = None;

                        // set a timeout task to double check and possible reset, if we do not receive
                        // a LeaderAck in time
                        election_timeout_handler(tx_quorum.clone(), true);
                    }

                    log_cluster_info(&leader, &hosts, &whoami, &health, &state);
                };

                match tx.send(accepted) {
                    Ok(_) => {}
                    Err(err) => {
                        error!("Error sending back LeaderReq result: {:?}", err);
                    }
                }
            }

            QuorumReq::LeaderReqAck { addr, election_ts } => {
                debug!("{} received QuorumReq::LeaderReqAck for {}", whoami, addr);

                match &leader {
                    None => {
                        // If someone else ReqAcked another leader while we do not even have any, just
                        // accept it right away.
                        let tx_leader = get_leader_tx(&hosts, &addr).await;
                        leader = Some(RegisteredLeader::Remote(RpcServer {
                            address: addr.clone(),
                            state: RpcServerState::Alive,
                            tx: tx_leader.clone(),
                            election_ts,
                        }));
                        state = QuorumState::LeaderTxAwait(addr.clone());
                        health_state.state = QuorumState::LeaderTxAwait(addr.clone());
                        health_state.tx_leader = tx_leader;

                        tx_remote
                            .send_async(RpcRequest::LeaderReqAck { addr, election_ts })
                            .await
                            .unwrap();

                        election_timeout_handler(tx_quorum.clone(), false);
                        log_cluster_info(&leader, &hosts, &whoami, &health, &state);
                    }

                    Some(lead) => {
                        check_leadership_conflict(
                            &leader,
                            &tx_quorum,
                            election_ts,
                            &addr,
                            &whoami,
                            &tx_remote,
                        )
                        .await;

                        match lead {
                            RegisteredLeader::Local(ts) => {
                                if addr == whoami {
                                    ack_count += 1;
                                    debug!(
                                    "Received a LeaderReqAck from {} and increasing counter to {}",
                                    addr, ack_count
                                );

                                    if ack_count >= quorum {
                                        state = QuorumState::Leader;
                                        health_state.state = QuorumState::Leader;
                                        health_state.tx_leader = None;

                                        if let Err(err) = tx_remote
                                            .send_async(RpcRequest::LeaderAck {
                                                addr: whoami.clone(),
                                                election_ts: *ts,
                                            })
                                            .await
                                        {
                                            error!("{:?}", CacheError::from(&err));
                                        }
                                    }

                                    log_cluster_info(&leader, &hosts, &whoami, &health, &state);
                                } else {
                                    debug!("Received a LeaderReqAck for remote Server {}", addr);
                                }
                            }
                            RegisteredLeader::Remote(_) => {
                                debug!("Received a LeaderReqAck for remote Server {}", addr);
                            }
                        }
                    }
                }
            }

            QuorumReq::LeaderAck { ack } => {
                match get_leader_tx(&hosts, &ack.addr).await {
                    None => {
                        leader = Some(RegisteredLeader::Remote(RpcServer {
                            address: ack.addr.clone(),
                            state: RpcServerState::Alive,
                            tx: None,
                            election_ts: ack.election_ts,
                        }));

                        state = QuorumState::LeaderTxAwait(ack.addr.clone());
                        health_state.state = QuorumState::LeaderTxAwait(ack.addr);
                        health_state.tx_leader = None;
                    }
                    Some(tx) => {
                        leader = Some(RegisteredLeader::Remote(RpcServer {
                            address: ack.addr,
                            state: RpcServerState::Alive,
                            tx: Some(tx.clone()),
                            election_ts: ack.election_ts,
                        }));

                        state = QuorumState::Follower;
                        health_state.state = QuorumState::Follower;
                        health_state.tx_leader = Some(tx);
                    }
                }

                // reset any possibly build up ack_count from before -> we have a remote leader now
                ack_count = 0;

                log_cluster_info(&leader, &hosts, &whoami, &health, &state);
            }

            QuorumReq::LeaderSwitch { req } => {
                debug!("QuorumReq::LeaderSwitch to: {}", req.vote_host);

                let addr = req.vote_host.clone();

                if whoami == req.vote_host {
                    info!("Taking over the leadership after QuorumReq::LeaderSwitch");
                    ack_count = 0;
                    leader = Some(RegisteredLeader::Local(req.election_ts));
                    health_state.state = QuorumState::LeaderSwitch;
                    health_state.tx_leader = None;
                } else {
                    let tx_leader = get_leader_tx(&hosts, &req.vote_host).await;
                    leader = Some(RegisteredLeader::Remote(RpcServer {
                        address: req.vote_host,
                        state: RpcServerState::Alive,
                        tx: tx_leader.clone(),
                        election_ts: req.election_ts,
                    }));
                    health_state.state = QuorumState::LeaderSwitch;
                    health_state.tx_leader = tx_leader;
                }

                state = QuorumState::LeaderSwitch;

                if let Err(err) = tx_remote
                    .send_async(RpcRequest::LeaderSwitchAck { addr })
                    .await
                {
                    error!("{:?}", CacheError::from(&err));
                }

                election_timeout_handler(tx_quorum.clone(), false);
                log_cluster_info(&leader, &hosts, &whoami, &health, &state);
            }

            QuorumReq::LeaderSwitchDead { req } => {
                debug!("QuorumReq::LeaderSwitchDead to: {}", req.vote_host);

                // If we get a LeaderSwitchDead, we assume, that a single host just crashed
                let tx_leader = get_leader_tx(&hosts, &req.vote_host).await;
                leader = Some(RegisteredLeader::Remote(RpcServer {
                    address: req.vote_host.clone(),
                    state: RpcServerState::Alive,
                    tx: tx_leader.clone(),
                    election_ts: req.election_ts,
                }));
                state = QuorumState::LeaderDead;
                health_state.state = QuorumState::LeaderDead;
                health_state.tx_leader = tx_leader;

                tx_remote
                    .send_async(RpcRequest::LeaderReqAck {
                        addr: req.vote_host,
                        election_ts: req.election_ts,
                    })
                    .await
                    .unwrap();

                election_timeout_handler(tx_quorum.clone(), false);
                log_cluster_info(&leader, &hosts, &whoami, &health, &state);
            }

            // This is almost the same code as LeaderReqAck but is kept separate for better debugging
            QuorumReq::LeaderSwitchAck { req } => match &leader {
                None => unreachable!(),
                Some(lead) => match lead {
                    RegisteredLeader::Local(election_ts) => {
                        if req.addr == whoami {
                            ack_count += 1;
                            debug!(
                                "Received a LeaderSwitchAck: {:?}, and increasing counter to {}",
                                req, ack_count
                            );

                            if ack_count >= quorum {
                                state = QuorumState::Leader;
                                health_state.state = QuorumState::Leader;
                                if let Err(err) = tx_remote
                                    .send_async(RpcRequest::LeaderAck {
                                        addr: whoami.clone(),
                                        election_ts: *election_ts,
                                    })
                                    .await
                                {
                                    error!("{:?}", CacheError::from(&err));
                                }
                            }

                            log_cluster_info(&leader, &hosts, &whoami, &health, &state);
                        } else {
                            debug!("Received a LeaderSwitchAck: {:?}", req);
                        }
                    }
                    RegisteredLeader::Remote(_) => {
                        debug!("Received a LeaderSwitchAck to remote: {:?}", req);
                    }
                },
            },

            QuorumReq::LeaderSwitchPriority { req } => {
                debug!("{}: QuorumReq::LeaderSwitchPriority to: {:?}", whoami, req);

                let do_switch = match &state {
                    QuorumState::Leader => {
                        // if we are the leader, compare priority
                        if let Some(RegisteredLeader::Local(ts)) = &leader {
                            req.election_ts < *ts
                        } else {
                            unreachable!("If we are the leader, leader can only be Some(RegisteredLeader::Local(_))")
                        }
                    }
                    QuorumState::LeaderDead => true,
                    QuorumState::LeaderSwitch => true,
                    QuorumState::LeaderTxAwait(addr) => {
                        // If we are waiting for another leader TX than this switch request,
                        // compare priorities
                        if &req.vote_host != addr {
                            if let Some(RegisteredLeader::Remote(lead)) = &leader {
                                req.election_ts < lead.election_ts
                            } else {
                                unreachable!("If we await a leader tx, leader can only be Some(RegisteredLeader::Remote(_))")
                            }
                        } else {
                            false
                        }
                    }
                    QuorumState::LeadershipRequested(ts) => {
                        // Only if the remote leader has the higher priority, we will do the switch
                        // and ignore otherwise.
                        req.election_ts < *ts
                    }
                    QuorumState::Follower => {
                        if let Some(RegisteredLeader::Remote(lead)) = &leader {
                            // If we are unlucky, a remote host might already be set as a 'real'
                            // leader while still waiting for other incoming leadership requests.
                            req.election_ts < lead.election_ts
                        } else {
                            unreachable!("If we are a follower, leader can only be Some(RegisteredLeader::Remote(_))")
                        }
                    }
                    QuorumState::Undefined => true,
                    QuorumState::Retry => true,
                };

                // let do_switch = if let &QuorumState::LeadershipRequested(ts) = &state {
                //     // Only if the remote leader has the higher priority, we will do the switch
                //     // and ignore otherwise.
                //     req.election_ts < ts
                // } else if let Some(RegisteredLeader::Local(ts)) = &leader {
                //     // If we get unlucky or have network latency, this host might already be set as
                //     // a 'real' leader while still waiting for other incoming leadership requests.
                //     // Even in that case, we should handle it properly like above and only switch,
                //     // if the other host has the higher priority.
                //     req.election_ts < *ts
                // } else if let Some(RegisteredLeader::Remote(lead)) = &leader {
                //     // Again, if we get unlucky, a remote host might already be set as a 'real'
                //     // leader while still waiting for other incoming leadership requests.
                //     req.election_ts < lead.election_ts
                // } else {
                //     true
                // };

                if do_switch {
                    if req.vote_host == whoami {
                        leader = Some(RegisteredLeader::Local(req.election_ts));
                        state = QuorumState::Leader;
                        health_state.state = QuorumState::Leader;
                        health_state.tx_leader = None;
                    } else {
                        let tx_leader = get_leader_tx(&hosts, &req.vote_host).await;
                        leader = Some(RegisteredLeader::Remote(RpcServer {
                            address: req.vote_host.clone(),
                            state: RpcServerState::Alive,
                            tx: tx_leader.clone(),
                            election_ts: req.election_ts,
                        }));
                        state = if tx_leader.is_some() {
                            QuorumState::Follower
                        } else {
                            QuorumState::LeaderTxAwait(req.vote_host.clone())
                        };
                        health_state.state = state.clone();
                        health_state.tx_leader = tx_leader;

                        tx_remote
                            .send_async(RpcRequest::LeaderReqAck {
                                addr: req.vote_host,
                                election_ts: req.election_ts,
                            })
                            .await
                            .unwrap();

                        // We only need to check again after the timeout, if we do not already have the
                        // leader tx
                        if state == QuorumState::LeaderDead {
                            election_timeout_handler(tx_quorum.clone(), false);
                        }
                    }
                } else {
                    debug!(
                        "Received a QuorumReq::LeaderSwitchPriority while still having a higher \
                        priority - ignoring it."
                    );
                }

                // if req.vote_host == whoami {
                //     leader = Some(RegisteredLeader::Local(req.election_ts));
                //     state = QuorumState::Leader;
                //     health_state.state = QuorumState::Leader;
                //     health_state.tx_leader = None;
                // } else if do_switch {
                //     let tx_leader = get_leader_tx(&hosts, &req.vote_host).await;
                //     leader = Some(RegisteredLeader::Remote(RpcServer {
                //         address: req.vote_host.clone(),
                //         state: RpcServerState::Alive,
                //         tx: tx_leader.clone(),
                //         election_ts: req.election_ts,
                //     }));
                //     state = if tx_leader.is_some() {
                //         QuorumState::Follower
                //     } else {
                //         QuorumState::LeaderTxAwait(req.vote_host.clone())
                //     };
                //     health_state.state = state.clone();
                //     health_state.tx_leader = tx_leader;
                //
                //     tx_remote
                //         .send_async(RpcRequest::LeaderReqAck {
                //             addr: req.vote_host,
                //             election_ts: req.election_ts,
                //         })
                //         .await
                //         .unwrap();
                //
                //     // We only need to check again after the timeout, if we do not already have the
                //     // leader tx
                //     if state == QuorumState::LeaderDead {
                //         election_timeout_handler(tx_quorum.clone(), false);
                //     }
                // } else {
                //     debug!(
                //         "Received a QuorumReq::LeaderSwitchPriority while still having a higher \
                //         priority - ignoring it."
                //     );
                // }

                log_cluster_info(&leader, &hosts, &whoami, &health, &state);
            }

            QuorumReq::CheckElectionTimeout => {
                debug!("QuorumReq::CheckElectionTimeout");
                if leader.is_none() {
                    continue;
                }

                match state {
                    QuorumState::Retry => {
                        debug!("QuorumReq::CheckElectionTimeout -> QuorumState::Retry");

                        // no matter what happens next, set to undefined state first to only retry once
                        state = QuorumState::Undefined;

                        match leader.clone().unwrap() {
                            RegisteredLeader::Local(_) => unreachable!(),
                            RegisteredLeader::Remote(l) => {
                                // We get here, if we had a race condition on startup - this is the retry
                                if let Some(tx) = get_leader_tx(&hosts, &l.address).await {
                                    warn!("Retry after Cache Cluster Leader election timeout - this should not happen");

                                    leader = Some(RegisteredLeader::Remote(RpcServer {
                                        address: l.address.clone(),
                                        state: RpcServerState::Alive,
                                        tx: Some(tx.clone()),
                                        election_ts: l.election_ts,
                                    }));
                                    state = QuorumState::Follower;
                                    health_state.state = QuorumState::Follower;
                                    health_state.tx_leader = Some(tx);

                                    tx_remote
                                        .send_async(RpcRequest::LeaderReqAck {
                                            addr: l.address.clone(),
                                            election_ts: l.election_ts,
                                        })
                                        .await
                                        .unwrap();

                                    election_timeout_handler(tx_quorum.clone(), false);
                                    log_cluster_info(&leader, &hosts, &whoami, &health, &state);
                                }
                            }
                        }
                    }

                    QuorumState::LeaderDead => match leader.as_ref().unwrap() {
                        RegisteredLeader::Local(_) => {
                            error!(
                                "Received QuorumState::LeaderDead when Leader == \
                            RegisteredLeader::Local - this should never happen"
                            );
                        }
                        RegisteredLeader::Remote(_) => {
                            warn!("The Cache Cluster Leader died");

                            if state == QuorumState::LeaderDead && hosts.len() >= quorum {
                                time::sleep(Duration::from_millis(get_rand_between(0, 100))).await;
                                info!("Quorum is still good after cache cluster leader died - requesting leadership");

                                // safe to unwrap until `2262-04-11`
                                let election_ts = Utc::now().timestamp_nanos_opt().unwrap();
                                tx_remote
                                    .send_async(RpcRequest::LeaderSwitchDead {
                                        vote_host: whoami.clone(),
                                        election_ts,
                                    })
                                    .await
                                    .unwrap();

                                leader = Some(RegisteredLeader::Local(election_ts));
                                state = QuorumState::LeadershipRequested(election_ts);
                                health_state.state = QuorumState::LeadershipRequested(election_ts);
                                health_state.tx_leader = None;

                                election_timeout_handler(tx_quorum.clone(), false);
                                log_cluster_info(&leader, &hosts, &whoami, &health, &state);
                            }
                        }
                    },

                    QuorumState::LeadershipRequested(ts) => {
                        warn!(
                            "New Leader request has still not been validated after {} s",
                            *ELECTION_TIMEOUT
                        );

                        match health {
                            // If quorum is good, just re-send the leader req
                            QuorumHealth::Good | QuorumHealth::Degraded => {
                                if let Err(err) = tx_remote
                                    .send_async(RpcRequest::LeaderReq {
                                        addr: whoami.clone(),
                                        election_ts: ts,
                                    })
                                    .await
                                {
                                    error!("{:?}", CacheError::from(&err));
                                }
                                election_timeout_handler(tx_quorum.clone(), false);
                                log_cluster_info(&leader, &hosts, &whoami, &health, &state);
                            }
                            QuorumHealth::Bad => leader = None,
                        }
                    }

                    QuorumState::LeaderSwitch => {
                        warn!(
                            "New LeaderSwitch request has still not been validated after {} s",
                            *ELECTION_TIMEOUT
                        );

                        match health {
                            QuorumHealth::Good | QuorumHealth::Degraded => {
                                // safe to unwrap until `2262-04-11`
                                let election_ts = Utc::now().timestamp_nanos_opt().unwrap();
                                leader = Some(RegisteredLeader::Local(election_ts));
                                health_state.state = QuorumState::LeadershipRequested(election_ts);
                                health_state.tx_leader = None;

                                if let Err(err) = tx_remote
                                    .send_async(RpcRequest::LeaderReq {
                                        addr: whoami.clone(),
                                        election_ts,
                                    })
                                    .await
                                {
                                    error!("{:?}", CacheError::from(&err));
                                }
                                election_timeout_handler(tx_quorum.clone(), false);
                                log_cluster_info(&leader, &hosts, &whoami, &health, &state);
                            }
                            QuorumHealth::Bad => leader = None,
                        }
                    }

                    _ => {}
                }
            }

            QuorumReq::HostShutdown => {
                if state == QuorumState::Leader {
                    info!("Current Cache Cluster Leader is shutting down");

                    // TODO maybe make this a more sophisticated vote depending on smt like RTT or
                    // host resource usage
                    if let Some(addr) = hosts.keys().next() {
                        info!("Voting new leader after QuorumReq::HostShutdown: {}", addr);

                        if let Err(err) = tx_remote
                            .send_async(RpcRequest::LeaderSwitch {
                                vote_host: addr.clone(),
                                // safe to unwrap until `2262-04-11`
                                election_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                            })
                            .await
                        {
                            debug!("Error sending RpcRequest::LeaderLeader: {}", err);
                        }
                    }
                }

                let _ = tx_remote.send_async(RpcRequest::Shutdown).await;

                time::sleep(Duration::from_secs(1)).await;
                warn!("Exiting HA cache quorum handler");
                break;
            }

            QuorumReq::Print => {
                log_cluster_info(&leader, &hosts, &whoami, &health, &state);
            }
        }

        if let Err(err) = tx_watch.send(Some(QuorumHealthState {
            health: health.clone(),
            state: state.clone(),
            tx_leader: health_state.tx_leader.clone(),
            connected_hosts: hosts.len(),
        })) {
            debug!("'tx_watch' update error: {}", err);
        }
    }

    Ok(())
}

/// Checks for a conflicting situation with the leader election and sends out requests to solve any
/// possible conflicts. Returns `true`, if there was a conflict and `false` if not.
#[inline]
async fn check_leadership_conflict(
    curr_leader: &Option<RegisteredLeader>,
    tx_quorum: &flume::Sender<QuorumReq>,
    ts_remote: i64,
    addr_remote: &str,
    whoami: &str,
    tx_remote: &flume::Sender<RpcRequest>,
) -> bool {
    // Check, if we have a leader to compare against remote priority
    let res = if let Some(lead) = curr_leader {
        match lead {
            RegisteredLeader::Local(ts) => Some((ts, whoami)),
            RegisteredLeader::Remote(srv) => Some((&srv.election_ts, srv.address.as_str())),
        }
    } else {
        None
    };

    if let Some((ts, addr)) = res {
        let req = if &ts_remote < ts {
            info!(
                "{} received new LeaderReq with higher priority - switching to {}",
                whoami, addr_remote
            );

            // If we need to switch, we need to send the priority switch to ourselves, since the
            // other request would only go out to the other cache members by default.
            let _ = tx_quorum
                .send_async(QuorumReq::LeaderSwitchPriority {
                    req: cache::LeaderSwitchPriority {
                        vote_host: addr_remote.to_string(),
                        election_ts: ts_remote,
                    },
                })
                .await;

            RpcRequest::LeaderSwitchPriority {
                vote_host: addr_remote.to_string(),
                election_ts: ts_remote,
            }
        } else {
            // To solve conflicts better, we send out a LeaderSwitch in this case to
            // also inform all others that we have the higher priority.
            // TODO should we keep doing this or does it even work without it too?
            debug!(
                "{} received new LeaderReq with lower priority - sending out switch requests",
                whoami
            );

            RpcRequest::LeaderSwitchPriority {
                vote_host: addr.to_string(),
                election_ts: *ts,
            }
        };

        if let Err(err) = tx_remote.send_async(req).await {
            error!("Error sending RpcRequest::LeaderLeader: {}", err);
        }

        return true;
    }
    false
}

#[inline]
async fn get_leader_tx(
    hosts: &HashMap<String, RpcServer>,
    addr: &str,
) -> Option<flume::Sender<RpcRequest>> {
    if let Some(host) = hosts.get(addr) {
        if host.tx.is_some() {
            return host.tx.clone();
        }
        warn!("Host TX is not available in QuorumReq::LeaderReq when it should be - sleeping 1 sec and trying again");
    }
    None
}

/// This is a safety function to reset any new leader request if it was not validated with a quorum
/// after the given timeout.
fn election_timeout_handler(tx_quorum: flume::Sender<QuorumReq>, retry: bool) {
    let timeout = if retry {
        *RECONNECT_TIMEOUT_UPPER + 100
    } else {
        *ELECTION_TIMEOUT
    };
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(timeout)).await;
        // This might fail if the cache was shut down in between, otherwise it will always succeed
        let _ = tx_quorum.send_async(QuorumReq::CheckElectionTimeout).await;
    });
}

fn log_cluster_info(
    leader: &Option<RegisteredLeader>,
    clients: &HashMap<String, RpcServer>,
    whoami: &str,
    health: &QuorumHealth,
    state: &QuorumState,
) {
    let mut lead = if let Some(leader) = leader {
        match leader {
            RegisteredLeader::Local(_) => whoami,
            RegisteredLeader::Remote(l) => &l.address,
        }
    } else {
        "None"
    };
    if state == &QuorumState::Undefined || state == &QuorumState::Retry {
        lead = "Election in progress";
    }

    // TODO optimize and get rid of the Vec
    let mut clients_list = Vec::with_capacity(clients.len());
    for c in clients {
        clients_list.push(c.0);
    }

    debug!(
        r#"

    Cluster Leader: {}
    This Host:      {}
    Host State:     {:?}
    Clients:        {:?}
    Quorum Health:  {:?}
    "#,
        lead, whoami, state, clients_list, health
    )
}
