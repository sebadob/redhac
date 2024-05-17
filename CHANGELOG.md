# Changelog

## v0.10.3

- lower default values for:
    - `CACHE_RECONNECT_TIMEOUT_LOWER=500`
    - `CACHE_RECONNECT_TIMEOUT_UPPER=2000`
    - `CACHE_ELECTION_TIMEOUT=2`
      to have quicker recoveries in case of a broken pipe or other errors
- additionally reduced tracing output on the `info` level
- the `insert` functions have been made more resilient during graceful leader switches and fail-overs

## v0.10.2

- Send some additional TCP keepalives for connections that are idle for longer periods of time to
  prevent them from being dropped.
- a little bit reduced logging for the `info` level

## v0.10.1

- removes a possible panic that could occur, when a cache-receiving side abruptly cancels the task
  without waiting for an answer

## v0.10.0

- core dependencies have been updated
- latest rust nightly clippy lints have been applied
- a very unlikely but possible channel panic in case of a conflict resolution has been fixed
- bump MSRV to 1.70.0

## 0.9.1

Typo corrections in documentation and removed now obsolete minimal-versions dependencies.

## 0.9.0

### Changes

The TLS setup has been changed a bit to make it more flexible.  
The only mandatory values with `CACHE_TLS=true` are now:

- `CACHE_TLS_SERVER_CERT` with new default: tls/redhac.cert-chain.pem
- `CACHE_TLS_SERVER_KEY` with new default: tls/redhac.key.pem
  While the following ones are now optional and do not have a default anymore:
- `CACHE_TLS_CLIENT_CERT`
- `CACHE_TLS_CLIENT_KEY`
- `CACHE_TLS_CA_SERVER`
- `CACHE_TLS_CA_CLIENT`

This makes it possible to use `redhac` with TLS without providing a private CA file, which you
would never need, if you certificates can be validated on system level anyway already. Also, the
mTLS setup is now optional with this change.

Additionally, the TLS certificate generation in the Readme and Docs have been updated and use
[Nioca](https://github.com/sebadob/nioca) for this task now, which is a lot more comfortable.
The test TLS certificates are checked into git as well for a faster start and evaluation.  
Do not use them in production!

## 0.8.0

### Features

This update introduces a small new feature.  
It is now possible to use the `redhac::quorum::QuorumHealthState` nicely externally.
The `quorum` module has been exported as `pub` and the `QuorumHealthState` has been added as
a `pub use` to the redhac main crate. This makes it possible for applications to actually use
the `QuorumHealthState` and pass it around to functions without the need to always pass the
whole `CacheConfig` each time.  
This is especially useful if you want to have your `CacheConfig.rx_health_state` in parts of
your application.

### Changes

- make `redhac::quorum::QuorumHealthState` pub usable
- `pub mod` of the `quorum` crate (with some new `pub(crate)` exceptions)
- bump versions or core dependencies
- fix minimal versions for new dependencies

## 0.7.0

This is a maintenance release.

- bump versions or core dependencies
- fix minimal versions for new dependencies
- introduce a justfile for easier maintenance
- bump MSRV to 1.65.0 due to core dependency updates
  [bcdfc62](https://github.com/sebadob/redhac/commit/bcdfc62665320a9ad3f832d0c28f0175d6e447c2)
  [506c9c6](https://github.com/sebadob/redhac/commit/506c9c6c2c2fb3cbb1253cabd6bf4cdf9b01f4b0)

## v0.6.0

- `cache_insert` and `cache_remove` default to their non-HA counterparts if `HA_MODE == false`
  [bcde62c](https://github.com/sebadob/redhac/commit/bcde62cbea233a68c86b21cd7300c150b2690bbf)
- deprecated code elements cleaned up
  [51fbe7f](https://github.com/sebadob/redhac/commit/51fbe7fe72598432c978ee24587a7a65e10f1c46)
- stability improvements inside certain K8s clusters - broken pipe prevention
  [e20bc39](https://github.com/sebadob/redhac/commit/e20bc39b925bb7738a0025a733e554efa1b4a546)
- removed the proto file build from `build.rs` to make docs.rs work (hopefully)
  [b53a2d3](https://github.com/sebadob/redhac/commit/b53a2d38f99f08bb9649035195228dc46be9cc7f)

## v0.5.0

`redhac` goes open source
