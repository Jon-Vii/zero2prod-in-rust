#!/usr/bin/env bash

set -x
set -eo pipefail

REDIS_PORT="${REDIS_PORT:=6379}"

docker run \
  -p "${REDIS_PORT}":6379 \
  -d redis

>&2 echo "Redis is running on port ${REDIS_PORT}."
