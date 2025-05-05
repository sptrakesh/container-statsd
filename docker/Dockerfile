FROM sptrakesh/cppbase AS base

RUN apk --update --no-cache add curl libunwind \
  && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

COPY Cargo.* /opt/statsd/
COPY src /opt/statsd/src

WORKDIR /opt/statsd
RUN . $HOME/.cargo/env \
  && cargo build --release

FROM alpine
WORKDIR /opt/statsd/bin
COPY --from=base /opt/statsd/target/release/container-statsd .