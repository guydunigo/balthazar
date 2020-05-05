FROM rust:alpine as BUILD

WORKDIR /balthazar
COPY . .
RUN ls

RUN cargo build --release

FROM alpine

COPY --from=BUILD target/release/balthacli /usr/bin/balthacli

ENTRYPOINT ["balthacli"]
CMD [ "worker" ]
