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
    util-linux  \
    wget \
    xz-utils && \
    rm -rf /var/lib/apt/lists/*

RUN locale-gen en_US.UTF-8

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

FROM builder AS rustup

USER ubuntu

RUN curl https://sh.rustup.rs -sSf | \
    sh -s -- --default-toolchain stable -y

FROM builder AS nvm

USER ubuntu

RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.2/install.sh | \
    bash

FROM builder

# common packages
RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    bash-completion \
    dstat \
    htop \
    libtool \
    lsof \
    ripgrep \
    tmux \
    less \
    vim && \
    rm -rf /var/lib/apt/lists/*

COPY --link --from=shournal /usr/local/bin/shournal* /usr/local/bin/
COPY --link --from=shournal /usr/local/lib/shournal* /usr/local/lib/
COPY --link --from=shournal /usr/local/share/shournal* /usr/local/share/

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
WORKDIR /home/ubuntu

RUN echo '. "$HOME/.cargo/env"' >>.bashrc
