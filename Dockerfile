FROM mcr.microsoft.com/devcontainers/rust:1-1-bullseye

RUN apt update && apt install libdbus-1-dev pkg-config -y
