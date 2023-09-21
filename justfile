set shell := ["bash", "-uc"]

export TAG := `cat Cargo.toml | grep '^version =' | cut -d " " -f3 | xargs`

# prints out the currently set version
version:
    #!/usr/bin/env bash
    echo "v$TAG"


# runs the full set of tests
test:
    #!/usr/bin/env bash
    set -euxo pipefail
    clear
    cargo test
    HA_MODE=true cargo test test_ha_cache -- --ignored
    echo All tests successful


# builds the code
build:
    #!/usr/bin/env bash
    set -euxo pipefail

    cargo clippy -- -D warnings
    # build as musl to make sure this works
    cargo build --release --target x86_64-unknown-linux-musl


# makes sure everything is fine
is-clean: test build
    #!/usr/bin/env bash
    set -euxo pipefail

    # exit early if clippy emits warnings
    cargo clippy -- -D warnings

    # make sure everything has been committed
    git diff --exit-code

    echo all good


# sets a new git tag and pushes it
release:
    #!/usr/bin/env bash
    set -euxo pipefail

    # make sure git is clean
    git diff --quiet || exit 1

    git tag "v$TAG"
    git push origin "v$TAG"


# publishes the current version to cargo.io
#publish: test build
#    #!/usr/bin/env bash
#    set -euxo pipefail
