#!/bin/bash

set -o allexport && source .env && set +o allexport 

if [ "${k8s_provider}" == "eks" ]; then
    echo " ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    cd ../ingress/${k8s_provider}
    kubectl delete -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/aws/1.21/deploy.yaml
    helm delete cert-manager --namespace cert-manager
    kubectl delete -f prod-issuer.yaml
    kubectl delete -f ingress.yaml
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi
