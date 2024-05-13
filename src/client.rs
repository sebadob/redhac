use std::env;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use cached::Cached;
use futures_util::TryStreamExt;
use lazy_static::lazy_static;
use tokio::sync::{mpsc, watch};
use tokio::{fs, time};
use tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::InterceptedService;
use tonic::metadata::MetadataValue;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity, Uri};
use tonic::{Request, Status};
use tower::util::ServiceExt;
use tracing::{debug, error, info};

use crate::quorum::{AckLevel, RpcServer, RpcServerState};
use crate::rpc::cache;
use crate::rpc::cache::cache_client::CacheClient;
use crate::rpc::cache::mgmt_ack::Method;
use crate::rpc::cache::{ack, cache_request, CacheRequest};
use crate::{get_cache_req_id, get_rand_between, CacheError, QuorumReq, TLS};

lazy_static! {
    static ref BUF_SIZE_CLIENT: usize = env::var("CACHE_BUF_CLIENT")
        .unwrap_or_else(|_| String::from("64"))
        .parse::<usize>()
        .expect("Error parsing 'CACHE_BUF_CLIENT' to usize");
    pub(crate) static ref RECONNECT_TIMEOUT_LOWER: u64 = env::var("CACHE_RECONNECT_TIMEOUT_LOWER")
        .unwrap_or_else(|_| String::from("2500"))
        .parse::<u64>()
        .expect("Error parsing 'CACHE_RECONNECT_TIMEOUT_LOWER' to u64");
    pub(crate) static ref RECONNECT_TIMEOUT_UPPER: u64 = env::var("CACHE_RECONNECT_TIMEOUT_UPPER")
        .unwrap_or_else(|_| String::from("5000"))
        .parse::<u64>()
        .expect("Error parsing 'CACHE_RECONNECT_TIMEOUT_UPPER' to u64");
    static ref PATH_TLS_CERT: Option<String> = env::var("CACHE_TLS_CLIENT_CERT").ok();
    static ref PATH_TLS_KEY: Option<String> = env::var("CACHE_TLS_CLIENT_KEY").ok();
    static ref PATH_SERVER_CA: Option<String> = env::var("CACHE_TLS_CA_SERVER").ok();
    static ref TLS_VALIDATE_DOMAIN: String = env::var("CACHE_TLS_CLIENT_VALIDATE_DOMAIN")
        .unwrap_or_else(|_| String::from("redhac.local"));
    static ref CACHE_TLS_SNI_OVERWRITE: String =
        env::var("CACHE_TLS_SNI_OVERWRITE").unwrap_or_else(|_| String::default());
}

#[derive(Debug, Clone)]
pub enum RpcRequest {
    Ping,
    Get {
        cache_name: String,
        entry: String,
        resp: flume::Sender<Option<Vec<u8>>>,
    },
    Put {
        cache_name: String,
        entry: String,
        value: Vec<u8>,
        resp: Option<flume::Sender<bool>>,
    },
    // 'Insert' is the HA version of Put - every request through the leader
    Insert {
        cache_name: String,
        entry: String,
        value: Vec<u8>,
        ack_level: AckLevel,
        resp: flume::Sender<bool>,
    },
    Del {
        cache_name: String,
        entry: String,
        resp: Option<flume::Sender<bool>>,
    },
    // 'Remove' is the HA version of Del - every request through the leader
    Remove {
        cache_name: String,
        entry: String,
        ack_level: AckLevel,
        resp: flume::Sender<bool>,
    },
    LeaderReq {
        addr: String,
        election_ts: i64,
    },
    LeaderAck {
        addr: String,
        election_ts: i64,
    },
    LeaderSwitch {
        vote_host: String,
        election_ts: i64,
    },
    LeaderReqAck {
        addr: String,
        election_ts: i64,
    },
    LeaderAckAck {
        addr: String,
    },
    LeaderSwitchAck {
        addr: String,
    },
    LeaderSwitchDead {
        vote_host: String,
        election_ts: i64,
    },
    LeaderSwitchPriority {
        vote_host: String,
        election_ts: i64,
    },
    Shutdown,
}

/// This is needed for a remote lookup GET request to return the correct result to the cache_get
/// function.
#[derive(Debug, Clone)]
enum HAReq {
    SaveGetCallbackTx {
        cache_name: String,
        entry: String,
        tx: flume::Sender<Option<Vec<u8>>>,
    },
    SaveModCallbackTx {
        req_id: String,
        tx: flume::Sender<bool>,
    },
    SendCacheGetResponse {
        cache_name: String,
        entry: String,
        value: Option<Vec<u8>>,
    },
    SendCacheModResponse {
        req_id: String,
        value: bool,
    },
}

type GrpcClient =
    CacheClient<InterceptedService<Channel, fn(Request<()>) -> Result<Request<()>, Status>>>;

pub(crate) async fn cache_clients(
    ha_hosts: Vec<String>,
    tx_quorum: flume::Sender<QuorumReq>,
    rx_remote_req: flume::Receiver<RpcRequest>,
) -> anyhow::Result<()> {
    let mut client_handles = vec![];
    let mut client_tx = vec![];

    let tx_quorum = Arc::new(tx_quorum);

    for h in ha_hosts {
        let (tx, rx) = flume::unbounded();
        let handle = tokio::spawn(run_client(h.clone(), tx_quorum.clone(), tx.clone(), rx));
        client_tx.push(tx);
        client_handles.push(handle);
    }

    // This task forwards incoming remote requests to every connected client.
    // Kind of like a broadcast channel from tokio, but async.
    tokio::spawn(async move {
        while let Ok(payload) = rx_remote_req.recv_async().await {
            debug!(
                "Received a RpcRequest in the ClientsBroadcastHandler: {:?}",
                payload
            );
            for tx in client_tx.iter() {
                if let Err(err) = tx.send_async(payload.clone()).await {
                    error!("Error in HA Cache client broadcaster: {}", err);
                }
            }
            if let RpcRequest::Shutdown = payload {
                debug!("Shutting down HA Cache client broadcaster");
                break;
            }
        }
    });

    for c in client_handles {
        c.await.unwrap().unwrap();
    }
    debug!("All cache_client handles joined - Exiting");

    Ok(())
}

async fn run_client(
    host: String,
    tx_quorum: Arc<flume::Sender<QuorumReq>>,
    tx_remote: flume::Sender<RpcRequest>,
    rx_remote_req: flume::Receiver<RpcRequest>,
) -> anyhow::Result<()> {
    loop {
        debug!("Trying to connect to host '{}'", host);

        let uri = Uri::from_str(&host).expect("Unable to build GRPC URI");
        let chan_res = if *TLS {
            if host.starts_with("http://") {
                error!("Connecting to an HTTP address with active will fail!");
            }

            let mut cfg = ClientTlsConfig::new();
            // .identity(id)
            // .domain_name(&*TLS_VALIDATE_DOMAIN);

            if let Some(path) = &*PATH_TLS_CERT {
                debug!("Loading client TLS cert from {}", path);
                let cert = fs::read(path)
                    .await
                    .expect("Error reading client TLS Certificate");

                let key = if let Some(path) = &*PATH_TLS_KEY {
                    debug!("Loading client TLS key from {}", path);
                    fs::read(path).await.expect("Error reading client TLS Key")
                } else {
                    panic!("If PATH_TLS_CERT is given, just must provide PATH_TLS_KEY too");
                };

                cfg = cfg.identity(Identity::from_pem(cert, key));
            }

            if let Some(path) = &*PATH_SERVER_CA {
                debug!("Loading server CA from {}", path);
                let ca = fs::read(path).await.expect("Error reading server TLS CA");
                let ca_chain = Certificate::from_pem(ca);
                cfg = cfg.ca_certificate(ca_chain);
            }

            // We always want to validate the domain name
            cfg = cfg.domain_name(&*TLS_VALIDATE_DOMAIN);

            let mut channel = Channel::builder(uri)
                .tls_config(cfg)
                .expect("Error creating TLS Config for Cache Client")
                .tcp_keepalive(Some(Duration::from_secs(30)))
                .http2_keep_alive_interval(Duration::from_secs(30))
                .keep_alive_while_idle(true);

            if !CACHE_TLS_SNI_OVERWRITE.is_empty() {
                let url = Uri::from_str(&CACHE_TLS_SNI_OVERWRITE)
                    .expect("Error parsing CACHE_TLS_SNI_OVERWRITE to URI");
                channel = channel.origin(url);
            }
            channel.connect().await
        } else {
            Channel::builder(uri)
                .tcp_keepalive(Some(Duration::from_secs(30)))
                .http2_keep_alive_interval(Duration::from_secs(30))
                .keep_alive_while_idle(true)
                .connect()
                .await
        };
        if chan_res.is_err() {
            error!("Unable to connect to host '{}'.", host);
            // retry with jitter
            time::sleep(Duration::from_millis(get_rand_between(2500, 7500))).await;
            continue;
        }
        let channel = chan_res.unwrap().ready_oneshot().await?;
        debug!(
            "Cache connection channel established successfully to host '{}'",
            host
        );

        // when the channel has a valid connection, empty any possibly received cache operations
        // from the channel to not execute outdated operations from a maybe longer period of
        // "waiting for the host to come up"
        let drain = rx_remote_req.drain();
        if drain.len() > 0 {
            debug!(
                "Drained 'rx_remote_req' channel for client {} had {} entries",
                host,
                drain.len()
            );
        }

        let mut client: GrpcClient = CacheClient::with_interceptor(channel, add_auth);

        let mut server = RpcServer {
            address: host.clone(),
            state: RpcServerState::Alive,
            tx: Some(tx_remote.clone()),
            election_ts: -1,
        };

        // We need a back channel for GET requests
        // This task stores the oneshot tx channels based on the lookup index so we can answer cache
        // remote lookups in case we get an answer from any remote host.
        let (callback_tx, callback_rx) = flume::unbounded::<HAReq>();
        let callback_handle = tokio::spawn(async move {
            let mut get_cache = cached::TimedCache::with_lifespan(10);
            let mut ack_cache = cached::TimedCache::with_lifespan(10);

            loop {
                match callback_rx.recv_async().await {
                    Err(err) => {
                        error!("Received None over get_rx - exiting: {:?}", err);
                        break;
                    }

                    Ok(req) => match req {
                        HAReq::SaveGetCallbackTx {
                            cache_name,
                            entry,
                            tx,
                        } => {
                            let idx = format!("{}-{}", cache_name, entry);
                            debug!("HAReq::SaveGetCallbackTx for '{}'", idx);
                            get_cache.cache_set(idx, tx);
                        }

                        HAReq::SaveModCallbackTx { req_id, tx } => {
                            debug!("HAReq::SaveModifyCallbackTx for req_id: {}", req_id);
                            ack_cache.cache_set(req_id, tx);
                        }

                        HAReq::SendCacheGetResponse {
                            cache_name,
                            entry,
                            value,
                        } => {
                            let idx = format!("{}-{}", cache_name, entry);
                            debug!("GetReq::Send for '{}'", idx);
                            match get_cache.cache_remove(&idx) {
                                None => {
                                    error!("Error in HAReq::SendCacheGetResponse: tx does not exist in the cache - possibly response timeout");
                                    continue;
                                }
                                Some(tx) => {
                                    if let Err(err) = tx.send_async(value).await {
                                        error!(
                                            "Error sending back GET response in HAReq::SendCacheGetResponse: {:?}",
                                            err
                                        );
                                    }
                                }
                            };
                        }

                        HAReq::SendCacheModResponse { req_id, value } => {
                            debug!("HAReq::SendCacheModResponse for req_id: {}", req_id);
                            match ack_cache.cache_remove(&req_id) {
                                None => {
                                    error!("Error in HAReq::SendCacheModResponse: tx does not exist in the cache - possibly response timeout");
                                    continue;
                                }
                                Some(tx) => {
                                    if let Err(err) = tx.send_async(value).await {
                                        debug!(
                                            "Error sending back MOD response in HAReq::SendCacheModResponse: {:?}",
                                            err
                                        );
                                    }
                                }
                            }
                        }
                    },
                }
            }
        });

        debug!("Starting the Sending Stream to the Server");
        if let Err(err)= tx_quorum
            .send_async(QuorumReq::UpdateServer {
                server: server.clone(),
            })
            .await {
            // can fail in case of a conflict resolution, if the other side has just shut down
            error!("tx_quorum send error: {:?}", err);
            callback_handle.abort();
            time::sleep(Duration::from_millis(get_rand_between(
                *RECONNECT_TIMEOUT_LOWER,
                *RECONNECT_TIMEOUT_UPPER,
            )))
                .await;
            continue;
        }

        // Sending Stream to the Server
        // the ReceiverStream only accepts an mpsc channel
        let (tx_server, rx_server) = mpsc::channel::<CacheRequest>(*BUF_SIZE_CLIENT);
        let (tx_shutdown, rx_shutdown) = watch::channel(false);
        let rx_remote_clone = rx_remote_req.clone();
        let callback_tx_clone = callback_tx.clone();
        let server_clone = server.clone();
        let rpc_request_handle = tokio::spawn(async move {
            debug!("Listening for RpcRequests for connected Client");

            while let Ok(req) = rx_remote_clone.recv_async().await {
                if tx_server.is_closed() {
                    // we may end up here if the server rx could not be converted into a stream
                    // successfully
                    error!("Stream to server has been closed");
                    break;
                }

                let forward_req = match req {
                    RpcRequest::Ping => CacheRequest {
                        method: Some(cache_request::Method::MgmtReq(cache::MgmtRequest {
                            method: Some(cache::mgmt_request::Method::Ping(cache::Ping {})),
                        })),
                    },

                    RpcRequest::Get {
                        cache_name,
                        entry,
                        resp,
                    } => {
                        // We need to save the oneshot tx if we get an async replay later
                        // on from the server
                        if let Err(err) = callback_tx_clone
                            .send_async(HAReq::SaveGetCallbackTx {
                                cache_name: cache_name.clone(),
                                entry: entry.clone(),
                                tx: resp,
                            })
                            .await
                        {
                            error!("Error sending HAReq::SaveGetCallbackTx: {}", err);
                        };
                        let get = cache_request::Method::Get(cache::Get { cache_name, entry });
                        CacheRequest { method: Some(get) }
                    }

                    RpcRequest::Put {
                        cache_name,
                        entry,
                        value,
                        resp,
                    } => {
                        let req_id = if let Some(tx) = resp {
                            let req_id = get_cache_req_id();
                            if let Err(err) = callback_tx_clone
                                .send_async(HAReq::SaveModCallbackTx {
                                    req_id: req_id.clone(),
                                    tx,
                                })
                                .await
                            {
                                error!(
                                    "Error sending HAReq::SaveModCallbackTx in RpcRequest::Put: {}",
                                    err
                                );
                            };
                            Some(req_id)
                        } else {
                            None
                        };

                        let put = cache_request::Method::Put(cache::Put {
                            cache_name,
                            entry,
                            value,
                            req_id,
                        });
                        CacheRequest { method: Some(put) }
                    }

                    RpcRequest::Insert {
                        cache_name,
                        entry,
                        value,
                        ack_level,
                        resp,
                    } => {
                        // We need to save the oneshot tx if we get an async replay later
                        // on from the server
                        let req_id = get_cache_req_id();
                        if let Err(err) = callback_tx_clone
                            .send_async(HAReq::SaveModCallbackTx {
                                req_id: req_id.clone(),
                                tx: resp,
                            })
                            .await
                        {
                            error!(
                                "Error sending HAReq::SaveModCallbackTx in RpcRequest::Insert: {}",
                                err
                            );
                        };

                        debug!("Sending out HA Method::Insert for req_id {}", req_id);
                        let insert = cache_request::Method::Insert(cache::Insert {
                            cache_name,
                            entry,
                            value,
                            req_id,
                            ack_level: Some(ack_level.get_rpc_value()),
                        });
                        CacheRequest {
                            method: Some(insert),
                        }
                    }

                    RpcRequest::Del {
                        cache_name,
                        entry,
                        resp,
                    } => {
                        let req_id = if let Some(tx) = resp {
                            let req_id = get_cache_req_id();
                            if let Err(err) = callback_tx_clone
                                .send_async(HAReq::SaveModCallbackTx {
                                    req_id: req_id.clone(),
                                    tx,
                                })
                                .await
                            {
                                error!(
                                    "Error sending HAReq::SaveModCallbackTx in RpcRequest::Del: {}",
                                    err
                                );
                            };
                            Some(req_id)
                        } else {
                            None
                        };

                        let req = cache_request::Method::Del(cache::Del {
                            cache_name,
                            entry,
                            req_id,
                        });
                        CacheRequest { method: Some(req) }
                    }

                    RpcRequest::Remove {
                        cache_name,
                        entry,
                        ack_level,
                        resp,
                    } => {
                        // We need to save the oneshot tx if we get an async replay later
                        // on from the server
                        let req_id = get_cache_req_id();
                        if let Err(err) = callback_tx_clone
                            .send_async(HAReq::SaveModCallbackTx {
                                req_id: req_id.clone(),
                                tx: resp,
                            })
                            .await
                        {
                            error!(
                                "Error sending HAReq::SaveModCallbackTx in RpcRequest::Remove: {}",
                                err
                            );
                        };

                        debug!("Sending out HA Method::Remove for req_id {}", req_id);
                        let req = cache_request::Method::Remove(cache::Remove {
                            cache_name,
                            entry,
                            req_id,
                            ack_level: Some(ack_level.get_rpc_value()),
                        });
                        CacheRequest { method: Some(req) }
                    }

                    // sends out a req in case we want to be the new leader
                    RpcRequest::LeaderReq { addr, election_ts } => {
                        let req = cache::mgmt_request::Method::LeaderReq(cache::LeaderReq {
                            addr,
                            election_ts,
                        });
                        CacheRequest {
                            method: Some(cache_request::Method::MgmtReq(cache::MgmtRequest {
                                method: Some(req),
                            })),
                        }
                    }

                    // should be sent out, when we are the leader and have quorum
                    RpcRequest::LeaderAck { addr, election_ts } => {
                        let req = cache::mgmt_request::Method::LeaderAck(cache::LeaderAck {
                            addr,
                            election_ts,
                        });
                        CacheRequest {
                            method: Some(cache_request::Method::MgmtReq(cache::MgmtRequest {
                                method: Some(req),
                            })),
                        }
                    }

                    RpcRequest::LeaderSwitch {
                        election_ts,
                        vote_host,
                    } => {
                        let req = cache::mgmt_request::Method::LeaderSwitch(cache::LeaderSwitch {
                            election_ts,
                            vote_host,
                        });
                        CacheRequest {
                            method: Some(cache_request::Method::MgmtReq(cache::MgmtRequest {
                                method: Some(req),
                            })),
                        }
                    }

                    // ack for a received LeaderReq from another host
                    RpcRequest::LeaderReqAck { addr, election_ts } => {
                        let req = cache::mgmt_request::Method::LeaderReqAck(cache::LeaderReqAck {
                            addr,
                            election_ts,
                        });
                        CacheRequest {
                            method: Some(cache_request::Method::MgmtReq(cache::MgmtRequest {
                                method: Some(req),
                            })),
                        }
                    }

                    // acks a LeaderAck and starts the normal caching operation
                    RpcRequest::LeaderAckAck { addr: _ } => {
                        todo!("...do we even need this´?");
                    }

                    // the same as LeaderAckAck, but for a switch / graceful shutdown of the
                    // current leader
                    RpcRequest::LeaderSwitchAck { addr } => {
                        let req =
                            cache::mgmt_request::Method::LeaderSwitchAck(cache::LeaderSwitchAck {
                                addr,
                            });
                        CacheRequest {
                            method: Some(cache_request::Method::MgmtReq(cache::MgmtRequest {
                                method: Some(req),
                            })),
                        }
                    }

                    RpcRequest::LeaderSwitchDead {
                        vote_host,
                        election_ts,
                    } => {
                        let req = cache::mgmt_request::Method::LeaderDead(cache::LeaderDead {
                            vote_host,
                            election_ts,
                        });
                        CacheRequest {
                            method: Some(cache_request::Method::MgmtReq(cache::MgmtRequest {
                                method: Some(req),
                            })),
                        }
                    }

                    RpcRequest::LeaderSwitchPriority {
                        vote_host,
                        election_ts,
                    } => {
                        let req = cache::mgmt_request::Method::LeaderSwitchPriority(
                            cache::LeaderSwitchPriority {
                                vote_host,
                                election_ts,
                            },
                        );
                        CacheRequest {
                            method: Some(cache_request::Method::MgmtReq(cache::MgmtRequest {
                                method: Some(req),
                            })),
                        }
                    }

                    RpcRequest::Shutdown => {
                        info!("Received shutdown message - exiting");
                        let _ = tx_shutdown.send(true);
                        break;
                    }
                };

                if let Err(err) = tx_server.send(forward_req).await {
                    error!(
                        "Error forwarding Cache Client message to handler for host {}: {}",
                        server_clone.address, err
                    );
                }
            }
        });

        // the input stream from the server
        let in_stream = ReceiverStream::new(rx_server);
        let res = client.stream_values(in_stream).await;
        let mut res_stream = match res {
            Ok(r) => r.into_inner(),
            Err(err) => {
                error!(
                    "Error opening 'stream_values' to the server {}: {}\n",
                    host, err
                );
                time::sleep(Duration::from_millis(get_rand_between(1000, 5000))).await;
                continue;
            }
        };

        loop {
            match res_stream.try_next().await {
                Ok(recv) => {
                    if recv.is_none() {
                        error!(
                            "Received None in client receiver for Host {} - exiting: {:?}",
                            host, recv
                        );
                        break;
                    }

                    let method = recv.unwrap().method;
                    if method.is_none() {
                        error!("Error with Ack from Server - result is None");
                        break;
                    }

                    match method.unwrap() {
                        ack::Method::GetAck(a) => {
                            let get_req = HAReq::SendCacheGetResponse {
                                cache_name: a.cache_name,
                                entry: a.entry,
                                value: a.value,
                            };
                            let send = callback_tx.send_async(get_req).await;
                            if send.is_err() {
                                error!("Error forwarding remote Cache GET request");
                            }
                        }

                        ack::Method::PutAck(ack) => {
                            if let Some(req_id) = ack.req_id {
                                let req = HAReq::SendCacheModResponse {
                                    req_id,
                                    value: ack.mod_res.unwrap_or(false),
                                };
                                if let Err(err) = callback_tx.send_async(req).await {
                                    error!("{}", err);
                                }
                            }
                        }

                        ack::Method::DelAck(ack) => {
                            if let Some(req_id) = ack.req_id {
                                let req = HAReq::SendCacheModResponse {
                                    req_id,
                                    value: ack.mod_res.unwrap_or(false),
                                };
                                if let Err(err) = callback_tx.send_async(req).await {
                                    error!("{}", err);
                                }
                            }
                        }

                        ack::Method::MgmtAck(ack) => {
                            if ack.method.is_none() {
                                continue;
                            }
                            let method = ack.method.unwrap();

                            match method {
                                Method::Pong(_) => {
                                    debug!("pong");
                                    // todo!("Method::Pong - use it to calculate a RTT in the future - not yet implemented");
                                }

                                Method::HealthAck(_) => {}

                                Method::LeaderInfo(info) => {
                                    if let Some(addr) = info.addr {
                                        debug!(
                                            "Received LeaderInfo: {:?} with quorum: {}",
                                            addr, info.has_quorum
                                        );
                                        if let Err(err) = tx_quorum
                                            .send_async(QuorumReq::LeaderInfo {
                                                addr,
                                                has_quorum: info.has_quorum,
                                                election_ts: info.election_ts,
                                            })
                                            .await
                                        {
                                            error!("{:?}", CacheError::from(&err));
                                        }
                                    }
                                }

                                Method::LeaderReqAck(req) => {
                                    debug!("Received LeaderReqAck: {:?}", req.addr);
                                    if let Err(err) = tx_quorum
                                        .send_async(QuorumReq::LeaderReqAck {
                                            addr: req.addr,
                                            election_ts: req.election_ts,
                                        })
                                        .await
                                    {
                                        error!("{:?}", CacheError::from(&err));
                                    }
                                }

                                Method::LeaderAckAck(_) => {}

                                Method::LeaderSwitchAck(req) => {
                                    debug!("Received LeaderSwitchAck: {:?}", req.addr);
                                    if let Err(err) = tx_quorum
                                        .send_async(QuorumReq::LeaderSwitchAck { req })
                                        .await
                                    {
                                        error!("{:?}", CacheError::from(&err));
                                    }
                                }
                            }
                        }

                        ack::Method::Error(e) => {
                            error!("Cache Server sent an error: {:?}", e);
                        }
                    }
                }

                Err(err) => {
                    // TODO push this into the debug level after enough testing
                    // -> duplicate logging with the error a few lines below
                    error!(
                        "Received an error in client receiver for Host {} - exiting: {:?}",
                        host, err
                    );
                    break;
                }
            }
        }

        // if we get here, we lost connection
        error!("Connection lost to host: '{}'", host);
        server.state = RpcServerState::Dead;
        tx_quorum
            .send_async(QuorumReq::UpdateServer {
                server: server.clone(),
            })
            .await
            .expect("Unregistering Dead Client");

        // Make sure to kill the spawns to not have memory leaks since they never end
        callback_handle.abort();
        rpc_request_handle.abort();

        if *rx_shutdown.borrow() {
            break;
        }

        time::sleep(Duration::from_millis(get_rand_between(
            *RECONNECT_TIMEOUT_LOWER,
            *RECONNECT_TIMEOUT_UPPER,
        )))
        .await;
    }

    Ok(())
}

fn add_auth(mut req: Request<()>) -> Result<Request<()>, Status> {
    let token_str = env::var("CACHE_AUTH_TOKEN").expect("CACHE_AUTH_TOKEN is not set");
    let token: MetadataValue<_> = token_str
        .parse()
        .expect("Could not parse the Token to MetadataValue - needs to be ASCII");
    req.metadata_mut().insert("authorization", token);
    Ok(req)
}
