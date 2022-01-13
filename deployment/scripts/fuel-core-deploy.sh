#!/bin/bash

echo "Please input the correct cloud provider- options include: aws"

read cloud

if [ "${cloud}" == "aws" ]; then
    echo "Please make sure to configure your aws cli locally"
    echo "Please enter your EKS Cluster Name"
    read eks
    echo "Updating your kube context locally"
    aws eks update-kubeconfig --name $eks
    cd ../charts
    echo "Deploying fuel-core helm chart to $eks"
    helm upgrade fuel-core . \
              --values values.yaml \
              --install \
              --create-namespace \
              --namespace=fuel-core \
              --wait \
              --timeout 8000s \
              --debug
else
   echo "You have chosen non-supported cloud provider .."
fi





