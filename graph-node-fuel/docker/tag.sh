#! /bin/bash

# This script is used by cloud build to push Docker images into Docker hub

tag_and_push() {
    tag=$1
    docker tag gcr.io/$PROJECT_ID/graph-node:$SHORT_SHA \
           graphprotocol/graph-node:$tag
    docker push graphprotocol/graph-node:$tag

    docker tag gcr.io/$PROJECT_ID/graph-node-debug:$SHORT_SHA \
           graphprotocol/graph-node-debug:$tag
    docker push graphprotocol/graph-node-debug:$tag
}

echo "Logging into Docker Hub"
echo $PASSWORD | docker login --username="$DOCKER_HUB_USER" --password-stdin

set -ex

tag_and_push "$SHORT_SHA"

# Builds of tags set the tag in Docker Hub, too
[ -n "$TAG_NAME" ] && tag_and_push "$TAG_NAME"
# Builds for tags vN.N.N become the 'latest'
[[ "$TAG_NAME" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] && tag_and_push latest

exit 0
