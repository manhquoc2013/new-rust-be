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
- Cache dữ liệu (TOLL, TOLL_LANE, PRICE, TOLL_CYCLE) với KeyDB/Redis và Moka

## 🚀 Quick Start

### Yêu Cầu

- Rust 1.70+ (edition 2021)
- Altibase database (RATING_DB, MEDIATION_DB, CRM_DB) - tùy chọn
- KeyDB/Redis - tùy chọn (có default)
- Kafka - tùy chọn (chỉ khi cần đồng bộ events)

### Cài Đặt

```bash
# Clone repository
git clone <repository_url>
cd new-rust

# Copy và cấu hình environment variables
cp env.template .env
# Chỉnh sửa .env với các giá trị thực tế

# Build project
cargo build

# Chạy project
cargo run
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

### Unit Tests (không cần DB)

```bash
cargo test
```

Chạy các unit tests không cần database (hiện có: `route_graph`).

### Repository Tests (cần DB)

Đặt các biến môi trường `RATING_DB_*` / `MEDIATION_DB_*` / `CRM_DB_*` (xem `env.template`) rồi chạy:

```bash
cargo test -- --ignored
```

## 📦 Build và Deploy

### Docker Build

```bash
docker build -t rust-core-be:latest .
```

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

**Lưu ý**: Cần tạo Secrets trước khi deploy (xem [k8s/README.md](k8s/README.md#create-secrets-required-before-deployment)).

## 📚 Tài Liệu

### Tài Liệu Chính

- **[FLOW_DOCUMENTATION.md](FLOW_DOCUMENTATION.md)**: Tài liệu đầy đủ về kiến trúc hệ thống, luồng xử lý, các thành phần, cấu hình, và hướng dẫn cho thành viên mới.

### Tài Liệu Chuyên Sâu

- **[k8s/README.md](k8s/README.md)**: Hướng dẫn deploy lên Kubernetes với các best practices về security, scaling, troubleshooting.
- **[src/db/README.md](src/db/README.md)**: Hướng dẫn sử dụng Database Repository Pattern, cách tạo entity, mapper, repository.

### Cấu Trúc Dự Án

```
new-rust/
├── src/
│   ├── main.rs              # Entry point: logging, config, DB pools, cache, Kafka, spawn tasks
│   ├── configs/             # Configuration (env, kafka, DB pools)
│   ├── network/             # TCP server, router, BOO1/BOO2 clients
│   ├── handlers/            # Request handlers (CONNECT, HANDSHAKE, CHECKIN, COMMIT, ROLLBACK, TERMINATE)
│   ├── logic/               # Business logic handler
│   ├── models/              # Message structures (TCOC, VETC, VDTC, ETDR, events)
│   ├── services/            # Business services (Kafka, TCOC, toll fee, route graph)
│   ├── cache/               # Cache (KeyDB, Moka, toll/lane/price/cycle cache)
│   ├── db/                  # Database repositories
│   ├── crypto/              # AES encryption/decryption
│   └── logging/             # File logger, cleanup
├── k8s/                     # Kubernetes manifests
├── FLOW_DOCUMENTATION.md    # Tài liệu đầy đủ về hệ thống
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

**Cập nhật lần cuối**: 2026-03-01
