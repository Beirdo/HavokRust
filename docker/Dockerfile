FROM ubuntu:mantic
USER root
ENV TERM=vt100

RUN apt update \
    && apt install -y \
        net-tools \
        python3 \
        apt-transport-https \
        ca-certificates \
        curl \
        gnupg-agent \
        less \
        sudo \
        vim-tiny \
        screen \
        dnsutils \
        netcat-traditional \
        telnet \
        redis \
        iputils-ping \
    && apt upgrade -y \
    && rm -rf /var/lib/apt/lists/*

RUN ln -sf /bin/bash /bin/sh

ARG ENV
ENV BUILD_MODE=$ENV

RUN useradd -m -G sudo -d /home/havokmud -s /bin/bash -c "HavokMud Account" havokmud
WORKDIR /home/havokmud
RUN mkdir -p /home/havokmud/.config/havokmudrust /home/havokmud/.local/share/havokmudrust
RUN chmod 700 /home/havokmud

RUN mkdir -p /etc/sudoers.d
RUN echo "havokmud ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/havokmud

ADD target/debug/havokmud_rust /usr/bin/havokmud_rust
ADD config .config/havokmudrust/
ADD data .local/share/havokmudrust/
ADD docker/aws-$ENV .aws/
ADD scripts scripts/

RUN chown -R havokmud /home/havokmud

USER havokmud
EXPOSE 3000