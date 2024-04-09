# redhac

`redhac` is derived from **Rust Embedded Distributed Highly Available Cache**

The keywords **embedded** and **distributed** are a bit strange at the same time.<br>
The idea of `redhac` is to provide a caching library which can be embedded into any Rust
application, while still providing the ability to build a distributed HA caching layer.<br>
It can be used as a cache for a single instance too, of course, but then it will not be the
most performant, since it needs clones of most values to be able to send them over the network
concurrently. If you need a distributed cache however, you might give `redhac` a try.

The underlying system works with quorum and will elect a leader dynamically on startup. Each
node will start a server and multiple client parts for bidirectional gRPC streaming. Each client will
connect to each of the other servers.

## Release

This crate has been used and tested already before going open source.<br>
By now, did thousands of HA cluster startups, where I produced leadership election conflicts on purpose to make
sure they are handled and resolved properly.

However, some benchmarks and maybe performance finetuning is still missing, which is the reason why this crate has not
reached v1.0.0 yet. After the benchmarks the API may change, if another approach of handling everything is just faster.

## Consistency and guarantees

**Important:** `redhac` is used for caching, not for persistent data!

During normal operation, all the instances will forward their cache modifications to the other cache members.<br>
If a node goes down or has temporary network issues and therefore loses connection to the cluster, it will invalidate
its own cache to make 100% sure, that it would never provide possibly outdated data from the cache. Once a node
rejoins the cluster, it will start receiving and saving updates from the remotes again.<br>
There is an option in the `cache_get!` macro to fetch cached values from remote instances after such an event. This can
be used if you maybe have some values that only live inside the cache and cannot be refreshed from the database, for
instance.

The other situation, when the cache will not save any values is when there is no quorum and cluster leader.

If you need more consistency / guarantees / syncing after a late join, you may take a look at
[openraft](https://github.com/datafuselabs/openraft) or other projects like this.

### Versions and dependencies

- MSRV is Rust 1.70
- Everything is checked and working with `-Zminimal-versions`
- No issue reported by `cargo audit` (2024-04-09)

## Single Instance Example

```rust
// These 3 are needed for the `cache_get` macro
use redhac::{cache_get, cache_get_from, cache_get_value};
use redhac::{cache_put, cache_recv, CacheConfig, SizedCache};

#[tokio::main]
async fn main() {
    let (_, mut cache_config) = CacheConfig::new();
    
    // The cache name is used to reference the cache later on
    let cache_name = "my_cache";
    // We need to spawn a global handler for each cache instance.
    // Communication is done over channels.
    cache_config.spawn_cache(
        cache_name.to_string(), SizedCache::with_size(16), None
    );
    
    // Cache keys can only be `String`s at the time of writing.
    let key = "myKey";
    // The value you want to cache must implement `serde::Serialize`.
    // The serialization of the values is done with `bincode`.
    let value = "myCacheValue".to_string();
    
    // At this point, we need cloned values to make everything work
    // nicely with networked connections. If you only ever need a 
    // local cache, you might be better off with using the `cached`
    // crate directly and use references whenever possible.
    cache_put(
        cache_name.to_string(), key.to_string(), &cache_config, &value
    )
        .await
        .unwrap();
    
    let res = cache_get!(
        // The type of the value we want to deserialize the value into
        String,
        // The cache name from above. We can start as many for our
        // application as we like
        cache_name.to_string(),
        // For retrieving values, the same as above is true - we
        // need real `String`s
        key.to_string(),
        // All our caches have the necessary information added to
        // the cache config. Here a
        // reference is totally fine.
        &cache_config,
        // This does not really apply to this single instance 
        // example. If we would have started a HA cache layer we
        // could do remote lookups for a value we cannot find locally,
        // if this would be set to `true`
        false
    )
        .await
        .unwrap();
    
    assert!(res.is_some());
    assert_eq!(res.unwrap(), value);
}
```

## High Availability Setup

The High Availability (HA) works in a way, that each cache member connects to each other. When
eventually quorum is reached, a leader will be elected, which then is responsible for all cache
modifications to prevent collisions (if you do not decide against it with a direct `cache_put`).
Since each node connects to each other, it means that you cannot just scale up the cache layer
infinitely. The ideal number of nodes is 3. You can scale this number up for instance to 5 or 7
if you like, but this has not been tested in greater detail so far.

**Write performance** will degrade the more nodes you add to the cluster, since you simply need
to wait for more Ack's from the other members.  

**Read performance** however should stay the same.

Each node will keep a local copy of each value inside the cache (if it has not lost connection
or joined the cluster at some later point), which means in most cases reads do not require any
remote network access.

## Configuration

The way to configure the `HA_MODE` is optimized for a Kubernetes deployment but may seem a bit
odd at the same time, if you deploy somewhere else. You can either provide the `.env` file and
use it as a config file, or just set these variables for the environment directly. You need
to set the following values:

### `HA_MODE`

The first one is easy, just set `HA_MODE=true`

### `HA_HOSTS`

**NOTE:**<br>
In a few examples down below, the name for deployments may be `rauthy`. The reason is, that this
crate was written originally to complement another project of mine, Rauthy (link will follow),
which is an OIDC Provider and Single Sign-On Solution written in Rust.

The `HA_HOSTS` is working in a way, that it is really easy inside Kubernetes to configure it,
as long as a `StatefulSet` is used for the deployment.
The way a cache node finds its members is by the `HA_HOSTS` and its own `HOSTNAME`.
In the `HA_HOSTS`, add every cache member. For instance, if you want to use 3 replicas in HA
mode which are running and deployed as a `StatefulSet` with the name `rauthy` again:

```text
HA_HOSTS="http://rauthy-0:8000, http://rauthy-1:8000 ,http://rauthy-2:8000"
```

The way it works:

1. **A node gets its own hostname from the OS**<br>
This is the reason, why you use a StatefulSet for the deployment, even without any volumes
attached. For a `StatefulSet` called `rauthy`, the replicas will always have the names `rauthy-0`,
`rauthy-1`, ..., which are at the same time the hostnames inside the pod.
2. **Find "me" inside the `HA_HOSTS` variable**<br>
If the hostname cannot be found in the `HA_HOSTS`, the application will panic and exit because
of a misconfiguration.
3. **Use the port from the "me"-Entry that was found for the server part**<br>
This means you do not need to specify the port in another variable which eliminates the risk of
having inconsistencies
or a bad config in that case.
4. **Extract "me" from the `HA_HOSTS`**<br>
then take the leftover nodes as all cache members and connect to them
5. **Once a quorum has been reached, a leader will be elected**<br>
From that point on, the cache will start accepting requests
6. **If the leader is lost - elect a new one - No values will be lost**
7. **If quorum is lost, the cache will be invalidated**<br>
This happens for security reasons to provide cache inconsistencies. Better invalidate the cache
and fetch the values fresh from the DB or other cache members than working with possibly invalid
values, which is especially true in an authn / authz situation.

**NOTE:**<br>
If you are in an environment where the described mechanism with extracting the hostname would
not work, you can set the `HOSTNAME_OVERWRITE` for each instance to match one of the `HA_HOSTS`
entries, or you can overwrite the name when using the `redhac::start_cluster`.

### `CACHE_AUTH_TOKEN`

You need to set a secret for the `CACHE_AUTH_TOKEN`, which is then used for authenticating
cache members.

### TLS

For the sake of this example, we will not dig into TLS and disable it in the example, which
can be done with `CACHE_TLS=false`.<br>
You can add your TLS certificates in PEM format and an optional Root CA. This is true for the
Server and the Client part separately. This means you can configure the cache layer to use mTLS
connections.

#### Generating TLS certificates (optional)

You can of course provide your own set of certificates, if you already have some, or just use your preferred way of
creating some. However, I want to show a way of doing this in the most simple way possible with another tool of mine
called [Nioca](https://github.com/sebadob/nioca).<br>
If you have `docker` or similar available, this is the easiest option. If not, you can grab one of the binaries from
the [out]() folder, which are available for linux amd64 and arm64.

I suggest to use `docker` for this task. Otherwise, you can use the `nioca` binary directly on any linux machine.
If you want a permanent way of generating certificates for yourself, take a look at Rauthy's `justfile` and copy
and adjust the recipes `create-root-ca` and `create-end-entity-tls` to your liking.  
If you just want to get everything started quickly, follow these steps:

##### Folder for your certificates

Let's create a folder for our certificates:

```
mkdir ca
```

##### Create an alias for the `docker` command

If you use one of the binaries directly, you can skip this step.

```
alias nioca='docker run --rm -it -v ./ca:/ca -u $(id -u ${USER}):$(id -g ${USER}) ghcr.io/sebadob/nioca'
```

To see the full feature set for more customization than mentioned below:
```
nioca x509 -h
```

##### Generate full certificate chain

We can create and generate a fully functioning, production ready Root Certificate Authority (CA) with just a single
command. Make sure that at least one of your `--alt-name-dns` from here matches the `CACHE_TLS_CLIENT_VALIDATE_DOMAIN`
from the redhac config later on.<br>
To keep things simple, we will use the same certificate for the server and the client. You can of course create
separate ones, if you like:

```
nioca x509 \
    --cn 'redhac.local' \
    --alt-name-dns redhac.local \
    --usages-ext server-auth \
    --usages-ext client-auth \
    --stage full \
    --clean
```

You will be asked 6 times (yes, 6) for an at least 16 character password:
- The first 3 times, you need to provide the encryption password for your Root CA
- The last 3 times, you should provide a different password for your Intermediate CA

When everything was successful, you will have a new folder named `x509` with sub folders `root`, `intermediate`
and `end_entity` in your current one.

From these, you will need the following files:

```
cp ca/x509/intermediate/ca-chain.pem ./redhac.ca-chain.pem && \
cp ca/x509/end_entity/$(cat ca/x509/end_entity/serial)/cert-chain.pem ./redhac.cert-chain.pem && \
cp ca/x509/end_entity/$(cat ca/x509/end_entity/serial)/key.pem ./redhac.key.pem
```

- You should have 3 files in `ls -l`:
```
redhac.ca-chain.pem
redhac.cert-chain.pem
redhac.key.pem
```

**4. Create Kubernetes Secrets**
```
kubectl create secret tls redhac-tls-server --key="redhac.key.pem" --cert="redhac.cert-chain.pem" && \
kubectl create secret tls redhac-tls-client --key="redhac.key.pem" --cert="redhac.cert-chain.pem" && \
kubectl create secret generic redhac-server-ca --from-file redhac.ca-chain.pem && \
kubectl create secret generic redhac-client-ca --from-file redhac.ca-chain.pem
``` 

### Reference Config

The following variables are the ones you can use to configure `redhac` via env vars.  
At the time of writing, the configuration can only be done via the env.

```text
# If the cache should start in HA mode or standalone
# accepts 'true|false', defaults to 'false'
HA_MODE=true

# The connection strings (with hostnames) of the HA instances
# as a CSV. Format: 'scheme://hostname:port'
HA_HOSTS="http://redhac.redhac:8080, http://redhac.redhac:8180, http://redhac.redhac:8280"

# This can overwrite the hostname which is used to identify each
# cache member. Useful in scenarios, where all members are on the
# same host or for testing. You need to add the port, since `redhac`
# will do an exact match to find "me".
#HOSTNAME_OVERWRITE="127.0.0.1:8080"

# Secret token, which is used to authenticate the cache members
CACHE_AUTH_TOKEN=SuperSafeSecretToken1337

# Enable / disable TLS for the cache communication (default: true)
CACHE_TLS=true

# The path to the server TLS certificate PEM file
# default: tls/redhac.cert-chain.pem
CACHE_TLS_SERVER_CERT=tls/redhac.cert-chain.pem
# The path to the server TLS key PEM file
# default: tls/redhac.key.pem
CACHE_TLS_SERVER_KEY=tls/redhac.key.pem

# The path to the client mTLS certificate PEM file. This is optional.
CACHE_TLS_CLIENT_CERT=tls/redhac.cert-chain.pem
# The path to the client mTLS key PEM file. This is optional.
CACHE_TLS_CLIENT_KEY=tls/redhac.key.pem

# If not empty, the PEM file from the specified location will be
# added as the CA certificate chain for validating
# the servers TLS certificate. This is optional.
CACHE_TLS_CA_SERVER=tls/redhac.ca-chain.pem
# If not empty, the PEM file from the specified location will
# be added as the CA certificate chain for validating
# the clients mTLS certificate. This is optional.
CACHE_TLS_CA_CLIENT=tls/redhac.ca-chain.pem

# The domain / CN the client should validate the certificate
# against. This domain MUST be inside the
# 'X509v3 Subject Alternative Name' when you take a look at 
# the servers certificate with the openssl tool.
# default: redhac.local
CACHE_TLS_CLIENT_VALIDATE_DOMAIN=redhac.local

# Can be used if you need to overwrite the SNI when the 
# client connects to the server, for instance if you are 
# behind a loadbalancer which combines multiple certificates. 
# default: ""
#CACHE_TLS_SNI_OVERWRITE=

# Define different buffer sizes for channels between the 
# components. Buffer for client request on the incoming 
# stream - server side (default: 128)
# Makes sense to have the CACHE_BUF_SERVER roughly set to:
# `(number of total HA cache hosts - 1) * CACHE_BUF_CLIENT`
CACHE_BUF_SERVER=128
# Buffer for client requests to remote servers for all cache 
# operations (default: 64)
CACHE_BUF_CLIENT=64

# Connections Timeouts
# The Server sends out keepalive pings with configured timeouts
# The keepalive ping interval in seconds (default: 5)
CACHE_KEEPALIVE_INTERVAL=5
# The keepalive ping timeout in seconds (default: 5)
CACHE_KEEPALIVE_TIMEOUT=5

# The timeout for the leader election. If a newly saved leader
# request has not reached quorum after the timeout, the leader
# will be reset and a new request will be sent out.
# CAUTION: This should not be below 
# CACHE_RECONNECT_TIMEOUT_UPPER, since cold starts and
# elections will be problematic in that case.
# value in seconds, default: 5
CACHE_ELECTION_TIMEOUT=5

# These 2 values define the reconnect timeout for the HA Cache
# Clients. The values are in ms and a random between these 2 
# will be chosen each time to avoid conflicts
# and race conditions (default: 2500)
CACHE_RECONNECT_TIMEOUT_LOWER=2500
# (default: 5000)
CACHE_RECONNECT_TIMEOUT_UPPER=5000
```

## Example Code

For example code, please take a look at `./examples`.

The `conflict_resolution_tests` are used for producing conflicts on purpose by starting multiple nodes from the same
code at the exact same time, which is maybe not too helpful if you want to take a look at how it's done.

The better example then would be the `ha_setup`, which can be used to start 3 nodes in 3 different terminals to observe
the behavior.
