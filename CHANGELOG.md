# Changelog

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
