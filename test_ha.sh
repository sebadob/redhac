#!/bin/bash

export HA_MODE=true

cargo test test_ha_cache -- --ignored
