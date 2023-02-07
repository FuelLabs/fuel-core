#!/bin/bash

set -o errexit # abort on nonzero exitstatus
set -o nounset # abort on unbound variable

set -o allexport && source .env && set +o allexport 

if [ "${k8s_provider}" == "eks" ]; then
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    echo "Deleting fuel-core helm chart on ${TF_VAR_eks_cluster_name} ...."
    helm delete ${fuel_core_service_name} \
              --namespace ${k8s_namespace} \
              --wait \
              --timeout 8000s \
              --debug
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi
