#! /usr/bin/env bash

BASE_DIR=$(
  cd $(dirname $0)/..
  pwd
)
source ${BASE_DIR}/bin/common_settings.sh

if [ $# -lt 1 ] ; then
  echo "Usage: $0 accountname"
  exit 1
fi

ACCOUNT=$1

BLOCKCHAIN_DIR=${BASE_DIR}/blockchain/${ACCOUNT}

if [ -f ${BLOCKCHAIN_DIR}/nodeos.pid ] ; then
  pid=$(cat ${BLOCKCHAIN_DIR}/nodeos.pid)
  echo ${pid}
  kill ${pid}
  rm -f ${BLOCKCHAIN_DIR}/nodeos.pid
  echo -ne "Stopping NodeOS for account ${ACCOUNT}"
  while true ; do
    [ ! -d /proc/$pid/fd ] && break
    echo -ne "."
    sleep 1
  done
  echo -ne "\rNodeOS for account ${ACCOUNT} stopped.\n"
fi