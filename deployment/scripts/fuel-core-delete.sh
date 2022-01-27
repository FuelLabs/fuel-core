#!/bin/bash

source .env

if [ "${k8s_provider}" == "eks" ]; then
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    echo "Deleting fuel-core helm chart on ${TF_VAR_eks_cluster_name} ...."
    helm delete fuel-core --namespace fuel-core
else
   echo "You have chosen a non-supported kubernetes provider"
fi