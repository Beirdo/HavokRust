#! /usr/bin/env bash

BASE_DIR=$(
  cd $(dirname $0)/..
  pwd
)
source ${BASE_DIR}/bin/common_settings.sh

if [ $# -lt 5 ] ; then
  echo "Usage: $0 accountname pubkey privkey rpc_port p2p_port [mode]"
  echo "   where mode can be: genesis, hard_replay or normal (the default)"
  exit 1
fi

ACCOUNT=$1
PUBKEY=$2
PRIVKEY=$3
RPC_PORT=$4
P2P_PORT=$5
MODE=${6:-normal}

P2P_PORTS="9010 9011 9012 9013"
P2P_PORTS=$(echo ${P2P_PORTS} | xargs -n 1 echo | grep -v ${P2P_PORT} | xargs echo)
P2P_ARGS=$(echo ${P2P_PORTS} | sed -e 's/\(\S*\)/--p2p-peer-address 127.0.0.1:\1/g')

BLOCKCHAIN_DIR=${BLOCKCHAIN_BASEDIR}/${ACCOUNT}
mkdir -p ${BLOCKCHAIN_DIR}/data ${BLOCKCHAIN_DIR}/blocks ${BLOCKCHAIN_DIR}/config \
  ${BLOCKCHAIN_DIR}/logs

if [ "$MODE" = "genesis" ] ; then
  GENESIS_ARGS="--genesis-json ${BASE_DIR}/genesis/genesis.json"
elif [ "$MODE" = "hard_replay" ] ; then
  HARD_REPLAY_ARGS="--hard-replay-blockchain"
fi

# Start the service
nodeos \
  ${GENESIS_ARGS} \
  --signature-provider ${PUBKEY}=KEOSD:http://127.0.0.1:6666 \
  --keosd-provider-timeout 100 \
  --plugin eosio::producer_plugin \
  --plugin eosio::producer_api_plugin \
  --plugin eosio::chain_plugin \
  --plugin eosio::chain_api_plugin \
  --plugin eosio::http_plugin \
  --plugin eosio::history_plugin \
  --plugin eosio::history_api_plugin \
  --data-dir ${BLOCKCHAIN_DIR}/data \
  --blocks-dir ${BLOCKCHAIN_DIR}/blocks \
  --config-dir ${BLOCKCHAIN_DIR}/config \
  --producer-name ${ACCOUNT} \
  --http-server-address 0.0.0.0:${RPC_PORT} \
  --p2p-listen-endpoint 0.0.0.0:${P2P_PORT} \
  --access-control-allow-origin='*' \
  --contracts-console \
  --http-validate-host false \
  --verbose-http-errors \
  --enable-stale-production \
  --max-irreversible-block-age -1 \
  --max-transaction-time=1000 \
  --abi-serializer-max-time-ms=60 \
  ${P2P_ARGS} \
  ${HARD_REPLAY_ARGS} \
  >> ${BLOCKCHAIN_DIR}/logs/nodeos.log 2>&1 &
echo $! > ${BLOCKCHAIN_DIR}/nodeos.pid
echo "Started nodeos for ${ACCOUNT} as PID $(cat ${BLOCKCHAIN_DIR}/nodeos.pid)"
sleep 2
