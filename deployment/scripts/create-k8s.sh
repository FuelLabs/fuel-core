#!/bin/bash

echo "This script is to create a new or update existing k8s cluster ...."

set -o allexport && source .env && set +o allexport 

cd ../terraform/environments/${k8s_provider}

mv state.tf state.template

envsubst < state.template > state.tf

rm state.template 

terraform init

echo "Creating or updating k8s cluster now .... please don't interrupt your terminal ...."

terraform apply -auto-approve

echo "Please wait while your k8s cluster gets ready ...."
sleep 600

echo "Now setting up your k8s cluster ..."

if [ "${k8s_provider}" == "eks" ]; then
    echo "Updating your kube context locally ....."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    cd ../ingress/${k8s_provider}
    echo "Deploying cert-manager helm chart to ${TF_VAR_eks_cluster_name} ...."
    helm repo add jetstack https://charts.jetstack.io
    helm repo update
    helm upgrade cert-manager jetstack/cert-manager --namespace cert-manager --version v1.7.1 --install --create-namespace
    echo "Deploying nginx ingress controller to ${TF_VAR_eks_cluster_name} ...."
    kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/aws/1.21/deploy.yaml
    echo "Deploying production cluster issuer to ${TF_VAR_eks_cluster_name} ...."
    mv prod-issuer.yaml prod-issuer.template
    envsubst < prod-issuer.template > prod-issuer.yaml
    rm prod-issuer.template
    kubectl apply -f prod-issuer.yaml 
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
    echo "Deploying monitoring ingress to ${TF_VAR_eks_cluster_name} ...."
    mv monitoring-ingress.yaml monitoring-ingress.template
    envsubst < monitoring-ingress.template > monitoring-ingress.yaml
    rm monitoring-ingress.template
    kubectl apply -f monitoring-ingress.yaml
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi
