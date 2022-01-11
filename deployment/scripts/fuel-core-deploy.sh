#!/bin/bash

echo "Please input the correct cloud provider- options include: aws"

read cloud

if [ "${cloud}" == "aws" ]; then
    echo "Please make sure to configure your aws cli locally"
    echo "Please enter your EKS Cluster Name"
    read eks-cluster-name
    echo "Updating your kube context locally"
    aws eks update-kubeconfig --name ${eks-cluster-name}
    cd ../charts
    echo "Please enter your fuel-core image tag to be deployed"
    read tag
    echo "Deploying fuel-core helm chart to ${eks-cluster-name}"
    helm upgrade fuel-core- . \
              --values values.yaml \
              --install \
              --namespace=fuel-core \
              --set image.tag=${tag} \
              --wait \
              --timeout 8000s \
              --debug
else
   echo "You have chosen non-supported cloud provider .."
fi





