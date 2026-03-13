#!/usr/bin/env bash
#
# Build Docker image and push to repository.
# Equivalent of docker-build-push.ps1 for Mac/Linux.
#
# Usage:
#   ./docker-build-push.sh                    # build only, tag from Cargo.toml
#   ./docker-build-push.sh -r ghcr.io/myorg    # build and push
#   ./docker-build-push.sh -r ghcr.io/myorg -n rust-core-be -t 1.0.0
#
# Options:
#   -r REPO    Registry/repo prefix (e.g. ghcr.io/myorg). If omitted, only build.
#   -n NAME    Image name (default: rust-core-be)
#   -t TAG     Image tag (default: version from Cargo.toml)
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

IMAGE_NAME="rust-core-be"
REPOSITORY=""
TAG=""

while getopts "r:n:t:h" opt; do
  case $opt in
    r) REPOSITORY="$OPTARG" ;;
    n) IMAGE_NAME="$OPTARG" ;;
    t) TAG="$OPTARG" ;;
    h) echo "Usage: $0 [-r REPO] [-n IMAGE_NAME] [-t TAG]"; exit 0 ;;
    *) exit 1 ;;
  esac
done

if [ -z "$TAG" ]; then
  CARGO_PATH="$SCRIPT_DIR/Cargo.toml"
  if [ ! -f "$CARGO_PATH" ]; then
    echo "Error: Cargo.toml not found at $CARGO_PATH" >&2
    exit 1
  fi
  TAG=$(grep -E '^version\s*=' "$CARGO_PATH" | sed -E 's/.*"([^"]+)".*/\1/' | tr -d ' ')
  if [ -z "$TAG" ]; then
    echo "Error: Could not parse version from Cargo.toml" >&2
    exit 1
  fi
  echo "Using tag from Cargo.toml: $TAG"
else
  echo "Using tag: $TAG"
fi

LOCAL_IMAGE="${IMAGE_NAME}:${TAG}"
echo "Building Docker image: $LOCAL_IMAGE"
docker build -t "$LOCAL_IMAGE" .

if [ -n "$REPOSITORY" ]; then
  REMOTE_IMAGE="${REPOSITORY%/}/${IMAGE_NAME}:${TAG}"
  echo "Tagging as: $REMOTE_IMAGE"
  docker tag "$LOCAL_IMAGE" "$REMOTE_IMAGE"
  echo "Pushing to repository: $REMOTE_IMAGE"
  docker push "$REMOTE_IMAGE"
  echo "Done. Image pushed: $REMOTE_IMAGE"
  echo "Removing local image(s): $REMOTE_IMAGE, $LOCAL_IMAGE"
  docker rmi "$REMOTE_IMAGE" "$LOCAL_IMAGE" 2>/dev/null || true
else
  echo "Done. Image built locally: $LOCAL_IMAGE (no repository specified, skip push)"
fi
