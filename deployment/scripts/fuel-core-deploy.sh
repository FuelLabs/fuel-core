#!/bin/bash

echo "Please input your cloud provider - options include: aws ...."

read cloud

if [ "${cloud}" == "aws" ]; then
    echo "Please configure your aws cli locally ...."
    echo "Please enter your EKS cluster name:"
    read eks
    echo "Updating your kube context locally ...."
    aws eks update-kubeconfig --name $eks
    cd ../charts
    echo "Deploying fuel-core helm chart to $eks ...."
    helm upgrade fuel-core . \
              --values values.yaml \
              --install \
              --create-namespace \
              --namespace=fuel-core \
              --wait \
              --timeout 1200s \
              --debug
else
   echo "You have chosen a non-supported cloud provider - please re-run this script with one of these options: aws"
fi
