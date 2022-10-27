#!/bin/bash

set -o errexit # abort on nonzero exitstatus
set -o nounset # abort on unbound variable

set -o allexport && source .env && set +o allexport 

if [ "${k8s_provider}" == "eks" ]; then
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}

    kube_prom_helm_chart_exists=$(helm list --namespace monitoring | grep kube-prometheus)
    if [[ $? != 0 ]]; then
        echo "kube-prometheus helm chart is not installed - please install it before redeploying this script ..."
        exit 1;
    elif [[ $kube_prom_helm_chart_exists ]]; then
        echo "kube-prometheus helm chart is detected ..."
    fi
    
    cd ../monitoring
    mv fuel-core-service-monitor.yaml fuel-core-service-monitor.template
    envsubst < fuel-core-service-monitor.template > mv fuel-core-service-monitor.yaml
    rm fuel-core-service-monitor.template
    echo "Deploying fuel-core service monitor to ${TF_VAR_eks_cluster_name} ...."
    kubectl apply -f fuel-core-service-monitor.yaml
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi

