#! /usr/bin/env bash

set -e

BASE_DIR=$(
  cd $(dirname $0)/..
  pwd
)

source ${BASE_DIR}/bin/common_settings.sh

mode=${1:-normal}

${BASE_DIR}/bin/keosd_start.sh

${CLEOS} wallet open -n development
DEVELOPMENT_PASSWORD=$(cat ${KEY_DIR}/development.password)
${CLEOS} wallet unlock -n development --password ${DEVELOPMENT_PASSWORD}

${CLEOS} wallet private_keys -n development --password ${DEVELOPMENT_PASSWORD} >${KEY_DIR}/development_keys.json
EOSIO_PUBKEY=$(cat ${KEY_DIR}/eosio.pubkey | sed -e 's/.*\"\(.*\)\".*/\1/')
EOSIO_PRIVKEY=$(${BASE_DIR}/bin/extract_private_key.py ${KEY_DIR}/development_keys.json ${EOSIO_PUBKEY})
${BASE_DIR}/bin/nodeos_start.sh eosio ${EOSIO_PUBKEY} ${EOSIO_PRIVKEY} 8000 9010 ${mode}

PRODUCER_ACCOUNTS="prod.1.1 prod.1.2 prod.1.3"
for account in ${PRODUCER_ACCOUNTS} ; do
  pubkey=$(cat ${KEY_DIR}/${account}.keys | sed -e 's/Public key: \(.*\)/\1/' -e t -e d)
  privkey=$(cat ${KEY_DIR}/${account}.keys | sed -e 's/Public key: \(.*\)/\1/' -e t -e d)
  rpc_port=${account/prod.1./800}
  p2p_port=${account/prod.1./901}
  # And start it up for real
  ${BASE_DIR}/bin/nodeos_start.sh ${account} ${pubkey} ${privkey} ${rpc_port} ${p2p_port} ${mode}
done
