FROM rust:1.64 as build

# Creates an empty project
RUN USER=root cargo new --bin cashflow
WORKDIR /cashflow

# Copies manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# Caches dependencies
RUN cargo build --release
RUN rm src/*.rs

# Adds source tree
COPY ./README.md ./
COPY ./src ./src

# Builds final release
RUN rm ./target/release/deps/cashflow*
RUN cargo build --release

# Runtime base to use
FROM debian:bullseye-slim
# Copies build artifact from earlier stage
COPY --from=build /cashflow/target/release/cashflow /usr/local/bin/cashflow
# Sets runtime working directory
WORKDIR /working
# Sets runtime to execute binary. Using ENTRYPOINT to make it easier to pass args to cashflow
ENTRYPOINT ["/usr/local/bin/cashflow"]
