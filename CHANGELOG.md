# Changelog

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
