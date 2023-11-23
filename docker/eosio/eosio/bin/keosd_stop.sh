#! /usr/bin/env bash

BASE_DIR=$(
  cd $(dirname $0)/..
  pwd
)
source ${BASE_DIR}/bin/common_settings.sh

if [ -f ${WALLET_DIR}/keosd.pid ] ; then
  pid=$(cat ${WALLET_DIR}/keosd.pid)
  echo ${pid}
  kill ${pid}
  rm -f ${WALLET_DIR}/keosd.pid
  echo -ne "Stopping keosd"
  while true ; do
    [ ! -d /proc/$pid/fd ] && break
    echo -ne "."
    sleep 1
  done
  echo -ne "\rkeosd stopped.\n"
fi
