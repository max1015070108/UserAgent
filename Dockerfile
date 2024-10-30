FROM ubuntu:22.04 as builder

# Install dependencies
RUN apt-get update && \
    apt-get install -y curl build-essential libssl-dev pkg-config cmake ca-certificates

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Copy the source code
COPY . /usr/src/myapp
WORKDIR /usr/src/myapp

# Build the Rust application
RUN cargo build --release --bin server

FROM ubuntu:22.04

# Install runtime dependencies and development tools
RUN apt-get update && \
    apt-get install -y libssl-dev pkg-config net-tools lsof ca-certificates

# Add custom CA certificates if needed
COPY --from=builder /usr/src/myapp/certs/custom-ca.crt /usr/local/share/ca-certificates/
RUN update-ca-certificates

# Set environment variables
ENV GITHUB_CLIENT_SECRET=${GITHUB_CLIENT_SECRET}
ENV GITHUB_CLIENT_ID=${GITHUB_CLIENT_ID}
ENV GOOGLE_CLIENT_SECRET=${GOOGLE_CLIENT_SECRET}
ENV GOOGLE_CLIENT_ID=${GOOGLE_CLIENT_ID}
ENV MYSQL_URL=${MYSQL_URL}
ENV AWS_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID}
ENV AWS_SECRET_ACCESS_KEY=${AWS_SECRET_ACCESS_KEY}
ENV AWS_DEFAULT_REGION=${AWS_DEFAULT_REGION}

# Copy the built application from the builder stage
COPY --from=builder /usr/src/myapp/target/release/server /usr/local/bin/server

# Set the entrypoint
CMD ["/usr/local/bin/server"]
