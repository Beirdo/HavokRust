#! /usr/bin/env bash

set -e

BASE_DIR=$(
  cd $(dirname $0)/..
  pwd
)

source ${BASE_DIR}/bin/common_settings.sh

rm -rf ${BLOCKCHAIN_BASEDIR}/* || /bin/true

exec > >(tee -ia ${BLOCKCHAIN_BASEDIR}/generate.log)
exec 2> >(tee -ia ${BLOCKCHAIN_BASEDIR}/generate.log >&2)

mkdir -p ${KEY_DIR}

declare -A features
declare -A featurepriv

featureorder=( GET_SENDER FORWARD_SETCODE ONLY_BILL_FIRST_AUTHORIZER RESTRICT_ACTION_TO_SELF \
               DISALLOW_EMPTY_PRODUCER_SCHEDULE FIX_LINKAUTH_RESTRICTION REPLACE_DEFERRED \
               NO_DUPLICATE_DEFERRED_ID ONLY_LINK_TO_EXISTING_PERMISSION \
               RAM_RESTRICTIONS WEBAUTHN_KEY WTMSIG_BLOCK_SIGNATURES )

features[GET_SENDER]="f0af56d2c5a48d60a4a5b5c903edfb7db3a736a94ed589d0b797df33ff9d3e1d"
featurepriv[GET_SENDER]=eosio
features[FORWARD_SETCODE]="2652f5f96006294109b3dd0bbde63693f55324af452b799ee137a81a905eed25"
featurepriv[FORWARD_SETCODE]=eosio
features[ONLY_BILL_FIRST_AUTHORIZER]="8ba52fe7a3956c5cd3a656a3174b931d3bb2abb45578befc59f283ecd816a405"
featurepriv[ONLY_BILL_FIRST_AUTHORIZER]=eosio
features[RESTRICT_ACTION_TO_SELF]="ad9e3d8f650687709fd68f4b90b41f7d825a365b02c23a636cef88ac2ac00c43"
features[DISALLOW_EMPTY_PRODUCER_SCHEDULE]="68dcaa34c0517d19666e6b33add67351d8c5f69e999ca1e37931bc410a297428"
features[FIX_LINKAUTH_RESTRICTION]="e0fb64b1085cc5538970158d05a009c24e276fb94e1a0bf6a528b48fbc4ff526"
features[REPLACE_DEFERRED]="ef43112c6543b88db2283a2e077278c315ae2c84719a8b25f25cc88565fbea99"
features[NO_DUPLICATE_DEFERRED_ID]="4a90c00d55454dc5b059055ca213579c6ea856967712a56017487886a4d4cc0f"
features[ONLY_LINK_TO_EXISTING_PERMISSION]="1a99a59d87e06e09ec5b028a9cbb7749b4a5ad8819004365d02dc4379a8b7241"
features[RAM_RESTRICTIONS]="4e7bf348da00a945489b2a681749eb56f5de00b900014e137ddae39f48f69d67"
features[WEBAUTHN_KEY]="4fca8bd82bbd181e714e283f83e1b45d95ca5af40fb89ad3977b653c448f78c2"
features[WTMSIG_BLOCK_SIGNATURES]="299dcb6af692324b899b39f16d5a530a33062804e41f09dc97e9f156b4476707"


read_password() {
  local walletname=$1
  local password=$(cat ${KEY_DIR}/${walletname}.password)
  echo ${password}
}

create_wallet() {
  local walletname=$1
  echo "Creating ${walletname} wallet"
  ${CLEOS} wallet create -n ${walletname} -f ${KEY_DIR}/${walletname}.password
  local WALLET_PASSWORD=$(read_password ${walletname})

  echo "Opening ${walletname} wallet"
  ${CLEOS} wallet open -n ${walletname}

  echo "Unlocking ${walletname} wallet"
  ${CLEOS} wallet unlock -n ${walletname} --password ${WALLET_PASSWORD}
}

create_pubkey() {
  local walletname=$1
  local keyname=$2
  ${CLEOS} wallet create_key -n ${walletname} > ${KEY_DIR}/${keyname}.pubkey
  cat ${KEY_DIR}/${keyname}.pubkey | sed -e 's/.*\"\(.*\)\".*/\1/'
}

# -2. Cleanup old wallets
echo "Cleaning up old wallets"
rm ${WALLET_DIR}/* ${KEY_DIR}/* || /bin/true

# -1. Preload the EOSIO Stock keys
cat - >${KEY_DIR}/eosio_stock.keys <<EOF
Public key: EOS6MRyAjQq8ud7hVNYcfnVPJqcVpscN5So8BhtHuGYqET5GDW5CV
Private key: 5KQwrPbwdL6PhXujxW37FSSQZ1JiwsST4cqQzDeyXtP79zkvFD3
EOF
EOSIO_STOCK_PUBKEY=$(cat ${KEY_DIR}/eosio_stock.keys | sed -e 's/Public key: \(.*\)/\1/' -e t -e d)
EOSIO_STOCK_PRIVKEY=$(cat ${KEY_DIR}/eosio_stock.keys | sed -e 's/Private key: \(.*\)/\1/' -e t -e d)

# 0. Startup keosd
echo "Starting keosd"
bash ${BASE_DIR}/bin/keosd_start.sh

# 1. Create development wallet
create_wallet development
DEVELOPMENT_PASSWORD=$(read_password development)

# 4. Create 2 keys in the wallet, one for development, one as the EOSIO key
echo "Creating development and eosio keys"
DEVELOPMENT_PUBKEY=$(create_pubkey development development)
EOSIO_PUBKEY=$(create_pubkey development eosio)

${CLEOS} wallet import -n development --private-key ${EOSIO_STOCK_PRIVKEY}

# 5. Extract the key pairs
echo "Extracting key pairs"
${CLEOS} wallet private_keys -n development --password ${DEVELOPMENT_PASSWORD} >${KEY_DIR}/development_keys.json

# 6. Extract the EOSIO privkey (could use jq, but it's hurting my brain!)
echo "Extracting eosio privkey"
EOSIO_PRIVKEY=$(${BASE_DIR}/bin/extract_private_key.py ${KEY_DIR}/development_keys.json ${EOSIO_PUBKEY})

# 4.5 Create the eosio wallet, add its keys
create_wallet eosio

echo "Importing stock eosio key into eosio wallet"
${CLEOS} wallet import -n eosio --private-key ${EOSIO_STOCK_PRIVKEY}

echo "Importing eosio key into eosio wallet"
${CLEOS} wallet import -n eosio --private-key ${EOSIO_PRIVKEY}

# 7. Generate the genesis.json file with the new EOSIO key
echo "Generating genesis.json"
cat ${BASE_DIR}/genesis/genesis.json.in | sed -e "s/@EOSIO_PUBKEY@/${EOSIO_PUBKEY}/" \
  >${BASE_DIR}/genesis/genesis.json
cp ${BASE_DIR}/genesis/genesis.json ${BLOCKCHAIN_BASEDIR}/genesis.json

# 8. Start the genesis producer
echo "Starting genesis producer"
${BASE_DIR}/bin/nodeos_start.sh eosio ${EOSIO_PUBKEY} ${EOSIO_PRIVKEY} 8000 9010 genesis

# 9. Shut down the genesis producer
sleep 10
echo "Shutting down genesis producer"
${BASE_DIR}/bin/nodeos_stop.sh eosio

# 10.  Start it back up in non-genesis mode
echo "Starting genesis node in non-genesis mode"
${BASE_DIR}/bin/nodeos_start.sh eosio ${EOSIO_PUBKEY} ${EOSIO_PRIVKEY} 8000 9010

# 11. Create the important system accounts and the base MUD accounts
echo "Create system and MUD accounts"
SYSTEM_ACCOUNTS="eosio.bpay eosio.msig eosio.names eosio.ram eosio.ramfee "
SYSTEM_ACCOUNTS+="eosio.saving eosio.stake eosio.token eosio.vpay eosio.rex "
MUD_ACCOUNTS="mud.havokmud banker"

declare -A pubkeys
declare -A privkeys
for account in ${SYSTEM_ACCOUNTS} ${MUD_ACCOUNTS}; do
  echo "Creating account: ${account}"
  pubkeys[${account}]=$(create_pubkey development ${account})
  ${CLEOS} wallet private_keys -n development --password ${DEVELOPMENT_PASSWORD} >${KEY_DIR}/development_keys.json
  privkeys[${account}]=$(${BASE_DIR}/bin/extract_private_key.py ${KEY_DIR}/development_keys.json ${pubkeys[${account}]})
  ${CLEOS} create account eosio ${account} ${EOSIO_PUBKEY} ${pubkeys[${account}]}

  create_wallet ${account}
  ${CLEOS} wallet import -n ${account} --private-key ${privkeys[${account}]}
done

# 12. Install some contracts
echo "Install contracts (eosio.token, eosio.msig, banker)"
EOSIO_OLD_CONTRACTS_DIRECTORY=~/src/eosio.contracts-1.8.x/build/contracts
EOSIO_CONTRACTS_DIRECTORY=~/src/eosio.contracts-1.9.x/build/contracts
HAVOKMUD_CONTRACTS_DIRECTORY=~/src/HavokMud-contracts

${CLEOS} set contract eosio.token ${EOSIO_CONTRACTS_DIRECTORY}/eosio.token/
${CLEOS} set contract eosio.msig ${EOSIO_CONTRACTS_DIRECTORY}/eosio.msig/
${CLEOS} set contract banker ${HAVOKMUD_CONTRACTS_DIRECTORY}/banker/

# 13. Create tokens!
echo "Creating tokens"
TOKEN_QUANTITY="1000000000.0000" # 1 billion
MUD_TOKENS="PP GP EP SP CP"
echo "Creating SYS token"
${CLEOS} push action eosio.token create \
  '[ "eosio", "'"${TOKEN_QUANTITY} SYS"'" ]' \
  -p eosio.token@active

for token in ${MUD_TOKENS}; do
  echo "Creating ${token} token"
  ${CLEOS} push action eosio.token create \
    '[ "mud.havokmud", "'"${TOKEN_QUANTITY} ${token}"'" ]' \
    -p eosio.token@active
done

# 14. Issue the tokens!
echo "Issuing SYS tokens"
${CLEOS} push action eosio.token issue \
  '[ "eosio", "'"${TOKEN_QUANTITY} SYS"'", "Initial Issue" ]' \
  -p eosio@active

for token in ${MUD_TOKENS}; do
  echo "Issuing ${token} tokens"
  ${CLEOS} push action eosio.token issue \
    '[ "mud.havokmud", "'"${TOKEN_QUANTITY} ${token}"'", "Initial Issue" ]' \
    -p mud.havokmud@active
done

# 15. Activate PREACTIVATE_FEATURE
echo "Activating PREACTIVATE feature"
curl -X POST http://127.0.0.1:8000/v1/producer/schedule_protocol_feature_activations \
  -d '{"protocol_features_to_activate": ["0ec7e080177b2c02b278d5088611686b49d739925a92d9bfcacd7fc6b74053bd"]}'
echo ""
sleep 2

# 16. Setup the eosio.system contract with the old version
echo "Setting up old version of eosio.system"
while true; do
  echo trying.
  ${CLEOS} set contract eosio ${EOSIO_OLD_CONTRACTS_DIRECTORY}/eosio.system/ && break
  sleep 1
done
sleep 2

# 17. Turn on a pile of recommended features
echo "Turning on recommended features"
for KEY in "${featureorder[@]}"; do
  echo "Turning on feature: ${KEY}"
  FEATURE_PRIV=${featurepriv[${KEY}]:-eosio@active}
  ${CLEOS} push action eosio activate '["'"${features[$KEY]}"'"]' -p ${FEATURE_PRIV}
done
sleep 2

# 18. Update the eosio.system contract
echo "Updating eosio.system contract"
while true; do
  ${CLEOS} set contract eosio ${EOSIO_CONTRACTS_DIRECTORY}/eosio.system/ && break
  sleep 1
done
sleep 2

# 19. Make the eosio.msig a privileged account
echo "Making eosio.msig a privileged account"
${CLEOS} push action eosio setpriv '["eosio.msig", 1]' -p eosio@active

# 19.5 Make the mud.havokmud a privileged account
${CLEOS} push action eosio setpriv '["mud.havokmud", 1]' -p eosio@active

# 20. Initialize the system token
echo "Initializing syste token"
${CLEOS} push action eosio init '["0", "4,SYS"]' -p eosio@active

# 21. Create the other three local producer accounts
echo "Creating three local producer accounts"
PRODUCER_ACCOUNTS="prod.1.1 prod.1.2 prod.1.3"

declare -A prodpubkeys
declare -A prodprivkeys
for account in ${PRODUCER_ACCOUNTS}; do
  echo "Creating account ${account}"
  ${CLEOS} create key --to-console >${KEY_DIR}/${account}.keys
  prodpubkeys[${account}]=$(cat ${KEY_DIR}/${account}.keys | sed -e 's/Public key: \(.*\)/\1/' -e t -e d)
  prodprivkeys[${account}]=$(cat ${KEY_DIR}/${account}.keys | sed -e 's/Private key: \(.*\)/\1/' -e t -e d)
  ${CLEOS} wallet import -n development --private-key ${prodprivkeys[${account}]}
  ${CLEOS} system newaccount eosio --transfer ${account} ${DEVELOPMENT_PUBKEY} ${prodpubkeys[${account}]} \
    --stake-net "100000000.0000 SYS" --stake-cpu "100000000.0000 SYS" --buy-ram-kbytes 8192
  rpc_port=${account/prod.1./800}
  p2p_port=${account/prod.1./901}
  echo "Setting account ${account} a producer}"
  ${CLEOS} system regproducer ${account} ${prodpubkeys[${account}]}
  # Now do genesis on the new producer
  echo "Running genesis on account ${account}"
  ${BASE_DIR}/bin/nodeos_start.sh ${account} ${prodpubkeys[${account}]} ${prodprivkeys[${account}]} ${rpc_port} ${p2p_port} genesis
  sleep 5
  echo "Shutting down genesis on account ${account}"
  ${BASE_DIR}/bin/nodeos_stop.sh ${account}
  sleep 5
  # And start it up for real
  echo "Restarting account ${account}"
  ${BASE_DIR}/bin/nodeos_start.sh ${account} ${prodpubkeys[${account}]} ${prodprivkeys[${account}]} ${rpc_port} ${p2p_port}
done

# 22. Vote for the new producers
echo "Voting for the new producers"
for voter in ${PRODUCER_ACCOUNTS} ; do
  ${CLEOS} system voteproducer prods ${voter} ${PRODUCER_ACCOUNTS}
done

# 23. Resign all the system accounts - not sure I wanna do this...
echo "Resigning all the system accounts"

echo "Resigning eosio"
${CLEOS} push action eosio updateauth '{"account": "eosio", "permission": "owner", "parent": "", "auth": {"threshold": 1, "keys": [{"key":"'"${DEVELOPMENT_PUBKEY}"'", "weight":1}], "waits": [], "accounts": [{"weight": 1, "permission": {"actor": "eosio.prods", "permission": "active"}}]}}' \
  -p eosio@owner
${CLEOS} push action eosio updateauth '{"account": "eosio", "permission": "active", "parent": "owner", "auth": {"threshold": 1, "keys": [{"key":"'"${EOSIO_PUBKEY}"'", "weight":1}], "waits": [], "accounts": [{"weight": 1, "permission": {"actor": "eosio.prods", "permission": "active"}}]}}' \
  -p eosio@active

for account in ${SYSTEM_ACCOUNTS}; do
  echo "Resigning ${account}"
  ${CLEOS} push action eosio updateauth '{"account": "'${account}'", "permission": "owner", "parent": "", "auth": {"threshold": 1, "keys": [], "waits": [], "accounts": [{"weight": 1, "permission": {"actor": "eosio", "permission": "active"}}]}}' \
    -p ${account}@owner
  ${CLEOS} push action eosio updateauth '{"account": "'${account}'", "permission": "active", "parent": "owner", "auth": {"threshold": 1, "keys": [], "waits": [], "accounts": [{"weight": 1, "permission": {"actor": "eosio", "permission": "active"}}]}}' \
    -p ${account}@active
done

echo "Done"
