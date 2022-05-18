#!/bin/bash

set -o errexit # abort on nonzero exitstatus
set -o nounset # abort on unbound variable

echo "This script is to delete existing k8s cluster"

set -o allexport && source .env && set +o allexport 

cd ../../

git clone -b master https://github.com/FuelLabs/infrastructure.git

cd infrastructure/k8s-deployment/terraform/environments/${k8s_provider}

mv state.tf state.template

envsubst < state.template > state.tf

rm state.template 

terraform init

echo "Deleting k8s cluster now"

terraform destroy -auto-approve
