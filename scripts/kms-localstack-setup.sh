#!/usr/bin/env bash
# Starts (or reuses) a LocalStack KMS instance, creates a signing key matching
# fuel-core's secp256k1 PoA scheme, and exports the vars the `aws-kms` test
# feature needs. FUEL_CORE_TEST_AWS_KMS_ARN is read via `option_env!` in
# signer.rs / aws_kms.rs, so it must be set *before* `cargo test` compiles,
# not just before it runs.
#
# CI: run directly as a step (`./scripts/kms-localstack-setup.sh`) after the
# `localstack` service container is up; vars are written to $GITHUB_ENV so
# later steps (the `cargo test` step) pick them up.
#
# Local dev: `eval "$(./scripts/kms-localstack-setup.sh)"`, then run:
#   cargo test -p fuel-core-tests --features aws-kms -- kms
set -euo pipefail

LOCALSTACK_ENDPOINT="${LOCALSTACK_ENDPOINT:-http://127.0.0.1:4566}"
LOCALSTACK_IMAGE="${LOCALSTACK_IMAGE:-localstack/localstack:4.14}" # last version that does not require a license
CONTAINER_NAME="${CONTAINER_NAME:-fuel-core-kms-localstack}"
AWS_REGION="${AWS_REGION:-us-east-1}"
AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-test}"
AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-test}"

is_kms_healthy() {
  local response state
  response=$(curl -fsS "$LOCALSTACK_ENDPOINT/_localstack/health" 2>/dev/null || true)
  state=$(printf '%s' "$response" | jq -r '.services.kms // empty' 2>/dev/null || true)
  [ "$state" = "running" ] || [ "$state" = "available" ]
}

if ! is_kms_healthy; then
  echo "Starting LocalStack ($LOCALSTACK_IMAGE) for KMS..." >&2
  docker run -d --rm \
    --name "$CONTAINER_NAME" \
    -p 4566:4566 \
    -e SERVICES=kms \
    -e DEBUG=1 \
    "$LOCALSTACK_IMAGE" >/dev/null

  healthy=false
  for _ in $(seq 1 30); do
    if is_kms_healthy; then
      healthy=true
      break
    fi
    sleep 2
  done
  if [ "$healthy" != true ]; then
    echo "LocalStack failed to become healthy" >&2
    exit 1
  fi
fi

echo "Creating KMS signing key (ECC_SECG_P256K1)..." >&2
key_arn=$(AWS_ACCESS_KEY_ID="$AWS_ACCESS_KEY_ID" AWS_SECRET_ACCESS_KEY="$AWS_SECRET_ACCESS_KEY" AWS_REGION="$AWS_REGION" \
  aws --endpoint-url "$LOCALSTACK_ENDPOINT" kms create-key \
    --customer-master-key-spec ECC_SECG_P256K1 \
    --key-usage SIGN_VERIFY \
    --region "$AWS_REGION" \
    --output json | jq -r '.KeyMetadata.Arn')

if [ -z "$key_arn" ] || [ "$key_arn" = "null" ]; then
  echo "Failed to create KMS key" >&2
  exit 1
fi
echo "Created key: $key_arn" >&2

if [ -n "${GITHUB_ENV:-}" ]; then
  {
    echo "AWS_ENDPOINT_URL=$LOCALSTACK_ENDPOINT"
    echo "AWS_ACCESS_KEY_ID=$AWS_ACCESS_KEY_ID"
    echo "AWS_SECRET_ACCESS_KEY=$AWS_SECRET_ACCESS_KEY"
    echo "AWS_REGION=$AWS_REGION"
    echo "FUEL_CORE_TEST_AWS_KMS_ARN=$key_arn"
  } >> "$GITHUB_ENV"
else
  echo "export AWS_ENDPOINT_URL=$LOCALSTACK_ENDPOINT"
  echo "export AWS_ACCESS_KEY_ID=$AWS_ACCESS_KEY_ID"
  echo "export AWS_SECRET_ACCESS_KEY=$AWS_SECRET_ACCESS_KEY"
  echo "export AWS_REGION=$AWS_REGION"
  echo "export FUEL_CORE_TEST_AWS_KMS_ARN=$key_arn"
fi
