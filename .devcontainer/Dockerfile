FROM ubuntu:rolling

RUN apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y \
        build-essential \
        ca-certificates \
        curl \
        git \
        ssh \
        libssl-dev \
        clang \
        lld \
        libelf-dev \
        elfutils \
        libbpf-dev \
        cmake \
        libgflags-dev \
        libdouble-conversion-dev \
        libboost-all-dev \
        libatomic-ops-dev \
        libatomic1 \
        libgoogle-glog-dev \
        openjdk-17-jdk-headless \
        iotop \
        atop \
        ncdu \
        htop \
        pkg-config && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*


ENV PATH=/root/.cargo/bin:$PATH

RUN echo "(curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly --no-modify-path) && rustup default nightly" > /install-rust.sh && chmod 755 /install-rust.sh && bash /install-rust.sh 

RUN cargo install sqlx-cli

ENTRYPOINT ["tail", "-f", "/dev/null"]