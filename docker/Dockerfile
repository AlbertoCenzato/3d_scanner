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

# install node20 to allow for github actions to run
RUN apt install -y ca-certificates curl gnupg
RUN curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /usr/share/keyrings/nodesource.gpg
RUN NODE_MAJOR=20 && \
	echo "deb [signed-by=/usr/share/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
	apt update && \
	apt install -y nodejs
