#!/bin/bash

echo "This script is to create a new or update existing k8s cluster"

set -o allexport && source .env && set +o allexport 

cd ../terraform/environments/${k8s_provider}

mv state.tf state.template

envsubst < state.template > state.tf

rm state.template 

terraform init

terraform apply -auto-approve



