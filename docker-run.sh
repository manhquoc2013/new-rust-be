#!/bin/bash

# Docker run command for rust-core-transaction
# Update the environment variables below with your actual values

docker run -d \
  --name rust-core-transaction \
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
  hoanvu/rust-core-transaction:latest

echo "Container started. Check logs with: docker logs -f rust-core-transaction"
