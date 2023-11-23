#! /usr/bin/env bash

BASE_DIR=$(
  cd $(dirname $0)/..
  pwd
)

source ${BASE_DIR}/bin/common_settings.sh

mkdir -p ${WALLET_DIR}/logs

# Startup keosd
keosd --unlock-timeout 100000 --http-server-address 0.0.0.0:6666 \
  --http-validate-host false --wallet-dir ${WALLET_DIR} \
  >> ${WALLET_DIR}/logs/keosd.log 2>&1 &
echo $! > ${WALLET_DIR}/keosd.pid
echo "Started keosd as PID $(cat ${WALLET_DIR}/keosd.pid)"
sleep 2