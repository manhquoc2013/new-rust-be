# Docker Desktop - Hướng dẫn chạy ứng dụng

**Lưu ý:** Port mặc định của ứng dụng trong code là **8080** (khi không set `PORT_LISTEN`). Các ví dụ bên dưới dùng port **19002** cho container; bạn có thể đổi sang 8080 hoặc port khác tùy môi trường.

## Mục lục

- [Cách 1: Docker Compose (Khuyến nghị)](#cách-1-sử-dụng-docker-compose-khuyến-nghị)
- [Cách 2: Docker Run (PowerShell)](#cách-2-sử-dụng-docker-run-powershell---windows)
- [Cách 3: Docker Run (Bash/Linux/Mac)](#cách-3-sử-dụng-docker-run-bashlinuxmac)
- [Quản lý Container](#quản-lý-container)
- [Build Image](#build-image)
- [Troubleshooting](#troubleshooting)
- [Best Practices & Security](#best-practices--security)
- [Tài liệu liên quan](#tài-liệu-liên-quan)

## Cách 1: Sử dụng Docker Compose (Khuyến nghị)

### 1. Tạo file `.env` cho cấu hình

**QUAN TRỌNG:** File `docker-compose.yml` sử dụng `env_file` để load các biến môi trường từ file `.env` nhằm bảo mật thông tin nhạy cảm (passwords, keys).

Tạo file `.env` trong thư mục gốc của project với nội dung:

```bash
# Port configuration
PORT_LISTEN=19002

# BOOX prefix lists
BOOX1_PREFIX_LIST=3416214B88,89301100013
BOOX2_PREFIX_LIST=3416214B94,89301100013

# VDTC configuration
VDTC_HOST=host.docker.internal
VDTC_PORT=8300
VDTC_USERNAME=vdtc
VDTC_PASSWORD=123456a@
VDTC_KEY_ENC=2F5ADF381CA64BDE
DELAY_TIME_RECONNECT_VDTC=5

# Kafka configuration (optional - uncomment if needed)
# KAFKA_BOOTSTRAP_SERVERS=kafka:9092
# KAFKA_TOPIC_TRANS_HUB_ONLINE=topics.hub.trans.online   # Một topic cho cả checkin và checkout (xem FLOW_DOCUMENTATION.md)
# KAFKA_DATA_VERSION=1.0
# KAFKA_QUEUE_CAPACITY=10000
# KAFKA_SEND_BATCH_SIZE=50
# KAFKA_RETRIES=10
# KAFKA_RETRY_INITIAL_DELAY_MS=1000
# KAFKA_RETRY_MAX_DELAY_MS=60000
# KAFKA_REQUEST_TIMEOUT_MS=5000
```

**BẢO MẬT:**

- **KHÔNG** commit file `.env` vào git (đã có trong `.gitignore`)
- Thay đổi các giá trị mặc định (đặc biệt là passwords và keys) với giá trị thực tế của bạn
- Trong production, nên sử dụng Docker secrets hoặc các hệ thống quản lý secrets khác

### 2. Chỉnh sửa cấu hình (nếu cần)

Chỉnh sửa file `docker-compose.yml` nếu cần thay đổi:

- Port mapping
- Network configuration
- Logging settings
- Các biến môi trường không nhạy cảm (có thể override giá trị từ `.env`)

### 3. Chạy ứng dụng

```bash
# Build image (nếu chưa có)
docker build -t hoanvu/rust-core-be:latest .

# Chạy với docker-compose
docker-compose up -d

# Xem logs
docker-compose logs -f

# Dừng ứng dụng
docker-compose down
```

## Cách 2: Sử dụng Docker Run (PowerShell - Windows)

### 1. Chỉnh sửa script (PowerShell)

Chỉnh sửa file `docker-run.ps1` và cập nhật các giá trị environment variables.

### 2. Chạy script (PowerShell)

```powershell
# Chạy script
.\docker-run.ps1

# Hoặc chạy trực tiếp lệnh docker run
docker run -d `
  --name rust-core-be `
  --restart unless-stopped `
  -p 19002:19002 `
  --add-host=host.docker.internal:host-gateway `
  -e PORT_LISTEN=19002 `
  -e BOOX1_PREFIX_LIST=3416214B88,89301100013 `
  -e BOOX2_PREFIX_LIST=3416214B94,89301100013 `
  -e VDTC_HOST=host.docker.internal `
  -e VDTC_PORT=8300 `
  -e VDTC_USERNAME=vdtc `
  -e VDTC_PASSWORD=123456a@ `
  -e VDTC_KEY_ENC=2F5ADF381CA64BDE `
  -e DELAY_TIME_RECONNECT_VDTC=5 `
  hoanvu/rust-core-be:latest
```

## Cách 3: Sử dụng Docker Run (Bash/Linux/Mac)

### 1. Chỉnh sửa script (Bash)

Chỉnh sửa file `docker-run.sh` và cập nhật các giá trị environment variables.

### 2. Chạy script (Bash)

```bash
# Cấp quyền thực thi
chmod +x docker-run.sh

# Chạy script
./docker-run.sh

# Hoặc chạy trực tiếp lệnh docker run
docker run -d \
  --name rust-core-be \
  --restart unless-stopped \
  -p 19002:19002 \
  --add-host=host.docker.internal:host-gateway \
  -e PORT_LISTEN=19002 \
  -e BOOX1_PREFIX_LIST=3416214B88,89301100013 \
  -e BOOX2_PREFIX_LIST=3416214B94,89301100013 \
  -e VDTC_HOST=host.docker.internal \
  -e VDTC_PORT=8300 \
  -e VDTC_USERNAME=vdtc \
  -e VDTC_PASSWORD=123456a@ \
  -e VDTC_KEY_ENC=2F5ADF381CA64BDE \
  -e DELAY_TIME_RECONNECT_VDTC=5 \
  hoanvu/rust-core-be:latest
```

## Quản lý Container

### Xem logs

```bash
# Xem logs real-time
docker logs -f rust-core-be

# Xem logs với docker-compose
docker-compose logs -f
```

### Dừng/Start/Restart container

```bash
# Dừng container
docker stop rust-core-be

# Start container
docker start rust-core-be

# Restart container
docker restart rust-core-be

# Xóa container
docker rm -f rust-core-be

# Với docker-compose
docker-compose stop
docker-compose start
docker-compose restart
docker-compose down
```

### Kiểm tra trạng thái

```bash
# Xem danh sách containers
docker ps -a

# Xem thông tin container
docker inspect rust-core-be

# Xem resource usage
docker stats rust-core-be
```

### Exec vào container

```bash
# Vào trong container
docker exec -it rust-core-be /bin/bash

# Hoặc với sh (nếu bash không có)
docker exec -it rust-core-be /bin/sh
```

## Build Image

### Build từ source

Nếu bạn cần build image từ source:

```bash
# Build image
docker build -t hoanvu/rust-core-be:latest .

# Tag và push lên registry (nếu cần)
docker tag hoanvu/rust-core-be:latest your-registry/rust-core-be:latest
docker push your-registry/rust-core-be:latest
```

### Thông tin về Dockerfile

Dockerfile sử dụng multi-stage build để tối ưu kích thước image:

**Stage 1 (Builder):**

- Sử dụng `rust:1.93-slim` để build ứng dụng
- Cài đặt các dependencies cần thiết: SSL, ODBC
- Build ứng dụng với `--locked` flag để đảm bảo reproducible builds

**Stage 2 (Runtime):**

- Sử dụng `debian:bookworm-slim` làm base image nhẹ
- Cài đặt runtime dependencies: SSL certificates, ODBC runtime
- Copy Altibase ODBC libraries từ thư mục `libs/`
- Tạo non-root user (`appuser`) để chạy ứng dụng (bảo mật)
- Copy binary từ builder stage

**Bảo mật:**

- Pin base images và package versions bằng digest/version để tránh dependency confusion
- Sử dụng non-root user để chạy ứng dụng
- Cấu hình Cargo chỉ sử dụng crates.io registry
- Yêu cầu `Cargo.lock` để đảm bảo reproducible builds

### Yêu cầu để build

- File `Cargo.lock` phải tồn tại (required cho `--locked` flag)
- Thư mục `libs/` phải chứa các Altibase ODBC libraries:
  - `libaltibase_odbc-64bit-ul64.so`
  - `libodbccli_sl.so`
  - `libalticapi_sl.so`

## Troubleshooting

### Container restart liên tục

```bash
# Xem logs để tìm lỗi
docker logs rust-core-be

# Xem logs real-time
docker-compose logs -f

# Kiểm tra container status
docker ps -a | grep rust-core-be

# Xem logs chi tiết với timestamps
docker logs --tail 100 --timestamps rust-core-be
```

### Container không start

```bash
# Xem logs để tìm lỗi
docker logs rust-core-be

# Kiểm tra container status
docker ps -a | grep rust-core-be

# Kiểm tra xem image đã được build chưa
docker images | grep rust-core-be
```

### Port đã được sử dụng

Nếu port 19002 đã được sử dụng, thay đổi port mapping:

```yaml
# Trong docker-compose.yml
ports:
  - "19003:19002"  # Host port:Container port
```

Và cập nhật PORT_LISTEN:

```yaml
environment:
  - PORT_LISTEN=19002  # Port bên trong container
```

Hoặc trong docker run:

```bash
-p 19003:19002  # Host port:Container port
-e PORT_LISTEN=19002  # Port bên trong container
```

**Lưu ý:**

- Dockerfile expose port **19002** mặc định
- Ứng dụng mặc định lắng nghe port **8080** nếu không set `PORT_LISTEN`
- Trong các ví dụ, cả host port và container port đều là **19002**
- Nếu thay đổi port mapping, cần cập nhật cả `PORT_LISTEN` để khớp với container port

### Cập nhật environment variables

**Với Docker Compose (khuyến nghị):**

1. Dừng container hiện tại:

   ```bash
   docker-compose down
   ```

2. Chỉnh sửa file `.env` với các giá trị mới

3. Chạy lại:

   ```bash
   docker-compose up -d
   ```

**Lưu ý:** Nếu chỉ thay đổi biến môi trường trong `.env`, có thể restart container mà không cần rebuild:

```bash
docker-compose restart
```

**Với Docker Run:**

1. Dừng và xóa container cũ:

   ```bash
   docker stop rust-core-be
   docker rm rust-core-be
   ```

2. Chạy lại với giá trị mới (chỉnh sửa script `docker-run.sh` hoặc `docker-run.ps1`):

   ```bash
   # Linux/Mac
   ./docker-run.sh

   # Windows PowerShell
   .\docker-run.ps1
   ```

### Environment Variables

**Các biến môi trường quan trọng:**

| Biến | Mô tả | Bắt buộc | Mặc định |
|------|-------|----------|----------|
| `PORT_LISTEN` | Port mà ứng dụng lắng nghe | Không | 8080 |
| `BOOX1_PREFIX_LIST` | Danh sách prefix cho BOOX1 | Có | - |
| `BOOX2_PREFIX_LIST` | Danh sách prefix cho BOOX2 | Có | - |
| `VDTC_HOST` | Host của VDTC server | Có | - |
| `VDTC_PORT` | Port của VDTC server | Có | - |
| `VDTC_USERNAME` | Username cho VDTC | Có | - |
| `VDTC_PASSWORD` | Password cho VDTC | Có | - |
| `VDTC_KEY_ENC` | Encryption key cho VDTC | Có | - |
| `DELAY_TIME_RECONNECT_VDTC` | Thời gian delay trước khi reconnect VDTC (giây) | Không | - |

**Kafka (tùy chọn):**

- `KAFKA_BOOTSTRAP_SERVERS`: Kafka bootstrap servers
- `KAFKA_TOPIC_TRANS_HUB_ONLINE`: Một topic cho cả checkin và checkout events (mặc định: topics.hub.trans.online)
- `KAFKA_DATA_VERSION`: Version của data format
- `KAFKA_QUEUE_CAPACITY`: Capacity của internal queue
- `KAFKA_SEND_BATCH_SIZE`: Batch size khi gửi messages
- `KAFKA_RETRIES`: Số lần retry
- `KAFKA_RETRY_INITIAL_DELAY_MS`: Delay ban đầu khi retry (ms)
- `KAFKA_RETRY_MAX_DELAY_MS`: Delay tối đa khi retry (ms)
- Chi tiết: xem [FLOW_DOCUMENTATION.md](FLOW_DOCUMENTATION.md#4-luồng-kafka-tùy-chọn)

### VDTC_HOST không kết nối được

Nếu không kết nối được đến VDTC server:

**Windows/Mac Docker Desktop:**

- Sử dụng `host.docker.internal` (đã cấu hình sẵn trong `docker-compose.yml` và scripts)

**Linux:**

- Sử dụng IP thực của VDTC server (ví dụ: `10.254.247.64`)
- Hoặc sử dụng `172.17.0.1` (Docker bridge gateway)
- Hoặc thêm `--add-host=host.docker.internal:host-gateway` trong docker run
- Với docker-compose, `extra_hosts` đã được cấu hình sẵn

**VDTC server trên máy khác:**

- Sử dụng IP thực của máy đó (ví dụ: `10.254.247.64`)

### Network configuration

**Docker Compose:**

- Container sử dụng network `rust-network` (bridge driver)
- Có thể kết nối với các container khác trong cùng network
- Để kết nối với container khác, sử dụng tên service làm hostname

**Docker Run:**

- Container chạy trên default bridge network
- Để kết nối với container khác, có thể:
  - Tạo custom network: `docker network create rust-network`
  - Chạy container với `--network rust-network`
  - Kết nối với container khác bằng container name hoặc IP

### Logging configuration

**Docker Compose:**

- Logging driver: `json-file`
- Max size: 10MB per file
- Max files: 3 (tổng cộng ~30MB logs)
- Logs được rotate tự động

**Xem logs:**

```bash
# Xem logs real-time
docker-compose logs -f

# Xem logs của service cụ thể
docker-compose logs -f rust-core-be

# Xem logs với timestamps
docker-compose logs -f --timestamps
```

## Best Practices & Security

### Bảo mật

1. **Không commit file `.env` vào git:**
   - File `.env` chứa thông tin nhạy cảm (passwords, keys)
   - Đảm bảo `.env` đã có trong `.gitignore`
   - Sử dụng `.env.example` để làm template (không chứa giá trị thực)

2. **Sử dụng Docker secrets trong production:**
   - Thay vì hardcode passwords trong scripts hoặc docker-compose.yml
   - Sử dụng Docker secrets hoặc các hệ thống quản lý secrets (HashiCorp Vault, AWS Secrets Manager, etc.)

3. **Pin image và package versions:**
   - Dockerfile đã pin base images và package versions để tránh dependency confusion
   - Khi update, kiểm tra và test kỹ trước khi deploy

4. **Chạy container với non-root user:**
   - Dockerfile đã tạo user `appuser` (UID 1000) để chạy ứng dụng
   - Giảm thiểu rủi ro nếu container bị compromise

5. **Giới hạn quyền truy cập:**
   - Chỉ expose các ports cần thiết
   - Sử dụng Docker networks để isolate containers
   - Không mount volumes không cần thiết với quyền write

### Monitoring & Maintenance

1. **Theo dõi logs:**
   - Sử dụng `docker-compose logs -f` để theo dõi logs real-time
   - Logs được rotate tự động (max 30MB)
   - Có thể tích hợp với log aggregation tools (ELK, Loki, etc.)

2. **Resource limits:**
   - Có thể thêm resource limits trong `docker-compose.yml`:

   ```yaml
   deploy:
     resources:
       limits:
         cpus: '1.0'
         memory: 512M
       reservations:
         cpus: '0.5'
         memory: 256M
   ```

3. **Health checks:**
   - Trong Kubernetes, health checks được cấu hình trong `k8s/deployment.yaml`
   - Với Docker Compose, có thể thêm healthcheck:

   ```yaml
   healthcheck:
     test: ["CMD", "curl", "-f", "http://localhost:19002/health"]
     interval: 30s
     timeout: 10s
     retries: 3
     start_period: 40s
   ```

4. **Backup:**
   - Backup file `.env` và cấu hình
   - Backup database nếu có
   - Backup logs quan trọng nếu cần

### Troubleshooting Tips

1. **Container không start:**
   - Kiểm tra logs: `docker logs rust-core-be`
   - Kiểm tra environment variables: `docker inspect rust-core-be`
   - Kiểm tra port conflicts: `netstat -an | grep 19002` (Linux) hoặc `netstat -an | findstr 19002` (Windows)

2. **Kết nối đến VDTC server thất bại:**
   - Kiểm tra network connectivity từ container: `docker exec -it rust-core-be ping VDTC_HOST`
   - Kiểm tra firewall rules
   - Kiểm tra `host.docker.internal` mapping (Windows/Mac) hoặc IP thực (Linux)

3. **Build image thất bại:**
   - Đảm bảo `Cargo.lock` tồn tại
   - Đảm bảo thư mục `libs/` chứa đầy đủ Altibase ODBC libraries
   - Kiểm tra disk space: `df -h` (Linux) hoặc `Get-PSDrive C` (Windows PowerShell)

4. **Performance issues:**
   - Kiểm tra resource usage: `docker stats rust-core-be`
   - Kiểm tra logs để tìm bottlenecks
   - Cân nhắc tăng resource limits nếu cần

## Tài liệu liên quan

- `Dockerfile`: Cấu hình build image
- `docker-compose.yml`: Cấu hình Docker Compose
- `docker-run.sh`: Script chạy container (Linux/Mac)
- `docker-run.ps1`: Script chạy container (Windows PowerShell)
- `.dockerignore`: Files/folders bị loại trừ khi build image
