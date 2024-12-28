FROM dtcooper/raspberrypi-os:bullseye

# install rust
RUN apt update && apt install -y curl && \
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
	. $HOME/.cargo/env && \
	apt remove -y curl && \
	apt autoremove -y
ENV PATH="/root/.cargo/bin:$PATH"

RUN apt install -y wget && \
	wget https://github.com/mozilla/sccache/releases/download/v0.9.0/sccache-v0.9.0-armv7-unknown-linux-musleabi.tar.gz && \
	tar -xzf sccache-v0.9.0-armv7-unknown-linux-musleabi.tar.gz && \
	mkdir -p /opt/ && \
	mv sccache-v0.9.0-armv7-unknown-linux-musleabi /opt/sccache && \
	rm sccache-v0.9.0-armv7-unknown-linux-musleabi.tar.gz && \
	apt remove -y wget
ENV PATH="/opt/sccache:$PATH"
ENV RUSTC_WRAPPER=/opt/sccache/sccache
ENV SCCACHE_DIR=/mnt/sccache

# install required packages
RUN apt install -y \
	clang \
	libcamera-dev \ 
	pkg-config

RUN apt install -y git