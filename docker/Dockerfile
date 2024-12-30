FROM dtcooper/raspberrypi-os:bookworm

# install rust
RUN apt update && apt install -y curl && \
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
	. $HOME/.cargo/env && \
	apt remove -y curl && \
	apt autoremove -y
ENV PATH="/root/.cargo/bin:$PATH"

# install required packages
RUN apt install -y clang libcamera-dev pkg-config

# install useful development tools
RUN apt install -y git