#!/bin/bash

set -o errexit # abort on nonzero exitstatus
set -o nounset # abort on unbound variable

set -o allexport && source .env && set +o allexport 

if [ "${k8s_provider}" == "eks" ]; then
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    cd ../ingress/${k8s_provider}
    echo "Deleting fuel-core ingress on ${TF_VAR_eks_cluster_name} ...."
    mv fuel-core-ingress.yaml fuel-core-ingress.template
    envsubst < fuel-core-ingress.template > fuel-core-ingress.yaml
    rm fuel-core-ingress.template
    kubectl delete -f fuel-core-ingress.yaml
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi
