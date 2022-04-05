#!/bin/bash

set -o allexport && source .env && set +o allexport 

if [ "${k8s_provider}" == "eks" ]; then
    echo " ...."
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    cd ../ingress/${k8s_provider}
    mv monitoring-ingress.yaml monitoring-ingress.template
    envsubst < monitoring-ingress.template > monitoring-ingress.yaml
    rm monitoring-ingress.template
    echo "Deploying monitoring ingress to ${TF_VAR_eks_cluster_name} ...."
    kubectl apply -f monitoring-ingress.yaml
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi
