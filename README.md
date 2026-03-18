# Rust Core Transaction

Middleware xử lý giao dịch thu phí điện tử (ETC - Electronic Toll Collection) được viết bằng Rust. Hệ thống đóng vai trò gateway trung gian giữa Front-End clients và các backend systems (VETC/VDTC).

## 📋 Tổng Quan

**Rust-core-transaction** là một hệ thống middleware xử lý giao dịch ETC với các tính năng chính:

- Nhận và xử lý requests từ FE clients qua TCP
- Mã hóa/giải mã dữ liệu bằng AES-CBC với PKCS7 padding
- Định tuyến request đến đúng backend system (VETC hoặc VDTC) dựa trên eTag prefix
- Quản lý kết nối và session với các backend systems
- Xử lý các giao dịch: CONNECT, HANDSHAKE, CHECKIN, COMMIT, ROLLBACK, TERMINATE
- Tích hợp Kafka (tùy chọn) để đồng bộ events lên HUB
- Cache dữ liệu (TOLL, TOLL_LANE, PRICE, closed_cycle_transition_stage, TOLL_FEE_LIST, SUBSCRIPTION_HISTORY) với KeyDB/Redis và Moka

## 🚀 Quick Start

### Yêu Cầu Hệ Thống

**Bắt buộc:**

- **Rust 1.70+** (edition 2021). Khuyến nghị Rust 1.93+ khi build Docker (trùng với Dockerfile).
- **Hệ điều hành**: Linux, macOS hoặc Windows (build và chạy binary).

**Tùy chọn (theo môi trường triển khai):**

- **Altibase** (RATING_DB, MEDIATION_DB, CRM_DB): Khi cần DB (cache, session, TCOC, transport stage). Cấu hình qua `RATING_DB_*`, `MEDIATION_DB_*`, `CRM_DB_*` trong `env.template`. Docker: đặt thư viện ODBC Altibase vào thư mục `libs/` (xem [Dockerfile](Dockerfile)).
- **KeyDB/Redis**: Cache phân tán, fallback khi DB lỗi, ETDR sync. Cấu hình `KEYDB_URL` (mặc định có sẵn nếu không set).
- **Kafka**: Đồng bộ event checkin/checkout lên HUB. Set `KAFKA_BOOTSTRAP_SERVERS` để bật.

**Build trên máy local (không dùng Docker):**

- Linux: thường cần `pkg-config`, `libssl-dev`, `unixodbc-dev` (hoặc tương đương) nếu dùng DB.
- macOS/Windows: cài Rust qua rustup; ODBC driver Altibase nếu cần kết nối DB.

### Cài Đặt & Chạy

```bash
# Clone repository
git clone <repository_url>
cd new-rust-be

# Copy và cấu hình environment variables
cp env.template .env
# Chỉnh sửa .env với các giá trị thực tế (PORT_LISTEN, VDTC_*, VETC_*, BOOX1/2_PREFIX_LIST, ...)

# Build project (debug)
cargo build

# Chạy
cargo run
```

**Build bản release (tối ưu):**

```bash
cargo build --release
# Binary: target/release/Rust-core-transaction (hoặc target\release\Rust-core-transaction.exe trên Windows)
./target/release/Rust-core-transaction
```

### Cấu Hình

Xem `env.template` để biết các biến môi trường cần thiết. Các biến quan trọng:

- `PORT_LISTEN`: Port lắng nghe FE clients (mặc định: 8080)
- `VDTC_HOST`, `VDTC_PORT`, `VDTC_USERNAME`, `VDTC_PASSWORD`, `VDTC_KEY_ENC`: Cấu hình VDTC
- `VETC_HOST`, `VETC_PORT`, `VETC_USERNAME`, `VETC_PASSWORD`, `VETC_KEY_ENC`: Cấu hình VETC
- `BOOX1_PREFIX_LIST`, `BOOX2_PREFIX_LIST`: Prefix để phân loại eTag
- `KEYDB_URL`: URL KeyDB/Redis (tùy chọn)
- `KAFKA_BOOTSTRAP_SERVERS`: Kafka bootstrap servers (tùy chọn)

Chi tiết đầy đủ xem [FLOW_DOCUMENTATION.md](FLOW_DOCUMENTATION.md#-cấu-hình-và-biến-môi-trường).

## 🧪 Testing

### Unit Tests (không cần DB / không cần dịch vụ ngoài)

```bash
cargo test
```

Chạy toàn bộ unit tests không có `#[ignore]`. Hiện có tests trong: `utils/epoch_datetime`, `utils/ip_denylist`, `price_ticket_type`. Các test này không cần database hay Redis.

### Repository Tests (cần DB – bỏ qua mặc định)

Các test tích hợp với DB được đánh dấu `#[ignore]`. Để chạy:

1. Cấu hình biến môi trường DB (xem `env.template`): `RATING_DB_*`, `MEDIATION_DB_*`, `CRM_DB_*`.
2. Chạy:

```bash
cargo test -- --ignored
```

Các test này nằm trong `db/repositories/` (tcoc_sessions, tcoc_request, tcoc_response, tcoc_connection_server, tcoc_users, transport_transaction_stage, transport_trans_stage_tcd).

### Công Cụ Test FE (FE Test Client)

Dự án có sẵn **FE Test Client** (Java/Swing) để test kết nối và gửi bản tin giao thức FE (TCOC) tới backend Rust: CONNECT, HANDSHAKE, CHECKIN, COMMIT, ROLLBACK, CHECKOUT_*, LOOKUP_VEHICLE, QUERY_VEHICLE_BOO, v.v.

**Yêu cầu:** Java 11+, Maven 3.x

**Build và chạy:**

```bash
cd tools/fe-test-client
mvn compile
mvn exec:java -Dexec.mainClass="fetest.FeTestClientApp"
```

Hoặc đóng gói JAR rồi chạy:

```bash
mvn package
java -jar target/fe-test-client-1.0.0.jar
```

Chi tiết cách dùng (profile, eTag, refill từ response, …): **[tools/fe-test-client/README.md](tools/fe-test-client/README.md)**.

## 📦 Build và Deploy

### Build Docker Image

**Lưu ý:** Build Docker **yêu cầu có file `Cargo.lock`** trong thư mục gốc (reproducible builds). Nếu chưa có: `cargo generate-lockfile`.

```bash
# Build image (tag mặc định từ version trong Cargo.toml)
docker build -t rust-core-be:latest .

# Build cho nền tảng linux/amd64 (ví dụ khi build trên Mac M1/M2)
docker build --platform linux/amd64 -t rust-core-be:latest .
```

Image dùng Rust 1.93-slim và Debian bookworm-slim. Nếu đặt đủ 3 file thư viện Altibase ODBC vào thư mục `libs/` (xem [Dockerfile](Dockerfile)), image sẽ cài driver ODBC Altibase; không có thì chạy bình thường nhưng không kết nối Altibase.

### Build và Push Image (script)

Trên Mac/Linux dùng script `docker-build-push.sh`:

```bash
# Chỉ build, tag lấy từ Cargo.toml (version)
./docker-build-push.sh

# Build và push lên registry
./docker-build-push.sh -r ghcr.io/myorg
./docker-build-push.sh -r manhquoc00   # ví dụ Docker Hub: manhquoc00/rust-core-be:<version>

# Tùy chọn: chỉ định tên image và tag
./docker-build-push.sh -r ghcr.io/myorg -n rust-core-be -t 0.1.0
```

**Tham số:**

- `-r REPO`: Registry/repo prefix (bắt buộc khi muốn push). Nếu bỏ qua thì chỉ build local.
- `-n NAME`: Tên image (mặc định: `rust-core-be`).
- `-t TAG`: Tag image (mặc định: version trong `Cargo.toml`).

### Chạy bằng Docker / Docker Compose

Cách chạy container (docker run, docker-compose), biến môi trường, troubleshooting: **[DOCKER.md](DOCKER.md)**.

### Kubernetes Deployment

Xem hướng dẫn chi tiết trong [k8s/README.md](k8s/README.md).

**Tóm tắt nhanh:**

```bash
# Tạo namespace
kubectl apply -f k8s/namespace.yaml

# Deploy resources (theo thứ tự)
kubectl apply -f k8s/serviceaccount.yaml
kubectl apply -f k8s/rbac.yaml
kubectl apply -f k8s/configmap.yaml
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/networkpolicy.yaml
kubectl apply -f k8s/poddisruptionbudget.yaml

# Hoặc deploy tất cả cùng lúc
kubectl apply -f k8s/
```

**Lưu ý:** Cần tạo Secrets trước khi deploy (xem [k8s/README.md](k8s/README.md)).

## 📚 Tài Liệu

### Tài Liệu Chính

- **[FLOW_DOCUMENTATION.md](FLOW_DOCUMENTATION.md)**: Tài liệu đầy đủ về kiến trúc hệ thống, luồng xử lý, các thành phần, cấu hình, và hướng dẫn cho thành viên mới.

### Tài Liệu Chuyên Sâu

- **[DOCKER.md](DOCKER.md)**: Chạy ứng dụng bằng Docker / Docker Compose, cấu hình, troubleshooting.
- **[k8s/README.md](k8s/README.md)**: Hướng dẫn deploy lên Kubernetes (namespace, RBAC, ConfigMap, Secrets, deployment, scaling).
- **[src/db/README.md](src/db/README.md)**: Database Repository Pattern, entity, mapper, repository.
- **[tools/fe-test-client/README.md](tools/fe-test-client/README.md)**: Công cụ test FE (Java) – profile, command, eTag, refill.

### Cấu Trúc Dự Án

```
new-rust-be/
├── src/
│   ├── main.rs              # Entry point: logging, config, DB pools, cache, Kafka, spawn tasks
│   ├── configs/             # Configuration (env, kafka, DB pools)
│   ├── network/             # TCP server, router, BOO1/BOO2 clients
│   ├── handlers/            # Request handlers (CONNECT, HANDSHAKE, CHECKIN, COMMIT, ROLLBACK, TERMINATE)
│   ├── logic/               # Logic handler (forward reply)
│   ├── models/              # Message structures (TCOC, VETC, VDTC, ETDR, events)
│   ├── services/            # Business services (Kafka, TCOC, toll_fee, transport stage)
│   ├── cache/               # Cache (KeyDB, Moka; toll, lane, price, closed_cycle_transition_stage, ...)
│   ├── db/                  # Database repositories
│   ├── crypto/              # AES encryption/decryption
│   └── logging/             # File logger, cleanup, statistics
├── tools/
│   └── fe-test-client/      # FE Test Client (Java/Swing) – test giao thức FE
├── k8s/                     # Kubernetes manifests
├── libs/                    # Tùy chọn: thư viện ODBC Altibase cho Docker image
├── FLOW_DOCUMENTATION.md    # Tài liệu đầy đủ về hệ thống
├── DOCKER.md                # Hướng dẫn Docker / Docker Compose
├── docker-build-push.sh     # Script build & push image (Mac/Linux)
└── env.template             # Template biến môi trường
```

Chi tiết cấu trúc xem [FLOW_DOCUMENTATION.md](FLOW_DOCUMENTATION.md#2-cấu-trúc-thư-mục).

## 🔍 Điểm Bắt Đầu Đọc Code

1. **`src/main.rs`**: Hiểu cách hệ thống khởi động và các tasks được spawn
2. **`src/network/server.rs`**: Hiểu cách nhận và xử lý connections
3. **`src/handlers/processor.rs`**: Hiểu cách route requests
4. **`src/handlers/checkin/`**: Ví dụ về handler phức tạp (có gọi backend)
5. **`src/network/boo1_client.rs`**: Hiểu cách kết nối với backend

Chi tiết xem [FLOW_DOCUMENTATION.md](FLOW_DOCUMENTATION.md#3-điểm-bắt-đầu-đọc-code).

## 🛠️ Development

### Format Code

```bash
cargo fmt
```

### Lint

```bash
cargo clippy
```

### Debug

Sử dụng `RUST_LOG` để điều chỉnh log level:

```bash
RUST_LOG=debug cargo run
```

## 📝 Changelog

Xem [FLOW_DOCUMENTATION.md](FLOW_DOCUMENTATION.md#-lịch-sử-phiên-bản-changelog) để biết lịch sử các phiên bản.

## 🔗 Liên Kết

- **Rust Async Book**: https://rust-lang.github.io/async-book/
- **Tokio Documentation**: https://tokio.rs/

---

**Cập nhật lần cuối**: 2026-03-18
