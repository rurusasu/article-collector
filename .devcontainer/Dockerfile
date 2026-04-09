FROM mcr.microsoft.com/devcontainers/base:ubuntu

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    curl \
    jq \
    bats \
    shellcheck \
  && rm -rf /var/lib/apt/lists/*
