# Deployment Assets

This directory contains the container build assets for the `fuel-core` node binary
and the `fuel-core-e2e-client` helper binary.

## Files

- `Dockerfile`
  Builds a production-oriented `fuel-core` image.
  The final image is based on distroless and includes the node binary together
  with the chain configuration files fetched during the build stage.

- `e2e-client.Dockerfile`
  Builds a container image for the `fuel-core-e2e-client` binary used in
  end-to-end and integration-style workflows.

- `../docker-compose.yml`
  Provides a minimal local setup for running a `fuel-core` node with a
  persistent database volume.

## Build The Node Image

From the repository root:

```sh
docker build -t fuel-core -f deployment/Dockerfile .
```

The resulting image starts `fuel-core run` by default.

## Run With Docker Compose

From the repository root:

```sh
docker compose up --build fuel-core
```

This uses [`docker-compose.yml`](../docker-compose.yml) to:

- expose GraphQL on port `4000`
- mount a persistent Docker volume at `/mnt/db`
- build the image from [`deployment/Dockerfile`](Dockerfile)

## Build The E2E Client Image

From the repository root:

```sh
docker build -t fuel-core-e2e-client -f deployment/e2e-client.Dockerfile .
```

## Notes

- These assets are intended for container-based local development and packaging.
- Kubernetes manifests are not currently stored in this directory.
- For operator-focused node setup guidance, see the Fuel node operator docs:
  <https://docs.fuel.network/docs/node-operator/>
