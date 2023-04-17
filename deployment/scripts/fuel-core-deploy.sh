#!/bin/bash

set -o errexit # abort on nonzero exitstatus
set -o nounset # abort on unbound variable

set -o allexport && source .env && set +o allexport 

if [ "${k8s_provider}" == "eks" ]; then
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name ${TF_VAR_eks_cluster_name}
    
    echo "Copying chainspec into deployment context ...."
    cp chainspec/${chain_spec_file} ../charts/chainspec.json
    export fuel_core_consensus_key_secret_base64_encoded=$(echo -n $fuel_core_consensus_key_secret | base64 -w 0) 
    
    cd ../secrets/
    echo "Creating the fuel-core k8s secret ...."
    mv fuel-core-secret.yaml fuel-core-secret.template
    envsubst < fuel-core-secret.template > fuel-core-secret.yaml
    rm fuel-core-secret.template
    kubectl create ns ${k8s_namespace} || true
    kubectl delete -f fuel-core-secret.yaml || true
    kubectl apply -f fuel-core-secret.yaml

    if [ "${fuel_core_pvc_clone_enabled}" == "true" ]; then
        cd ../volume_snapshots/
        kubectl apply -f VolumeSnapshotClass.yaml || true 
        mv VolumeSnapshotContent.yaml VolumeSnapshotContent.template
        envsubst < VolumeSnapshotContent.template > VolumeSnapshotContent.yaml
        rm VolumeSnapshotContent.template
        kubectl apply -f VolumeSnapshotContent.yaml
        mv VolumeSnapshot.yaml VolumeSnapshot.template
        envsubst < VolumeSnapshot.template > VolumeSnapshot.yaml
        rm VolumeSnapshot.template
        kubectl apply -f VolumeSnapshot.yaml
    else 
       echo "Volume Snapshots will not be created ...."
    fi 

    cd ../charts
    mv Chart.yaml Chart.template
    envsubst < Chart.template > Chart.yaml
    rm Chart.template
    mv values.yaml values.template
    envsubst < values.template > values.yaml
    rm values.template
    
    echo "Deploying ${fuel_core_service_name} helm chart to ${TF_VAR_eks_cluster_name} ...."
    helm upgrade ${fuel_core_service_name} . \
              --values values.yaml \
              --install \
              --create-namespace \
              --namespace=${k8s_namespace} \
              --wait \
              --timeout 8000s \
              --debug
else
   echo "You have inputted a non-supported kubernetes provider in your .env"
fi
