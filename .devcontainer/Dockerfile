# See here for image contents: https://github.com/microsoft/vscode-dev-containers/tree/v0.238.1/containers/rust/.devcontainer/base.Dockerfile

# [Choice] Debian OS version (buster, bullseye)
ARG VARIANT="bullseye"
FROM mcr.microsoft.com/vscode/devcontainers/base:0-${VARIANT}

RUN apt-get update
RUN apt-get install -y build-essential openssl pkg-config
RUN apt-get dist-upgrade -y

# # Install Rust
# RUN curl https://sh.rustup.rs -sSf | sh -s -- -y \
#     && . "$HOME/.cargo/env" \
#     && rustup component add rust-analyzer clippy \
#     && rustup install stable \
#     && rustup default stable

# [Optional] Uncomment this section to install additional packages.
# RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
#     && apt-get -y install --no-install-recommends <your-package-list-here>
