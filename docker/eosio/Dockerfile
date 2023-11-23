FROM ubuntu:18.04

USER root
ENV TERM=vt100
ARG ENV
ENV BUILD_MODE=$ENV

RUN apt-get update \
    && apt-get install -y \
	apt-transport-https \
	apt-utils \
	build-essential \
	ca-certificates \
	cmake \
	curl \
	dnsutils \
	g++ \
	git-core \
	iputils-ping \
	jq \
	less \
	libssl1.0-dev \
    libusb-1.0 \
	netcat \
	net-tools \
	psmisc \
	python3 \
	python3-pip \
	screen \
	sudo \
	vim-tiny \
	wget \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -G sudo -d /home/eosio -s /bin/bash -c "EOSIO Account" eosio
WORKDIR /home/eosio
RUN chmod 700 /home/eosio
RUN mkdir -p /etc/sudoers.d
RUN echo "eosio ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/eosio
RUN mkdir /home/eosio/src /home/eosio/eosio
RUN chown -R eosio /home/eosio

USER eosio

ADD docker/eosio/eosio /home/eosio/basechain
WORKDIR /home/eosio/basechain
ENV PATH=$PATH:/home/eosio/eosio/2.0/bin

USER root
RUN chown -R eosio /home/eosio

USER eosio

# keosd
EXPOSE 6666

# nodeos
EXPOSE 8000
