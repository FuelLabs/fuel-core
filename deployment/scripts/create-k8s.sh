#!/bin/bash

echo "This script is to create or update your k8s cluster"

echo "Please input the correct cloud provider- options include: aws"

read cloud

cd ../terraform/environments/$cloud

terraform init

terraform apply 



