# Fuel Client Deployment on Kubernetes (k8s)

In order to deploy Fuel Client on k8s you must:

1) Create k8s Cluster via Terraform (deployment/terraform)
2) Deploy Fuel Core Helm Chart to your k8s Cluster

## Prerequisites

Before proceeding make sure to have these software packages installed on your machine:

1) [Helm][helm]: Install latest version of Helm3 for your OS

2) Terraform: Install latest version of Terraform for your OS (https://learn.hashicorp.com/tutorials/terraform/install-cli)

3) kubectl: Install latest version of kubectl (https://kubernetes.io/docs/tasks/tools/)

4) AWS:
- AWS CLI v2: https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html
- aws-iam-authenticator: https://docs.aws.amazon.com/eks/latest/userguide/install-aws-iam-authenticator.html


## Creating k8s Cluster

Currently Fuel Core provides terraform based k8s cluster environment deploymnets for:

1) AWS Elastic Kubernetes Service (EKS)

### EKS Cluster Setup

To begin to setup your EKS cluster, you will need to modify the state.tf 
in deployment/terraform/enviroments/aws/

The state.tf stores the state of your deployed EKS cluster and needs to be backed by a S3 bucket created in your account. Update the bucket, key, and region in the state.tf according to your S3 bucket.

You will need modify then the main.tf in deployment/terraform/environments/aws:

- environment: You can input any environment name 

- region: The region where you plan to deploy your EKS Cluster to

- account_id: Your AWS Account ID 

- vpc_cidr_block: Your VPC CIDR Block

- azs: A list of regional availability zones for your VPC's subnets

- public_subnets: A list of CIDR Blocks for your public subnets

- private_subnets: A list of CIDR Blocks for your private subnets

- eks-cluster-name: Your EKS Cluster Name

- eks-cluster-version: The EKS Cluster Version
  Options: 1.18.16 | 1.19.8 | 1.20.7 | 1.21.2

- eks-node-groupname: Your EKS Worker Node Group name

- eks-node-ami-type: The EKS Worker Node Group AMI Type 
Options: AL2_x86_64 | AL2_x86_64_GPU | AL2_ARM_64 | CUSTOM | BOTTLEROCKET_ARM_64 | BOTTLEROCKET_x86_64

- eks-node-disk-size: Disk size in GiB for EKS Worker Nodes

- eks-node-instance-types: A list of instance type for EKS Worker Nodes

- eks-node-min-size: Minimum number of EKS Worker Nodes

- eks-node-desired-size: Desired number of EKS Worker Nodes

- eks-node-max-size: Maximum Number of EKS Worker Nodes

- eks-capacity-type: Type of capacity associated with the EKS Node Group
Options: ON_DEMAND | SPOT

- ec2-ssh-key: EC2 Key Pair name for SSH Access - You must create this key pair in
your acount prior cluster creation

Once your main.tf is updated with your parameters, then run the create-k8s.sh script:

```bash
./create-k8s.sh
```
The script will prompt you for your cloud provider, type in 'aws'

```bash
Please input your cloud provider - options include: aws ....
aws
```
When terraform proposes the infrastructure updates to be deployed, either type "yes" to deploy the changes:

```bash
Do you want to perform these actions?
  Terraform will perform the actions described above.
  Only 'yes' will be accepted to approve.

  Enter a value: yes
```

Terraform then will deploy your VPC network (subnets, route tables, internet & nat gateways) as well as the EKS Cluster and node groups.

## Deploying Fuel Client on k8s

Now that your k8s cluster is setup you can deploy the fuel-core helm chart.

First you need to update the deployment/charts/templates/secrets.yaml with your personal github access token.

Create a github access token following this article (https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token) make sure to select "read:packages" to pull the fuel-core image (https://github.com/fuellabs/fuel-core/pkgs/container/fuel-core).

Then in the secrets.yaml, you need to first Base64 encode "git-username:git-auth-token" in:

```
  .dockerconfigjson: {"auths":{"ghcr.io":{"auth":"git-username:git-auth-token"}}}
```

At your command line:

```
  echo -n "git-username:git-auth-token" | base64
```
Take the output of this insert back into the original place of "git-username:git-auth-token" and now Base64 encode the {"auths": ...} json and at your command line:

```
  echo -n  '{"auths":{"ghcr.io":{"auth":"<base64-string>"}}}' | base64
```

Finally take this string and insert in the secrets.yaml and save:

```
  .dockerconfigjson: <base64-final-output>
```

Now deploy the fuel-core-deploy script. You will be prompted to enter your cloud provider and cluster name. Helm will install the fuel-core helm chart.

To check that the helm chart is succesful run the following commands:

```bash
  % helm list
NAME            NAMESPACE       REVISION        UPDATED                                 STATUS          CHART                APP VERSION
fuel-core       fuel-core       2               2022-01-12 22:29:28.632358 -0500 EST    deployed        fuel-core-1.0.0      1.0   
```

```bash
  % kubectl get all -n fuel-core
NAME                                 READY   STATUS    RESTARTS   AGE
pod/fuel-core-k8s-5f58c6fcbd-h7n5w   1/1     Running   0          131m

NAME                       TYPE           CLUSTER-IP     EXTERNAL-IP                                  PORT(S)        AGE
service/fuel-core-k8s-lb   LoadBalancer   172.20.69.45   xxxxxxxxxxxxxx.us-east-1.elb.amazonaws.com   80:31327/TCP   123m

NAME                            READY   UP-TO-DATE   AVAILABLE   AGE
deployment.apps/fuel-core-k8s   1/1     1            1           131m

NAME                                       DESIRED   CURRENT   READY   AGE
replicaset.apps/fuel-core-k8s-5f58c6fcbd   1         1         1       131m 
```

If the "STATUS" is deployed, the fuel-core helm chart has been deployed successfully. 

Having fuel-core pod(s) running and a service associated with an External IP, load balancer DNS address 
further means the helm chart was deployed successfully.

If its not "deployed', then you will need to delete the helm chart:

```bash
  % helm delete fuel-core --namespace fuel-core
```

Then re-run the fuel-core-deploy script.

[helm]: https://helm.sh/docs/intro/install/



