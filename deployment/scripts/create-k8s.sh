#!/bin/bash

echo "This script is to create a new or update existing k8s cluster"

echo "Please input your cloud provider - options include: aws ...."

read cloud

cd ../terraform/environments/$cloud

terraform init

terraform apply 



