#!/bin/bash

set -o allexport && source .env && set +o allexport 

if [ "${k8s_provider}" == "eks" ]; then
    echo " ...."
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    cd ../ingress/${k8s_provider}
    echo "Deploying cert-manager helm chart to ${TF_VAR_eks_cluster_name} ...."
    kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/aws/1.21/deploy.yaml
    helm repo add jetstack https://charts.jetstack.io
    helm repo update
    helm upgrade cert-manager jetstack/cert-manager --namespace cert-manager --version v1.7.1 --install --create-namespace 
    mv prod-issuer.yaml prod-issuer.template
    envsubst < prod-issuer.template > prod-issuer.yaml
    rm prod-issuer.template
    kubectl apply -f prod-issuer.yaml
    mv fuel-core-ingress.yaml fuel-core-ingress.template
    envsubst < fuel-core-ingress.template > fuel-core-ingress.yaml
    rm fuel-core-ingress.template
    echo "Deploying fuel-core ingress to ${TF_VAR_eks_cluster_name} ...."
    kubectl apply -f fuel-core-ingress.yaml
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi
