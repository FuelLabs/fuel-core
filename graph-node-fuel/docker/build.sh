#! /bin/bash

# This file is only here to ease testing/development. Official images are
# built using the 'cloudbuild.yaml' file

type -p podman > /dev/null && docker=podman || docker=docker

cd $(dirname $0)/..

if [ -d .git ]
then
    COMMIT_SHA=$(git rev-parse HEAD)
    TAG_NAME=$(git tag --points-at HEAD)
    REPO_NAME="Checkout of $(git remote get-url origin) at $(git describe --dirty)"
    BRANCH_NAME=$(git rev-parse --abbrev-ref HEAD)
fi
for stage in graph-node-build graph-node graph-node-debug
do
    $docker build --target $stage \
            --build-arg "COMMIT_SHA=$COMMIT_SHA" \
            --build-arg "REPO_NAME=$REPO_NAME" \
            --build-arg "BRANCH_NAME=$BRANCH_NAME" \
            --build-arg "TAG_NAME=$TAG_NAME" \
            -t $stage \
            -f docker/Dockerfile .
done
