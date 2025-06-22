FROM ubuntu:noble AS base

FROM base AS builder

RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    autoconf automake autotools-dev  \
    b3sum \
    bsdutils \
    build-essential \
    bzip2 \
    ca-certificates \
    cmake \
    make \
    coreutils \
    curl  \
    file \
    gzip \
    gcc \
    g++ \
    git \
    jq \
    libtool \
    linux-headers-generic \
    locales \
    patch \
    qtbase5-dev libqt5sql5-sqlite \
    tar \
    uuid-dev libcap-dev uuid-runtime \
    uidmap \
    util-linux  \
    wget \
    xz-utils && \
    rm -rf /var/lib/apt/lists/*

RUN locale-gen en_US.UTF-8

FROM builder AS mitmproxy-arm64
ADD --link https://downloads.mitmproxy.org/11.1.3/mitmproxy-11.1.3-linux-aarch64.tar.gz /archive.tar.gz

FROM builder AS mitmproxy-amd64
ADD --link https://downloads.mitmproxy.org/11.1.3/mitmproxy-11.1.3-linux-aarch64.tar.gz /archive.tar.gz

FROM mitmproxy-$TARGETARCH AS mitmproxy
RUN tar -xzf /archive.tar.gz -C /usr/local/bin
RUN test -x /usr/local/bin/mitmproxy

FROM builder AS shournal

ADD --link https://github.com/kolloch/shournal/archive/refs/heads/pagesize.tar.gz /workspace.tar.gz
RUN mkdir -p /workspace
RUN tar -xzf /workspace.tar.gz --strip-components=1 -C /workspace
RUN chown -R ubuntu /workspace

USER ubuntu
RUN mkdir -p /workspace/build
WORKDIR /workspace/build
RUN cmake -DSHOURNAL_EDITION=docker ..
RUN make

USER root
RUN make install

FROM builder AS cargo_binstall

USER ubuntu
ADD --chown=ubuntu --link \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
    /tmp/install.sh
RUN ls -al /tmp/install.sh &&bash /tmp/install.sh
# => Creates /root/.cargo/bin/cargo-binstall

FROM builder AS rustup

USER ubuntu

RUN curl https://sh.rustup.rs -sSf | \
    sh -s -- --default-toolchain stable -y
COPY --chown=ubuntu --link --from=cargo_binstall \
    /home/ubuntu/.cargo/bin/cargo-binstall \
    /home/ubuntu/.cargo/bin/cargo-binstall

FROM builder AS nvm

USER ubuntu

RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.2/install.sh | \
    bash

FROM builder AS build-root

RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    debootstrap && \
    rm -rf /var/lib/apt/lists/*

RUN debootstrap --variant=minbase  --include=build-essential,git,curl noble /build-root && \
    rm -rf /build-root/var/cache && \
    mkdir /build-root/var/cache

FROM builder

RUN yes | unminimize

# common packages
RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    bash-completion \
    clang \
    dstat \
    htop \
    iproute2 \
    libtool \
    lsof \
    manpages \
    manpages-dev \
    manpages-posix \
    manpages-posix-dev \
    man-db \
    mtr-tiny \
    openssl \
    python3 \
    python3-pip \
    python3-setuptools \
    python3-wheel \
    ripgrep \
    ssh \
    tmux \
    less \
    ltrace \
    strace \
    sudo \
    vim && \
    rm -rf /var/lib/apt/lists/*

RUN echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers

COPY --link --from=shournal /usr/local/bin/shournal* /usr/local/bin/
COPY --link --from=shournal /usr/local/lib/shournal* /usr/local/lib/
COPY --link --from=shournal /usr/local/share/shournal* /usr/local/share/
RUN groupadd shournalmsenter

COPY --link --from=mitmproxy /usr/local/bin/* /usr/local/bin/

WORKDIR /home/ubuntu

ENV USERNAME=ubuntu
RUN SNIPPET="export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.bash_history" \
    && mkdir /commandhistory \
    && touch /commandhistory/.bash_history \
    && chown -R $USERNAME /commandhistory \
    && echo "$SNIPPET" >> "/home/$USERNAME/.bashrc"

COPY --chown=ubuntu --link --from=nvm /home/ubuntu/.nvm /home/ubuntu/.nvm
COPY --chown=ubuntu --link --from=nvm /home/ubuntu/.bashrc /home/ubuntu/.bashrc
COPY --chown=ubuntu --link --from=rustup /home/ubuntu/.cargo /home/ubuntu/.cargo
COPY --chown=ubuntu --link --from=rustup /home/ubuntu/.rustup /home/ubuntu/.rustup
USER ubuntu
RUN echo '. "$HOME/.cargo/env"' >>.bashrc

RUN "$HOME/.cargo/bin/cargo" binstall --strategies crate-meta-data jj-cli
RUN echo 'source <(COMPLETE=bash jj)' >> ~/.bashrc

# Generate certificates in ~/.mitmproxy
RUN mitmdump -r /dev/null -n
USER root
# https://askubuntu.com/questions/73287/how-do-i-install-a-root-certificate/94861#94861
RUN mkdir /usr/local/share/ca-certificates/extra
RUN openssl x509 -in /home/ubuntu/.mitmproxy/mitmproxy-ca.pem -inform PEM -out /usr/local/share/ca-certificates/extra/mitmproxy-ca.crt
RUN update-ca-certificates

COPY --link --from=build-root /build-root /build-root

USER ubuntu
