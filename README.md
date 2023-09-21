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

- MSRV is Rust 1.65
- Everything is checked and working with `-Zminimal-versions`
- No issue reported by `cargo audit` (2023-09-21)

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
    cache_config.spawn_cache(cache_name.to_string(), SizedCache::with_size(16), None);
    
    // Cache keys can only be `String`s at the time of writing.
    let key = "myKey";
    // The value you want to cache must implement `serde::Serialize`.
    // The serialization of the values is done with `bincode`.
    let value = "myCacheValue".to_string();
    
    // At this point, we need cloned values to make everything work nicely with networked
    // connections. If you only ever need a local cache, you might be better off with using the
    // `cached` crate directly and use references whenever possible.
    cache_put(cache_name.to_string(), key.to_string(), &cache_config, &value)
        .await
        .unwrap();
    
    let res = cache_get!(
        // The type of the value we want to deserialize the value into
        String,
        // The cache name from above. We can start as many for our application as we like
        cache_name.to_string(),
        // For retrieving values, the same as above is true - we need real `String`s
        key.to_string(),
        // All our caches have the necessary information added to the cache config. Here a
        // reference is totally fine.
        &cache_config,
        // This does not really apply to this single instance example. If we would have started
        // a HA cache layer we could do remote lookups for a value we cannot find locally, if
        // this would be set to `true`
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

**Note:**<br>
The TLS tools provided in this repository are very basic and have a terrible user experience.<br>
They should only be used, if you do not have an already existing TLS setup or workflow.<br>
A project specifically tailored to TLS CA and certificates is in the making.

If you want to use these for testing anyway, here is how to do it:

#### Generating TLS certificates (optional)

As mentioned, the tools are very basic. If you for instance type in a bad password during CA / intermediate generation,
they will just throw an error, and you need to clean up again. They should only get you started and be used for testing.  
There are a lot of good tools out there which can get you started with TLS and there is no real benefit in creating
just another one that does the same stuff.

The scripts can be found in `tls/ca`. You need to have `openssl` and a BASH shell available on your system. They have 
not been tested on Windows and will most probably not work at all. They might work however on Mac OS, even though this
was never tested.

The cache layer does validate the CA for mTLS connections, which is why you can generate a full set of certificates.
`cd` into the `tls/ca` directory and do the following:

**1. Certificate Authority (CA)**
- Execute
```
./build_ca.sh
```
and enter an at least 4 character password for the private key file for the CA (3 times).

**2. Intermediate CA**
- Execute
```
./build_intermediate.sh
```
and enter an at least 4 character password for the private key file for the CA (3 times again).
- The 4. password needs to be the one from the CA private key file from step 1.
- Type `y` 2 times to verify the signing of the new certificate
- On success, you will see `intermediate/certs/intermediate.cert.pem: OK` as the last line

**3. End Entity Certificates**  
These are the certificates used by the cache or any other server / client.
- The end entity script needs the common name for the certificate as the 1. option when you execute it:
```
./build_end_entity.sh redhac.local
```
- Enter the password for the Intermediate CA private key from step 2 and verify the signing.
- On success, you will see `intermediate/certs/redhac.local.cert.pem: OK` as the last line
- - You should have 3 files in the `../` folder:
```
ca-chain.pem
redhac.local.cert.pem
redhac.local.key.pem
```

**Info:**  
This is not a tutorial about TLS certificates.  
As mentioned above already, another dedicated TLS project is in the making.

**4. Create Kubernetes Secrets**
```
kubectl create secret tls redhac-tls-server --key="../redhac.local.key.pem" --cert="../redhac.local.cert.pem" && \
kubectl create secret tls redhac-tls-client --key="../redhac.local.key.pem" --cert="../redhac.local.cert.pem" && \
kubectl create secret generic redhac-server-ca --from-file ../ca-chain.pem && \
kubectl create secret generic redhac-client-ca --from-file ../ca-chain.pem
``` 

### Reference Config

The following variables are the ones you can use to configure `redhac` via env vars.  
At the time of writing, the configuration can only be done via the env.

```text
# If the cache should start in HA mode or standalone
# accepts 'true|false', defaults to 'false'
HA_MODE=true

# The connection strings (with hostnames) of the HA instances as a CSV
# Format: 'scheme://hostname:port'
HA_HOSTS="http://redhac.redhac:8080, http://redhac.redhac:8180 ,http://redhac.redhac:8280"

# This can overwrite the hostname which is used to identify each cache member.
# Useful in scenarios, where all members are on the same host or for testing.
# You need to add the port, since `redhac` will do an exact match to find "me".
#HOSTNAME_OVERWRITE="127.0.0.1:8080"

# Secret token, which is used to authenticate the cache members
CACHE_AUTH_TOKEN=SuperSafeSecretToken1337

# Enable / disable TLS for the cache communication (default: true)
CACHE_TLS=true

# The path to the server TLS certificate PEM file (default: tls/certs/redhac.local.cert.pem)
CACHE_TLS_SERVER_CERT=tls/redhac.local.cert.pem
# The path to the server TLS key PEM file (default: tls/certs/redhac.local.key.pem)
CACHE_TLS_SERVER_KEY=tls/redhac.local.key.pem

# The path to the client mTLS certificate PEM file (default: tls/certs/redhac.local.cert.pem)
CACHE_TLS_CLIENT_CERT=tls/redhac.local.cert.pem
# The path to the client mTLS key PEM file (default: tls/certs/redhac.local.key.pem)
CACHE_TLS_CLIENT_KEY=tls/redhac.local.key.pem

# If not empty, the PEM file from the specified location will be added as the CA certificate
# chain for validating the servers TLS certificate (default: tls/certs/ca-chain.cert.pem)
CACHE_TLS_CA_SERVER=tls/ca-chain.cert.pem

# If not empty, the PEM file from the specified location will be added as the CA certificate
# chain for validating the clients mTLS certificate (default: tls/certs/ca-chain.cert.pem)
CACHE_TLS_CA_CLIENT=tls/ca-chain.cert.pem

# The domain / CN the client should validate the certificate against. This domain MUST be
# inside the 'X509v3 Subject Alternative Name' when you take a look at the servers certificate
# with the openssl tool. (default: redhac.local)
CACHE_TLS_CLIENT_VALIDATE_DOMAIN=redhac.local
# Can be used if you need to overwrite the SNI when the client connects to the server, for
# instance if you are behind a loadbalancer which combines multiple certificates. (default: "")
#CACHE_TLS_SNI_OVERWRITE=

# Define different buffer sizes for channels between the components
# Buffer for client request on the incoming stream - server side (default: 128)
# Makes sense to have the CACHE_BUF_SERVER roughly set to:
# `(number of total HA cache hosts - 1) * CACHE_BUF_CLIENT`
CACHE_BUF_SERVER=128
# Buffer for client requests to remote servers for all cache operations (default: 64)
CACHE_BUF_CLIENT=64

# Connections Timeouts
# The Server sends out keepalive pings with configured timeouts
# The keepalive ping interval in seconds (default: 5)
CACHE_KEEPALIVE_INTERVAL=5
# The keepalive ping timeout in seconds (default: 5)
CACHE_KEEPALIVE_TIMEOUT=5

# The timeout for the leader election. If a newly saved leader request has not reached quorum
# after the timeout, the leader will be reset and a new request will be sent out.
# CAUTION: This should not be below CACHE_RECONNECT_TIMEOUT_UPPER, since cold starts and
# elections will be problematic in that case.
# value in seconds, default: 5
CACHE_ELECTION_TIMEOUT=5

# These 2 values define the reconnect timeout for the HA Cache Clients.
# The values are in ms and a random between these 2 will be chosen each time to avoid conflicts
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
