use crate::quorum::{AckLevel, QuorumState, RegisteredLeader, RpcServer, RpcServerState};
use crate::remove_from_leader;
use crate::rpc::cache;
use crate::rpc::cache::cache_server::{Cache, CacheServer};
use crate::rpc::cache::mgmt_request::Method;
use crate::rpc::cache::{ack, cache_request, mgmt_ack, Ack, CacheRequest, DelAck, GetAck, PutAck};
use crate::TLS;
use crate::{
    insert_from_leader, CacheConfig, CacheMethod, CacheNotify, CacheReq, QuorumHealth, QuorumReq,
};
use core::time::Duration;
use futures_core::Stream;
use futures_util::StreamExt;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::env;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::pin::Pin;
use tokio::fs;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tokio_stream::wrappers::ReceiverStream;
use tonic::metadata::MetadataValue;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

lazy_static! {
    static ref BUF_SIZE_SERVER: usize = env::var("CACHE_BUF_SERVER")
        .unwrap_or_else(|_| String::from("128"))
        .parse::<usize>()
        .expect("Error parsing 'CACHE_BUF_SERVER' to usize");
    static ref UPTIME: Instant = Instant::now();
    static ref PATH_TLS_CERT: String = env::var("CACHE_TLS_SERVER_CERT")
        .unwrap_or_else(|_| String::from("tls/redhac.local.cert.pem"));
    static ref PATH_TLS_KEY: String = env::var("CACHE_TLS_SERVER_KEY")
        .unwrap_or_else(|_| String::from("tls/redhac.local.key.pem"));
    static ref PATH_CLIENT_CA: String =
        env::var("CACHE_TLS_CA_CLIENT").unwrap_or_else(|_| String::from("tls/ca-chain.cert.pem"));
}

pub(crate) type CacheMap = HashMap<String, flume::Sender<CacheReq>>;

fn match_for_io_error(err_status: &Status) -> Option<&std::io::Error> {
    let mut err: &(dyn std::error::Error + 'static) = err_status;
    debug!("match_for_io_error: {:?}", err_status);

    loop {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err);
        }

        err = match err.source() {
            Some(err) => err,
            None => return None,
        };
    }
}

pub(crate) struct RpcCacheService {
    host_addr: String,
    cache_config: CacheConfig,
    tx_quorum: flume::Sender<QuorumReq>,
    tx_notify: Option<mpsc::Sender<CacheNotify>>,
}

impl RpcCacheService {
    pub async fn serve(
        addr: String,
        cache_config: CacheConfig,
        tx_quorum: flume::Sender<QuorumReq>,
        tx_notify: Option<mpsc::Sender<CacheNotify>>,
    ) -> anyhow::Result<()> {
        {
            // initialize the uptime
            let _ = UPTIME;
        }

        let interval = env::var("CACHE_KEEPALIVE_INTERVAL")
            .unwrap_or_else(|_| String::from("5"))
            .parse::<u64>()
            .expect("Error parsing 'CACHE_KEEPALIVE_INTERVAL' to u64");
        let timeout = env::var("CACHE_KEEPALIVE_TIMEOUT")
            .unwrap_or_else(|_| String::from("5"))
            .parse::<u64>()
            .expect("Error parsing 'CACHE_KEEPALIVE_TIMEOUT' to u64");

        let port = addr
            .split(':')
            .last()
            .expect("Error with the 'addr' format in 'RpcCacheService::serve'")
            .trim()
            .parse::<u16>()
            .expect("Cannot parse the Port to u16 in 'RpcCacheService::serve'");
        let addr_mod = format!("0.0.0.0:{}", port);
        let socket_addr: SocketAddr = addr_mod
            .parse()
            .expect("Could not parse the addr for the GrpcServer");

        let rpc_cache_service = RpcCacheService {
            host_addr: addr,
            cache_config,
            tx_quorum,
            tx_notify,
        };

        let svc = CacheServer::with_interceptor(rpc_cache_service, check_auth);
        let mut server = Server::builder()
            .http2_keepalive_interval(Some(Duration::from_secs(interval)))
            .http2_keepalive_timeout(Some(Duration::from_millis(timeout)))
            .http2_adaptive_window(Some(true))
            .timeout(Duration::from_secs(10));

        if *TLS {
            debug!("Loading server TLS cert from {}", *PATH_TLS_CERT);
            let cert = fs::read(&*PATH_TLS_CERT)
                .await
                .expect("Error reading server TLS Certificate");
            debug!("Loading server TLS key from {}", *PATH_TLS_KEY);
            let key = fs::read(&*PATH_TLS_KEY)
                .await
                .expect("Error reading server TLS Key");
            let id = Identity::from_pem(cert, key);

            let mut cfg = ServerTlsConfig::new().identity(id);

            if !PATH_CLIENT_CA.is_empty() {
                debug!("Loading client CA from {}", *PATH_CLIENT_CA);
                let ca = fs::read(&*PATH_CLIENT_CA)
                    .await
                    .expect("Error reading client TLS CA");
                let ca_chain = Certificate::from_pem(ca);
                cfg = cfg.client_ca_root(ca_chain);
            }

            server = server
                .tls_config(cfg)
                .expect("Error adding the TLS Certificate to the Cache Server");
        }

        server.add_service(svc).serve(socket_addr).await?;

        Ok(())
    }
}

#[tonic::async_trait()]
impl Cache for RpcCacheService {
    type StreamValuesStream = Pin<Box<dyn Stream<Item = Result<Ack, Status>> + Send + 'static>>;

    async fn stream_values(
        &self,
        request: Request<Streaming<CacheRequest>>,
    ) -> Result<Response<Self::StreamValuesStream>, Status> {
        debug!(
            "Debug req:\n{:?}\nRemote Addr: {:?}",
            request,
            request.remote_addr()
        );

        let host_addr = self.host_addr.clone();
        let addr = request.remote_addr().as_ref().map(|a| a.to_string());

        let mut in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(*BUF_SIZE_SERVER);

        let mut client = RpcServer {
            address: addr.unwrap_or_else(|| String::from("unknown")),
            state: RpcServerState::Alive,
            tx: None,
            election_ts: -1,
        };
        info!("RpcCacheClient connected: {:?}", client);

        let tx_quorum = self.tx_quorum.clone();

        let cache_config = self.cache_config.clone();
        let tx_notify = self.tx_notify.clone();

        // build up the LeaderInfo as the first message
        let (tx_oneshot, rx_quorum) = oneshot::channel();
        tx_quorum
            .send_async(QuorumReq::GetReport { tx: tx_oneshot })
            .await
            .expect("QuorumReq::GetReport in RpcServer");
        let report = rx_quorum
            .await
            // cannot fail at this point since we just opened the channel
            .unwrap();

        let has_quorum = match report.health {
            QuorumHealth::Good => true,
            QuorumHealth::Bad => false,
        };

        let (addr, election_ts) = match report.leader {
            None => (None, -1),
            Some(l) => match l {
                RegisteredLeader::Local(ts) => (Some(host_addr.clone()), ts),
                RegisteredLeader::Remote(lead) => (Some(lead.address), lead.election_ts),
            },
        };
        let info = mgmt_ack::Method::LeaderInfo(cache::LeaderInfo {
            addr,
            has_quorum,
            election_ts,
        });
        tx.send(Ok(Ack {
            method: Some(ack::Method::MgmtAck(cache::MgmtAck { method: Some(info) })),
        }))
        .await
        .expect("Error sending LeaderInfo on connect");

        // listen to the client stream
        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(req) => {
                        if req.method.is_none() {
                            tx.send(Err(Status::aborted(
                                "Error for CacheRequest - method is none",
                            )))
                            .await
                            .expect("Error Sending msg in stream_values");
                            continue;
                        }

                        match req.method.unwrap() {
                            cache_request::Method::Get(m) => {
                                let tx_cache = cache_config.cache_map.get(&m.cache_name);
                                if tx_cache.is_none() {
                                    error!(
                                        "'cache_map' misconfiguration in rpc server - Method::Get"
                                    );
                                    continue;
                                }

                                let (tx_get, rx_get) = flume::unbounded::<Option<Vec<u8>>>();
                                let res = tx_cache
                                    .unwrap()
                                    .send_async(CacheReq::Get {
                                        entry: m.entry.clone(),
                                        resp: tx_get,
                                    })
                                    .await;
                                if res.is_err() {
                                    error!("Error executing GET on the internal cache");
                                }

                                let val_res = rx_get.recv_async().await;
                                if val_res.is_err() {
                                    error!("Error receiving value through GET channel from the internal cache");
                                    continue;
                                }
                                let value = val_res.unwrap();

                                tx.send(Ok(Ack {
                                    method: Some(ack::Method::GetAck(GetAck {
                                        cache_name: m.cache_name,
                                        entry: m.entry.to_owned(),
                                        value,
                                    })),
                                }))
                                .await
                                .expect("Error Sending msg in stream_values");
                            }

                            cache_request::Method::Put(m) => {
                                debug!("Method::Put request for cache_name '{}'", &m.cache_name);
                                let tx_cache = cache_config.cache_map.get(&m.cache_name);
                                if tx_cache.is_none() {
                                    error!(
                                        "'cache_map' misconfiguration in rpc server - Method::Put"
                                    );
                                    continue;
                                }

                                if let Some(tx) = &tx_notify {
                                    let msg = CacheNotify {
                                        cache_name: m.cache_name.clone(),
                                        entry: m.entry.clone(),
                                        method: CacheMethod::Put,
                                    };

                                    // Fail fast and don't send the update to not block the cache, if
                                    // the channel is full
                                    if let Err(err) = tx.try_send(msg) {
                                        debug!("Sending CacheNotify for PUT over channel: {}", err);
                                    }
                                }

                                match tx_cache
                                    .unwrap()
                                    .send_async(CacheReq::Put {
                                        entry: m.entry.clone(),
                                        value: m.value,
                                    })
                                    .await
                                {
                                    Ok(_) => {
                                        tx.send(Ok(Ack {
                                            method: Some(ack::Method::PutAck(PutAck {
                                                req_id: m.req_id,
                                                mod_res: Some(true),
                                            })),
                                        }))
                                        .await
                                        .expect("Error Sending msg in stream_values");
                                    }
                                    Err(err) => {
                                        error!("{}", err);
                                        tx.send(Err(Status::internal(
                                            "Error executing PUT on the internal cache",
                                        )))
                                        .await
                                        .expect("Error Sending msg in stream_values");
                                    }
                                }
                            }

                            cache_request::Method::Insert(req) => {
                                debug!(
                                    "Method::Insert request for cache_name '{}'",
                                    &req.cache_name
                                );

                                let ack_level = AckLevel::from_rpc_value(req.ack_level);
                                match insert_from_leader(
                                    req.cache_name.clone(),
                                    req.entry.clone(),
                                    req.value,
                                    &cache_config,
                                    ack_level.clone(),
                                    None,
                                )
                                .await
                                {
                                    Ok(res) => {
                                        // notification listener channel
                                        if let Some(tx) = &tx_notify {
                                            let msg = CacheNotify {
                                                cache_name: req.cache_name,
                                                entry: req.entry,
                                                method: CacheMethod::Insert(ack_level),
                                            };

                                            // Fail fast and don't send the update to not block the cache, if
                                            // the channel is full
                                            if let Err(err) = tx.try_send(msg) {
                                                debug!(
                                                    "Sending CacheNotify for INSERT over channel: {}",
                                                    err
                                                );
                                            }
                                        }

                                        // answer to the client
                                        tx.send(Ok(Ack {
                                            method: Some(ack::Method::PutAck(PutAck {
                                                req_id: Some(req.req_id),
                                                mod_res: Some(res),
                                            })),
                                        }))
                                        .await
                                        .expect("Error Sending msg in stream_values");
                                    }
                                    Err(err) => {
                                        error!("{:?}", err);
                                        tx.send(Err(Status::internal(
                                            "Error executing INSERT on the internal cache",
                                        )))
                                        .await
                                        .expect("Error Sending msg in stream_values");
                                    }
                                }
                            }

                            cache_request::Method::Del(m) => {
                                debug!("Method::Del request for cache_name '{}'", &m.cache_name);
                                let tx_cache = cache_config.cache_map.get(&m.cache_name);
                                if tx_cache.is_none() {
                                    error!(
                                        "'cache_map' misconfiguration in rpc server - Method::Del"
                                    );
                                    continue;
                                }

                                if let Some(tx) = &tx_notify {
                                    let msg = CacheNotify {
                                        cache_name: m.cache_name.clone(),
                                        entry: m.entry.clone(),
                                        method: CacheMethod::Del,
                                    };

                                    // Fail fast and don't send the update to not block the cache, if
                                    // the channel is full
                                    if let Err(err) = tx.try_send(msg) {
                                        error!("Sending CacheNotify for DEL over channel: {}", err);
                                    }
                                }

                                match tx_cache
                                    .unwrap()
                                    .send_async(CacheReq::Del {
                                        entry: m.entry.clone(),
                                    })
                                    .await
                                {
                                    Ok(_) => {
                                        tx.send(Ok(Ack {
                                            method: Some(ack::Method::DelAck(DelAck {
                                                req_id: m.req_id,
                                                mod_res: Some(true),
                                            })),
                                        }))
                                        .await
                                        .expect("Error Sending msg in stream_values");
                                    }
                                    Err(err) => {
                                        error!("{}", err);
                                        tx.send(Err(Status::internal(
                                            "Error executing DEL on the internal cache",
                                        )))
                                        .await
                                        .expect("Error Sending msg in stream_values");
                                    }
                                }
                            }

                            cache_request::Method::Remove(req) => {
                                debug!(
                                    "Method::Remove request for cache_name '{}'",
                                    &req.cache_name
                                );

                                let ack_level = AckLevel::from_rpc_value(req.ack_level);
                                match remove_from_leader(
                                    req.cache_name.clone(),
                                    req.entry.clone(),
                                    &cache_config,
                                    ack_level.clone(),
                                    None,
                                )
                                .await
                                {
                                    Ok(res) => {
                                        // notification listener channel
                                        if let Some(tx) = &tx_notify {
                                            let msg = CacheNotify {
                                                cache_name: req.cache_name,
                                                entry: req.entry,
                                                method: CacheMethod::Remove(ack_level),
                                            };

                                            // Fail fast and don't send the update to not block the cache, if
                                            // the channel is full
                                            if let Err(err) = tx.try_send(msg) {
                                                debug!(
                                                    "Sending CacheNotify for REMOVE over channel: {}",
                                                    err
                                                );
                                            }
                                        }

                                        // answer to the client
                                        tx.send(Ok(Ack {
                                            method: Some(ack::Method::DelAck(DelAck {
                                                req_id: Some(req.req_id),
                                                mod_res: Some(res),
                                            })),
                                        }))
                                        .await
                                        .expect("Error Sending msg in stream_values");
                                    }
                                    Err(err) => {
                                        error!("{:?}", err);
                                        tx.send(Err(Status::internal(
                                            "Error executing REMOVE on the internal cache",
                                        )))
                                        .await
                                        .expect("Error Sending msg in stream_values");
                                    }
                                }
                            }

                            cache_request::Method::MgmtReq(mgmt_req) => {
                                debug!("MgmtReq received: {:?}", mgmt_req);

                                if let Some(m) = mgmt_req.method {
                                    match m {
                                        Method::Ping(_) => {
                                            debug!("Ping from {}", client.address);

                                            let pong = mgmt_ack::Method::Pong(cache::Pong {});

                                            tx.send(Ok(Ack {
                                                method: Some(ack::Method::MgmtAck(
                                                    cache::MgmtAck { method: Some(pong) },
                                                )),
                                            }))
                                            .await
                                            .expect("Error sending Pong");
                                        }

                                        Method::Health(_) => {
                                            debug!(
                                                "Received Health Request from {}",
                                                client.address
                                            );

                                            let (tx_oneshot, rx_quorum) = oneshot::channel();
                                            tx_quorum
                                                .send_async(QuorumReq::GetReport { tx: tx_oneshot })
                                                .await
                                                .expect("QuorumReq::GetReport in RpcServer");
                                            let report = rx_quorum.await.expect("");

                                            let quorum = match report.health {
                                                QuorumHealth::Good => true,
                                                QuorumHealth::Bad => false,
                                            };
                                            let state = match report.state {
                                                QuorumState::Leader => cache::State {
                                                    value: Some(cache::state::Value::Leader(
                                                        Default::default(),
                                                    )),
                                                },
                                                QuorumState::LeaderDead => cache::State {
                                                    value: Some(cache::state::Value::LeaderDead(
                                                        Default::default(),
                                                    )),
                                                },
                                                QuorumState::LeaderSwitch => cache::State {
                                                    value: Some(cache::state::Value::LeaderSwitch(
                                                        Default::default(),
                                                    )),
                                                },
                                                QuorumState::LeaderTxAwait(_addr) => cache::State {
                                                    value: Some(
                                                        cache::state::Value::LeaderTxAwait(
                                                            Default::default(),
                                                        ),
                                                    ),
                                                },
                                                QuorumState::LeadershipRequested(_timestamp) => cache::State {
                                                    value: Some(
                                                        cache::state::Value::LeadershipRequested(
                                                            Default::default(),
                                                        ),
                                                    ),
                                                },
                                                QuorumState::Follower => cache::State {
                                                    value: Some(cache::state::Value::Follower(
                                                        Default::default(),
                                                    )),
                                                },
                                                QuorumState::Undefined => cache::State {
                                                    value: Some(cache::state::Value::Undefined(
                                                        Default::default(),
                                                    )),
                                                },
                                                QuorumState::Retry => cache::State {
                                                    value: Some(cache::state::Value::Undefined(
                                                        Default::default(),
                                                    )),
                                                },
                                            };
                                            let leader = report.leader.map(|c| {
                                                let (addr, election_ts, connected) = match c {
                                                    RegisteredLeader::Local(ts) => {
                                                        (host_addr.clone(), ts, true)
                                                    }
                                                    RegisteredLeader::Remote(lead) => (
                                                        lead.address,
                                                        lead.election_ts,
                                                        lead.state == RpcServerState::Alive,
                                                    ),
                                                };
                                                cache::Leader {
                                                    addr,
                                                    election_ts,
                                                    connected,
                                                }
                                            });
                                            let mut host_health = vec![];
                                            for host in report.hosts {
                                                host_health.push(cache::HostHealth {
                                                    addr: host.address,
                                                    connected: host.state == RpcServerState::Alive,
                                                });
                                            }

                                            let health =
                                                mgmt_ack::Method::HealthAck(cache::HealthAck {
                                                    uptime_secs: UPTIME.elapsed().as_secs(),
                                                    quorum,
                                                    state: Some(state),
                                                    leader,
                                                    host_health,
                                                });

                                            tx.send(Ok(Ack {
                                                method: Some(ack::Method::MgmtAck(
                                                    cache::MgmtAck {
                                                        method: Some(health),
                                                    },
                                                )),
                                            }))
                                            .await
                                            .expect("Error sending HealthAck");
                                        }

                                        Method::LeaderReq(req) => {
                                            debug!("Received a LeaderReq: {:?}", req);
                                            let (tx_oneshot, rx_quorum) = oneshot::channel();
                                            let addr = req.addr.clone();
                                            let election_ts = req.election_ts;

                                            tx_quorum
                                                .send_async(QuorumReq::LeaderReq {
                                                    req,
                                                    tx: tx_oneshot,
                                                })
                                                .await
                                                .expect("Error sending LeaderReq");

                                            let accepted = rx_quorum.await.expect(
                                                "Error receiving LeaderReqAck from quorum handler",
                                            );
                                            if accepted {
                                                let req_ack = mgmt_ack::Method::LeaderReqAck(
                                                    cache::LeaderReqAck { addr, election_ts },
                                                );
                                                tx.send(Ok(Ack {
                                                    method: Some(ack::Method::MgmtAck(
                                                        cache::MgmtAck {
                                                            method: Some(req_ack),
                                                        },
                                                    )),
                                                }))
                                                .await
                                                .expect("Error sending LeaderReqAck");
                                            }
                                        }

                                        Method::LeaderReqAck(req) => {
                                            debug!("Received a LeaderReqAck: {:?}", req);
                                            tx_quorum
                                                .send_async(QuorumReq::LeaderReqAck {
                                                    addr: req.addr,
                                                    election_ts: req.election_ts,
                                                })
                                                .await
                                                .expect("Error sending LeaderReq");
                                        }

                                        Method::LeaderAck(ack) => {
                                            debug!("Received LeaderAck: {}", ack.addr);
                                            tx_quorum
                                                .send_async(QuorumReq::LeaderAck { ack })
                                                .await
                                                .expect("Error sending LeaderAck");
                                        }

                                        Method::LeaderSwitch(req) => {
                                            debug!("Received Method::LeaderSwitch: {:?}", req);

                                            tx_quorum
                                                .send_async(QuorumReq::LeaderSwitch { req })
                                                .await
                                                .expect("Error sending LeaderSwitch");
                                        }

                                        Method::LeaderSwitchPriority(req) => {
                                            debug!(
                                                "Received Method::LeaderSwitchPriority: {:?}",
                                                req
                                            );

                                            tx_quorum
                                                .send_async(QuorumReq::LeaderSwitchPriority { req })
                                                .await
                                                .expect("Error sending LeaderSwitchPriority");
                                        }

                                        Method::LeaderDead(req) => {
                                            tx_quorum
                                                .send_async(QuorumReq::LeaderSwitchDead { req })
                                                .await
                                                .expect("Error sending LeaderSwitchDead");
                                        }

                                        Method::LeaderSwitchAck(req) => {
                                            tx_quorum
                                                .send_async(QuorumReq::LeaderSwitchAck { req })
                                                .await
                                                .expect("Error sending LeaderSwitchDead");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(err) => {
                        if let Some(io_err) = match_for_io_error(&err) {
                            if io_err.kind() == ErrorKind::BrokenPipe {
                                error!("cache rpc client disconnected: broken pipe");
                                break;
                            }
                        }

                        match tx.send(Err(err)).await {
                            Ok(_) => (),
                            Err(err) => {
                                error!("{:?}", err);
                                break;
                            } // response was dropped
                        }
                    }
                }
            }

            // stream has ended - client is dead
            client.state = RpcServerState::Dead;
            warn!(
                "Cache Server: Stream with client '{:?}' on host '{}' ended",
                client, host_addr
            );
        });

        // just write the same data that was received
        let out_stream = ReceiverStream::new(rx);

        Ok(Response::new(
            Box::pin(out_stream) as Self::StreamValuesStream
        ))
    }
}

fn check_auth(req: Request<()>) -> Result<Request<()>, Status> {
    let token_str = env::var("CACHE_AUTH_TOKEN").expect("CACHE_AUTH_TOKEN is not set");
    let token: MetadataValue<_> = token_str
        .parse()
        .expect("Could not parse the Token to MetadataValue - needs to be ASCII");

    match req.metadata().get("authorization") {
        Some(t) if token == t => Ok(req),
        _ => {
            warn!("Connection request with bad Token");
            Err(Status::unauthenticated("No valid auth token"))
        }
    }
}
