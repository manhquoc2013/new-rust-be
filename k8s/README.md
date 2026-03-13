# Kubernetes Deployment Guide

> **Lưu ý**: Tài liệu này tập trung vào việc deploy ứng dụng lên Kubernetes. Để hiểu về kiến trúc hệ thống, luồng xử lý, và các thành phần, xem [FLOW_DOCUMENTATION.md](../FLOW_DOCUMENTATION.md).

## Prerequisites

- Kubernetes cluster (v1.20+)
- kubectl configured
- Docker image built and pushed to registry

## Build and Push Docker Image

```bash
# Build image
docker build -t rust-core-be:latest .

# Tag for your registry (replace with your registry)
docker tag rust-core-be:latest hoanvu/rust-core-be:latest

# Push to registry
docker push hoanvu/rust-core-be:latest
```

## Deploy to Kubernetes

### 1. Create Namespace (Security Best Practice)

**IMPORTANT**: All resources are configured to use a dedicated namespace `rust-core-be` instead of the default namespace. This improves security by isolating resources and preventing conflicts.

```bash
# Create the namespace first
kubectl apply -f k8s/namespace.yaml

# Verify namespace was created
kubectl get namespace rust-core-be
```

### 2. Update Environment Variables

Edit `k8s/deployment.yaml` and update the environment variables in the `env` section with your actual values:

```yaml
env:
  # ... update other values
```

### 3. Deployment Image

The deployment is already configured to use:
```yaml
image: hoanvu/rust-core-be:latest
```

If you need to use a different image, update `k8s/deployment.yaml`:
```yaml
image: your-registry/rust-core-be:latest
```

### 4. Deploy Application

**IMPORTANT**: Deploy resources in the correct order to ensure dependencies are met:

```bash
# Step 1: Create namespace
kubectl apply -f k8s/namespace.yaml

# Step 2: Create ServiceAccount and RBAC (required for secure access control)
kubectl apply -f k8s/serviceaccount.yaml
kubectl apply -f k8s/rbac.yaml

# Step 3: Create ConfigMap and Secrets
kubectl apply -f k8s/configmap.yaml
# kubectl apply -f k8s/secret.yaml.example  # Update values first!

# Step 4: Deploy application (Deployment and Service)
kubectl apply -f k8s/deployment.yaml

# Step 5: Apply NetworkPolicy (optional but recommended)
kubectl apply -f k8s/networkpolicy.yaml

# Step 6: Apply PodDisruptionBudget (ensures high availability)
kubectl apply -f k8s/poddisruptionbudget.yaml

# Or deploy all at once (ensure namespace exists first)
kubectl apply -f k8s/
```

### 5. Verify Deployment

```bash
# Check pods (specify namespace)
kubectl get pods -n rust-core-be -l app=rust-core-be

# Check logs (specify namespace)
kubectl logs -n rust-core-be -l app=rust-core-be -f

# Check service (specify namespace)
kubectl get svc -n rust-core-be rust-core-be-service

# Check all resources in namespace
kubectl get all -n rust-core-be
```

## Testing HA on Kubernetes

Khi chạy HA với nhiều replicas trên K8s, cần kiểm tra: (1) nhiều pod cùng chạy, (2) chỉ một pod consume Kafka (leader lock KeyDB), (3) failover khi pod leader chết, (4) traffic FE phân tán đều, (5) ticket_id không trùng giữa các pod.

### Điều kiện để test HA

- **KeyDB** (Redis): bắt buộc cho leader lock và ETDR/ticket_id. Cấu hình `KEYDB_URL` trong Secret.
- **Kafka**: bật producer (`KAFKA_BOOTSTRAP_SERVERS`) và nếu dùng consumer CHECKIN_HUB_INFO thì bật `KAFKA_CONSUMER_ENABLED=true`.
- **Replicas ≥ 2**: trong `k8s/deployment.yaml` đã có `replicas: 2`.

Cấu hình HA cho consumer (thêm vào ConfigMap hoặc env trong deployment nếu chưa có):

- `KAFKA_CONSUMER_ENABLED=true`
- `KAFKA_TOPIC_CHECKIN_HUB_ONLINE=<topic nhận CHECKIN_HUB_INFO>`
- `KAFKA_CONSUMER_LEADER_LOCK_KEY=kafka_consumer_leader` (mặc định)
- `KAFKA_CONSUMER_LEADER_LOCK_TTL_SECS=60`

### 1. Kiểm tra nhiều pod đang chạy

```bash
kubectl get pods -n rust-core-be -l app=rust-core-be -o wide
```

Kỳ vọng: 2 (hoặc hơn) pod ở trạng thái `Running`, mỗi pod có IP riêng. `TICKET_ID_IP_SUFFIX` trong mỗi pod lấy từ `status.podIP` nên mỗi replica có suffix khác nhau → ticket_id không trùng.

### 2. Kiểm tra leader lock (chỉ một pod consume Kafka)

Pod nào giành được lock KeyDB sẽ log `[Kafka] consumer leader lock acquired`; các pod còn lại log `consumer not leader, waiting` (mức debug) và không consume.

```bash
# Log tất cả pod, lọc theo Kafka consumer
kubectl logs -n rust-core-be -l app=rust-core-be --tail=500 | grep -E "consumer leader lock|CHECKIN_HUB_INFO|Kafka consumer"
```

Kỳ vọng: đúng **một** pod có dòng `[Kafka] consumer leader lock acquired` (và `leader_id=<pod-name>`). Các pod khác không có dòng này.

Xem log từng pod để xác định pod nào là leader:

```bash
for p in $(kubectl get pods -n rust-core-be -l app=rust-core-be -o name); do
  echo "=== $p ==="
  kubectl logs -n rust-core-be $p --tail=200 | grep -E "consumer leader|BOO1|BOO2|STARTUP"
done
```

### 3. Test failover: kill pod leader

- Ghi nhận tên pod đang giữ leader (từ bước 2).
- Xóa pod đó; K8s sẽ tạo pod mới.

```bash
LEADER_POD="rust-core-be-xxxx-yyyy"   # thay bằng tên pod leader
kubectl delete pod -n rust-core-be $LEADER_POD
```

- Trong vòng TTL (mặc định 60s), lock KeyDB hết hạn. Pod còn lại (hoặc pod mới) sẽ giành lock và log `[Kafka] consumer leader lock acquired`.
- Kiểm tra:

```bash
kubectl get pods -n rust-core-be -l app=rust-core-be
kubectl logs -n rust-core-be -l app=rust-core-be --tail=100 | grep "consumer leader lock acquired"
```

Kỳ vọng: vẫn chỉ **một** pod có dòng "consumer leader lock acquired" (pod khác với pod vừa xóa).

### 4. Kiểm tra traffic FE phân tán (load balance)

Service `rust-core-be-service` dùng `ClusterIP` với nhiều endpoint (mỗi pod một endpoint). Client kết nối qua Service sẽ được K8s load-balance tới các pod.

- Gửi nhiều request FE (CONNECT, CHECKIN, …) tới địa chỉ Service (trong cluster) hoặc qua Ingress/LoadBalancer.
- Xem log theo từng pod để thấy request rải đều các pod (ví dụ theo `request_id` hoặc `conn_id` trong log).

```bash
# Ví dụ: log vài dòng gần nhất của từng pod
kubectl logs -n rust-core-be -l app=rust-core-be --prefix=true --tail=20
```

Mỗi pod có connection BOO1/BOO2 riêng; request tới pod nào thì dùng connection của pod đó.

### 5. Kiểm tra PodDisruptionBudget (PDB)

Khi drain node hoặc rollout, PDB đảm bảo luôn có ít nhất 1 pod available:

```bash
kubectl get pdb -n rust-core-be
kubectl describe pdb rust-core-be-pdb -n rust-core-be
```

Kỳ vọng: `minAvailable: 1`, và khi có voluntary disruption thì K8s không evict quá số pod cho phép.

### 6. Tóm tắt checklist HA

| Kiểm tra | Lệnh / Cách làm |
|----------|------------------|
| Nhiều replica chạy | `kubectl get pods -n rust-core-be -l app=rust-core-be` |
| Chỉ 1 pod consume Kafka | `kubectl logs ... \| grep "consumer leader lock acquired"` → đúng 1 pod |
| Failover khi kill leader | `kubectl delete pod <leader>` → pod khác giành lock |
| Traffic phân tán | Gửi request FE qua Service, xem log từng pod |
| Ticket_id không trùng | Mỗi pod có `TICKET_ID_IP_SUFFIX` từ pod IP (trong deployment) |
| PDB | `kubectl get pdb -n rust-core-be` |

## Configuration

### Update Environment Variables

Edit `k8s/deployment.yaml` and update the environment variables in the `env` section, then apply:

```bash
kubectl apply -f k8s/deployment.yaml -n rust-core-be
```

The deployment will automatically restart pods with the new configuration.

### Using ConfigMap and Secrets

For better security, consider moving sensitive values to Secrets:

```bash
# Create secret from file (recommended)
kubectl create secret generic rust-core-be-secrets \
  --from-literal=key-enc='YOUR_KEY_ENC' \
  --namespace=rust-core-be

# Or apply from example file (update values first!)
kubectl apply -f k8s/secret.yaml.example
```

**IMPORTANT**: Deployment now uses Secrets for all sensitive data (passwords, keys). ConfigMap only contains non-sensitive configuration.

**QUAN TRỌNG**: Deployment hiện sử dụng Secrets cho tất cả dữ liệu nhạy cảm (passwords, keys). ConfigMap chỉ chứa cấu hình không nhạy cảm.

#### Create Secrets (REQUIRED before deployment)

```bash
# Method 1: Create secret using kubectl (RECOMMENDED)
# Cách 1: Tạo secret bằng kubectl (KHUYẾN NGHỊ)
kubectl create secret generic rust-core-be-secrets \
  --from-literal=key-enc='YOUR_KEY_ENC' \
  --from-literal=vdtc-username='YOUR_VDTC_USERNAME' \
  --from-literal=vdtc-password='YOUR_VDTC_PASSWORD' \
  --from-literal=vdtc-key-enc='YOUR_VDTC_KEY_ENC' \
  --from-literal=keydb-url='redis://username:password@host:port' \
  --namespace=rust-core-be

# Method 2: Create secret from file (if you have secret.yaml with real values)
# Cách 2: Tạo secret từ file (nếu bạn có secret.yaml với giá trị thật)
# WARNING: Never commit secret.yaml with real values to git!
# CẢNH BÁO: Không bao giờ commit secret.yaml với giá trị thật vào git!
kubectl apply -f k8s/secret.yaml.example  # Update values first!

# Method 3: Use sealed-secrets, external-secrets, Vault, etc.
# Cách 3: Sử dụng sealed-secrets, external-secrets, Vault, etc.
```

#### Verify Secrets

```bash
# List secrets
kubectl get secrets -n rust-core-be

# View secret (values are base64 encoded)
kubectl get secret rust-core-be-secrets -n rust-core-be -o yaml

# Decode a specific key (example)
kubectl get secret rust-core-be-secrets -n rust-core-be -o jsonpath='{.data.pass}' | base64 -d
```

## Scaling

```bash
# Scale to 3 replicas (specify namespace)
kubectl scale deployment rust-core-be --replicas=3 -n rust-core-be

# Or edit deployment
kubectl edit deployment rust-core-be -n rust-core-be
```

## Troubleshooting

### View Pod Logs

```bash
# View logs for all pods with label
kubectl logs -n rust-core-be -l app=rust-core-be -f

# View logs for specific pod
kubectl logs -n rust-core-be <pod-name> -f
```

### Describe Pod

```bash
kubectl describe pod <pod-name> -n rust-core-be
```

### Exec into Pod

```bash
kubectl exec -it <pod-name> -n rust-core-be -- /bin/bash
```

### Check Events

```bash
# Events in namespace
kubectl get events -n rust-core-be --sort-by=.metadata.creationTimestamp

# All events
kubectl get events --all-namespaces --sort-by=.metadata.creationTimestamp
```

### Check Resource Status

```bash
# Check all resources in namespace
kubectl get all -n rust-core-be

# Check deployment status
kubectl get deployment -n rust-core-be

# Check pod status
kubectl get pods -n rust-core-be
```

## Security Best Practices

### ✅ Security Features Implemented

#### 1. **Namespace Isolation**
- ✅ Dedicated namespace `rust-core-be` instead of `default`
- ✅ Resource isolation prevents conflicts with other applications
- ✅ Easier to apply RBAC policies per namespace

#### 2. **ServiceAccount & RBAC**
- ✅ Dedicated ServiceAccount (`rust-core-be-sa`) instead of default
- ✅ **automountServiceAccountToken: false** - Service account tokens are NOT automatically mounted
- ✅ **Defense in depth**: RBAC configured but unused (application doesn't need Kubernetes API access)
- ✅ Role-based access control (RBAC) with minimal permissions (defensive measure)
- ✅ Role allows only necessary API access (ConfigMap, Secret reads) if tokens are enabled
- ✅ Prevents unauthorized API access and reduces attack surface

#### 3. **Pod Security Standards**
- ✅ **Namespace-level enforcement**: Pod Security Standards restricted mode enforced
- ✅ **Labels**: `pod-security.kubernetes.io/enforce: restricted`
- ✅ **Audit & Warning**: All violations are audited and warned
- ✅ **Version**: Uses latest Pod Security Standards version

#### 4. **Pod Security Context**
- ✅ **runAsNonRoot**: Pods run as non-root user (UID 1000)
- ✅ **runAsUser/runAsGroup**: Explicitly set to 1000 (appuser)
- ✅ **fsGroup**: Sets filesystem group ownership (1000)
- ✅ **supplementalGroups**: Empty array (no additional groups)
- ✅ **allowPrivilegeEscalation: false**: Prevents privilege escalation
- ✅ **capabilities.drop: ALL**: Drops all Linux capabilities
- ✅ **seccompProfile**: Uses RuntimeDefault seccomp profile
- ✅ **Sysctls**: Commented out (only safe sysctls allowed in restricted mode)

#### 5. **Container Security Context**
- ✅ **runAsNonRoot**: Container runs as non-root user (UID 1000)
- ✅ **runAsUser/runAsGroup**: Explicitly set to 1000 (appuser)
- ✅ **readOnlyRootFilesystem: true**: Root filesystem is read-only
- ✅ **tmpfs mounts**: `/tmp` and `/var/tmp` mounted as tmpfs for writable directories
- ✅ **allowPrivilegeEscalation: false**: Prevents privilege escalation
- ✅ **capabilities.drop: ALL**: All Linux capabilities dropped
- ✅ **seccompProfile**: RuntimeDefault seccomp profile enabled
- ✅ **AppArmor/SELinux**: Ready for additional profiles if needed

#### 6. **Network Policies**
- ✅ NetworkPolicy restricts ingress/egress traffic
- ✅ Only allows necessary network communication
- ✅ DNS resolution allowed for Kubernetes DNS
- ✅ Outbound traffic to VDTC/VETC servers configured

### Security Configuration Files

| File | Purpose |
|------|---------|
| `namespace.yaml` | Creates isolated namespace with Pod Security Standards |
| `serviceaccount.yaml` | Dedicated service account with **automountServiceAccountToken: false** |
| `rbac.yaml` | Role and RoleBinding for API access control (defensive measure, currently unused) |
| `networkpolicy.yaml` | Network traffic restrictions |
| `poddisruptionbudget.yaml` | Ensures high availability during disruptions |
| `deployment.yaml` | Contains comprehensive security contexts (pod & container level) + **automountServiceAccountToken: false** |

### Verifying Security Configuration

```bash
# Check Pod Security Standards on namespace
kubectl get namespace rust-core-be -o yaml | grep pod-security

# Check ServiceAccount (verify automountServiceAccountToken is false)
kubectl get serviceaccount rust-core-be-sa -n rust-core-be -o yaml | grep automountServiceAccountToken
# Should show: automountServiceAccountToken: false

# Check RBAC
kubectl get role,rolebinding -n rust-core-be

# Verify service account token is NOT mounted in pod
kubectl get pod <pod-name> -n rust-core-be -o yaml | grep -i "serviceAccount\|token"
# Should NOT show /var/run/secrets/kubernetes.io/serviceaccount volume mount

# Verify token directory doesn't exist in running pod
kubectl exec -n rust-core-be <pod-name> -- ls -la /var/run/secrets/kubernetes.io/serviceaccount 2>&1
# Should fail: No such file or directory

# Check Pod Security Context (pod level)
kubectl get pod <pod-name> -n rust-core-be -o yaml | grep -A 30 "spec:" | grep -A 20 "securityContext"

# Check Container Security Context
kubectl get pod <pod-name> -n rust-core-be -o jsonpath='{.spec.containers[0].securityContext}'

# Check NetworkPolicy
kubectl get networkpolicy -n rust-core-be

# Check PodDisruptionBudget
kubectl get poddisruptionbudget -n rust-core-be

# Verify pod is running as non-root
kubectl exec -n rust-core-be <pod-name> -- id
# Should show: uid=1000(appuser) gid=1000(appuser)

# Verify read-only root filesystem
kubectl exec -n rust-core-be <pod-name> -- touch /test.txt
# Should fail with: touch: /test.txt: Read-only file system

# Verify capabilities are dropped
kubectl exec -n rust-core-be <pod-name> -- capsh --print
# Should show no capabilities
```

### Additional Security Recommendations

1. **Use Secrets instead of ConfigMap for sensitive data**
   ```bash
   kubectl create secret generic rust-core-be-secrets \
     --from-literal=key-enc='YOUR_KEY' \
     --namespace=rust-core-be
   ```

2. **Pod Security Standards** ✅ Already Enabled
   - Namespace has Pod Security Standards restricted mode enforced
   - All pods must comply with restricted policy
   - Violations are audited and warned

3. **Use Image Pull Secrets** if using private registry
   ```yaml
   # Add to deployment.yaml
   spec:
     imagePullSecrets:
       - name: registry-secret
   ```

4. **Service Account Token Management** ✅ Already Configured
   - `automountServiceAccountToken: false` prevents unnecessary token mounting
   - Application doesn't need Kubernetes API access, so tokens are disabled
   - Reduces attack surface - even if pod is compromised, tokens won't be available
   - RBAC is configured as defensive measure but unused

5. **Regular Security Audits**
   ```bash
   # Use kubectl to audit resources
   kubectl get all -n rust-core-be -o yaml > audit.yaml
   
   # Use tools like kube-score or kubeaudit
   kube-score score k8s/deployment.yaml
   
   # Verify service account token is not mounted
   kubectl get pod <pod-name> -n rust-core-be -o jsonpath='{.spec.automountServiceAccountToken}'
   # Should show: false
   ```

## Cleanup

```bash
# Delete all resources in namespace
kubectl delete -f k8s/ -n rust-core-be

# Or delete namespace (this will delete all resources in it)
kubectl delete namespace rust-core-be

# Verify deletion
kubectl get namespace rust-core-be
```
