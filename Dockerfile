# Multi-stage build cho Rust application
# Stage 1: Build
# Su dung Rust 1.93+ de ho tro cac dependencies moi nhat (time, tracing, v.v.)
# Using Rust 1.93+ to support latest dependencies (time, tracing, etc.)
# SECURITY: Pin base image by digest to prevent dependency confusion (immutable reference)
FROM rust:1.93-slim@sha256:760ad1d638d70ebbd0c61e06210e1289cbe45ff6425e3ea6e01241de3e14d08e AS builder

# Install build dependencies
# Cai dat cac thu vien can thiet de build: SSL va ODBC
# Install required libraries for building: SSL and ODBC
# SECURITY: Pin package versions to prevent dependency confusion (explicit =<version>)
# To refresh versions: docker run --rm rust:1.93-slim apt-cache madison <package>
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config=1.8.1-4 \
    libssl-dev=3.5.4-1~deb13u2 \
    unixodbc-dev=2.3.12-2 \
    && apt-mark hold pkg-config libssl-dev unixodbc-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy dependency files first (for better caching)
# Copy Cargo.lock if it exists (optional for libraries, required for binaries)
# SECURITY: Cargo.lock is required for reproducible builds and prevents dependency confusion
COPY Cargo.toml ./
COPY Cargo.lock ./

# Verify Cargo.lock exists (required for --locked flag)
RUN test -f Cargo.lock || (echo "ERROR: Cargo.lock is required for secure builds" && exit 1)

# Configure Cargo to use only crates.io registry (prevent dependency confusion)
# SECURITY: Explicitly configure cargo to use only trusted registry
RUN mkdir -p /usr/local/cargo && \
    echo '[registry]' > /usr/local/cargo/config.toml && \
    echo 'default = "crates-io"' >> /usr/local/cargo/config.toml && \
    echo '[registries.crates-io]' >> /usr/local/cargo/config.toml && \
    echo 'protocol = "sparse"' >> /usr/local/cargo/config.toml

# Create a dummy src directory to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this layer will be cached if Cargo.toml doesn't change)
# Xoa dummy binary sau khi build dependencies de cargo phai rebuild voi source that
# Remove dummy binary after building dependencies so cargo must rebuild with real source
# SECURITY: --locked flag enforces exact versions from Cargo.lock, preventing dependency confusion
RUN cargo build --release --locked && rm -rf src && rm -f target/release/Rust-core-transaction

# Copy source code
COPY src ./src

# Build the actual application (force rebuild)
# Build ung dung that (buoc phai rebuild)
# SECURITY: --locked flag enforces exact versions from Cargo.lock
RUN touch src/main.rs && cargo build --release --locked

# Stage 2: Runtime
# SECURITY: Pin base image by digest to prevent dependency confusion (immutable reference)
FROM debian:bookworm-slim@sha256:98f4b71de414932439ac6ac690d7060df1f27161073c5036a7553723881bffbe

# Install runtime dependencies
# Cai dat cac thu vien can thiet de chay: SSL certificates va ODBC runtime (bao gom libodbcinst.so)
# Install required libraries for runtime: SSL certificates and ODBC runtime (including libodbcinst.so)
# SECURITY: Pin package versions to prevent dependency confusion (explicit =<version>)
# --no-install-recommends reduces attack surface by avoiding unnecessary packages
# To refresh versions: docker run --rm debian:bookworm-slim apt-cache madison <package>
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates=20230311+deb12u1 \
    unixodbc=2.3.11-2+deb12u1 \
    libodbc1=2.3.11-2+deb12u1 \
    odbcinst1debian2=2.3.11-2+deb12u1 \
    && apt-mark hold ca-certificates unixodbc libodbc1 odbcinst1debian2 \
    && rm -rf /var/lib/apt/lists/*

# Create app user for security
RUN useradd -m -u 1000 appuser

# Set working directory
WORKDIR /app

# Copy libs/ (may contain Altibase ODBC .so or only README). If all three .so present, install driver.
# Copy libs/ (co the chua Altibase ODBC .so hoac chi README). Neu du 3 file .so thi cai driver.
COPY libs/ /tmp/libs/
RUN set -e; \
    if [ -f /tmp/libs/libalticapi_sl.so ] && [ -f /tmp/libs/libodbccli_sl.so ] && [ -f /tmp/libs/libaltibase_odbc-64bit-ul64.so ]; then \
        cp /tmp/libs/libaltibase_odbc-64bit-ul64.so /usr/lib/ && \
        cp /tmp/libs/libodbccli_sl.so /usr/lib/ && \
        cp /tmp/libs/libalticapi_sl.so /usr/lib/ && \
        ln -sf /usr/lib/x86_64-linux-gnu/libodbcinst.so.2 /usr/lib/libodbcinst.so && \
        ln -sf /usr/lib/libodbccli_sl.so /usr/lib/libodbccli.so && \
        ln -sf /usr/lib/libalticapi_sl.so /usr/lib/libalticapi.so && \
        ldconfig && \
        echo "[ALTIBASE_HDB_ODBC_64bit]" > /etc/odbcinst.ini && \
        echo "Description = Altibase HDB ODBC Driver 64-bit" >> /etc/odbcinst.ini && \
        echo "Driver = /usr/lib/libaltibase_odbc-64bit-ul64.so" >> /etc/odbcinst.ini && \
        echo "Setup = /usr/lib/libaltibase_odbc-64bit-ul64.so" >> /etc/odbcinst.ini && \
        echo "FileUsage = 1" >> /etc/odbcinst.ini; \
    else \
        echo "Altibase ODBC libs not in libs/ - image runs without Altibase driver."; \
    fi; \
    rm -rf /tmp/libs

# Copy binary from builder stage
# Binary name is based on package name in Cargo.toml: "Rust-core-transaction"
COPY --from=builder /app/target/release/Rust-core-transaction /app/rust-core-be

# Change ownership to appuser
RUN chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose port (default 19002, can be overridden via env)
EXPOSE 19002

# Note: Health checks are handled by Kubernetes livenessProbe and readinessProbe
# See k8s/deployment.yaml for configuration

# Run the application
CMD ["./rust-core-be"]
