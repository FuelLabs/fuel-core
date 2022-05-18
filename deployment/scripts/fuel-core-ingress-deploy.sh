#!/bin/bash

set -o errexit # abort on nonzero exitstatus
set -o nounset # abort on unbound variable

set -o allexport && source .env && set +o allexport 

if [ "${k8s_provider}" == "eks" ]; then
    echo " ...."
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    cd ../ingress/${k8s_provider}
    echo "Deploying production cluster issuer to ${TF_VAR_eks_cluster_name} ...."
    mv prod-issuer.yaml prod-issuer.template
    envsubst < prod-issuer.template > prod-issuer.yaml
    rm prod-issuer.template
    kubectl apply -f prod-issuer.yaml
    echo "Deploying fuel-core ingress to ${TF_VAR_eks_cluster_name} ...."
    mv fuel-core-ingress.yaml fuel-core-ingress.template
    envsubst < fuel-core-ingress.template > fuel-core-ingress.yaml
    rm fuel-core-ingress.template
    kubectl apply -f fuel-core-ingress.yaml
    echo "Deploying monitoring ingress to ${TF_VAR_eks_cluster_name} ...."
    mv monitoring-ingress.yaml monitoring-ingress.template
    envsubst < monitoring-ingress.template > monitoring-ingress.yaml
    rm monitoring-ingress.template
    kubectl apply -f monitoring-ingress.yaml
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi
