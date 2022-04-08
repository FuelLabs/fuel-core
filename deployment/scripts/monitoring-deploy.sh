#!/bin/bash

set -o allexport && source .env && set +o allexport 

if [ "${k8s_provider}" == "eks" ]; then
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    echo "Deploying kube-prometheus helm chart to ${TF_VAR_eks_cluster_name} ...."
    helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
    helm repo update
    helm upgrade kube-prometheus prometheus-community/kube-prometheus-stack \
              --install \
              --create-namespace \
              --namespace=monitoring \
              --wait \
              --timeout 8000s \
              --debug
              --version ^34
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi
