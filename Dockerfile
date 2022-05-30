FROM rust:latest AS builder

RUN update-ca-certificates

ENV USER=runner
ENV UID=1000

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    "${USER}"

WORKDIR /app

COPY . .

RUN cargo build --bin mtg --release

# Final Image
FROM gcr.io/distroless/cc

COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

WORKDIR /app
COPY --from=builder /app/target/release/mtg .

USER runner:runner

ENV PORT=8000

CMD ["/app/mtg"]
