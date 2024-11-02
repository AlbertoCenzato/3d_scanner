FROM dtcooper/raspberrypi-os:bullseye

# install rust
RUN apt update && apt install -y curl && \
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
	. $HOME/.cargo/env && \
	apt remove -y curl

# install required packages
RUN apt install -y \
	clang \
	libcamera-dev \ 
	pkg-config

RUN apt install -y git