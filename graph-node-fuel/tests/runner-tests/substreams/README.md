# Substreams-powered subgraph: tracking contract creation

A basic Substreams-powered subgraph, including the Substreams definition. This example detects new
contract deployments on Ethereum, tracking the creation block and timestamp. There is a
demonstration of the Graph Node integration, using `substreams_entity_change` types and helpers.

## Prerequisites

This
[requires the dependencies necessary for local Substreams development](https://substreams.streamingfast.io/developers-guide/installation-requirements).

## Quickstart

```
yarn install # install graph-cli
yarn substreams:prepare # build and package the substreams module
yarn subgraph:build # build the subgraph
yarn subgraph:deploy # deploy the subgraph
```
