# Graph Node Docker Image

Preconfigured Docker image for running a Graph Node.

## Usage

```sh
docker run -it \
  -e postgres_host=<HOST> \
  -e postgres_port=<PORT> \
  -e postgres_user=<USER> \
  -e postgres_pass=<PASSWORD> \
  -e postgres_db=<DBNAME> \
  -e ipfs=<HOST>:<PORT> \
  -e ethereum=<NETWORK_NAME>:<ETHEREUM_RPC_URL> \
  graphprotocol/graph-node:latest
```

### Example usage

```sh
docker run -it \
  -e postgres_host=host.docker.internal \
  -e postgres_port=5432 \
  -e postgres_user=graph-node \
  -e postgres_pass=oh-hello \
  -e postgres_db=graph-node \
  -e ipfs=host.docker.internal:5001 \
  -e ethereum=mainnet:http://localhost:8545/ \
  graphprotocol/graph-node:latest
```

## Docker Compose

The Docker Compose setup requires an Ethereum network name and node
to connect to. By default, it will use `mainnet:http://host.docker.internal:8545`
in order to connect to an Ethereum node running on your host machine.
You can replace this with anything else in `docker-compose.yaml`.

After you have set up an Ethereum node—e.g. Ganache or Parity—simply
clone this repository and run

```sh
docker-compose up
```

This will start IPFS, Postgres and Graph Node in Docker and create persistent
data directories for IPFS and Postgres in `./data/ipfs` and `./data/postgres`. You
can access these via:

- Graph Node:
  - GraphiQL: `http://localhost:8000/`
  - HTTP: `http://localhost:8000/subgraphs/name/<subgraph-name>`
  - WebSockets: `ws://localhost:8001/subgraphs/name/<subgraph-name>`
  - Admin: `http://localhost:8020/`
- IPFS:
  - `127.0.0.1:5001` or `/ip4/127.0.0.1/tcp/5001`
- Postgres:
  - `postgresql://graph-node:let-me-in@localhost:5432/graph-node`

Once this is up and running, you can use
[`graph-cli`](https://github.com/graphprotocol/graph-tooling/tree/main/packages/cli) to create and
deploy your subgraph to the running Graph Node.
  
### Running Graph Node on an Macbook M1
  
We do not currently build native images for Macbook M1, which can lead to processes being killed due to out-of-memory errors (code 137). Based on the example `docker-compose.yml` is possible to rebuild the image for your M1 by running the following, then running `docker-compose up` as normal:
 
> **Important** Increase memory limits for the docker engine running on your machine. Otherwise docker build command will fail due to out of memory error. To do that, open docker-desktop and go to Resources/advanced/memory.
```
# Remove the original image
docker rmi graphprotocol/graph-node:latest

# Build the image
./docker/build.sh

# Tag the newly created image
docker tag graph-node graphprotocol/graph-node:latest
```
