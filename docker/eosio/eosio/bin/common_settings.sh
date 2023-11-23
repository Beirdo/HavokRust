#! /usr/bin/env bash

BASE_DIR=$(
  cd $(dirname $0)/..
  pwd
)
WALLET_DIR=${BASE_DIR}/wallets
KEY_DIR=${WALLET_DIR}/keys
CLEOS="cleos --wallet-url http://127.0.0.1:6666 --url http://127.0.0.1:8000"
BLOCKCHAIN_BASEDIR=${BASE_DIR}/blockchain