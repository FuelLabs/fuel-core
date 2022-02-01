# Fuel Client Deployment with Docker

## Prerequisites

Before proceeding make sure to have these software packages installed on your machine:

1) [Docker Desktop][docker-desktop]: Install latest version of Docker Desktop on your OS

## Deploying Fuel Client with Docker

Navigate to the [deployment][deploy-dir] directory and run at your command line:

```bash
docker-compose up -d
```

This will start the fuel-core container in the background and keep it running.

In order to stop and remove the fuel-core container, run at your command line:

```bash
docker-compose down
```

Note: Linx, Unix, and Windows operating systems are supported for docker-compose deployment option.

# Fuel Client Deployment on Kubernetes (k8s)

In order to deploy Fuel Client on k8s you must:

1) Create [k8s cluster via terraform][k8s-terraform]
2) Deploy [fuel-core helm chart][fuel-helm-chart] to your k8s cluster

## Prerequisites

Before proceeding make sure to have these software packages installed on your machine:

1) [Helm][helm]: Install latest version of Helm3 for your OS

2) [Terraform][terraform]: Install latest version of Terraform for your OS

3) [kubectl][kubectl-cli]: Install latest version of kubectl

4) AWS:
- [aws cli v2][aws-cli]: Install latest version of aws cli v2
- [aws-iam-authenticator][iam-auth]: Install to authenticate to EKS cluster via AWS IAM

5) [gettext][gettext-cli]: Install gettext for your OS

Note: Currently only Linux and Unix operating systems are supported for k8s cluster creation.

## Deploying k8s Cluster

Currently Fuel Core support terraform based k8s cluster environment deployments for:

1) AWS Elastic Kubernetes Service ([EKS][aws-eks])

### k8s Cluster Configuration

The current k8s cluster configuration is based on a single [env][env-file] file.

You will need to customize the following environment variables as needed (for variables not needed - keep the defaults):

| ENV Variable                   |  Script Usage     | Description                                                                                       |
| ------------------------------ |:-----------------:|:-------------------------------------------------------------------------------------------------:|
| k8s_provider                   |  create-k8s (all) | your kubernetes provider name, possible options: eks                                              | 
| fuel_core_image_repository     |  fuel-core-deploy | fuel-core ghcr image registry URI                                                                 |   
| fuel_core_image_tag            |  fuel-core-deploy | fuel-core ghcr image tag                                                                          | 
| base64_github_auth_token       |  fuel-core-deploy | base64 encoded github auth token to pull the fuel-core ghcr image (more info below)               |
| TF_VAR_environment             |  create-k8s (all) | environment name                                                                                  |
| TF_VAR_region                  |  create-k8s (aws) | AWS region where you plan to deploy your EKS cluster e.g. us-east-1                               |
| TF_VAR_account_id              |  create-k8s (aws) | AWS account id                                                                                    |
| TF_state_bucket                |  create-k8s (aws) | the s3 bucket to store the deployed terraform state                                               |
| TF_state_bucket_key            |  create-k8s (aws) | the s3 key to save the deployed terraform state.tf                                                |
| TF_VAR_vpc_cidr_block          |  create-k8s (aws) | AWS vpc cidr block                                                                                |
| TF_VAR_azs                     |  create-k8s (aws) | A list of regional availability zones for the AWS vpc subnets                                     |
| TF_VAR_public_subnets          |  create-k8s (aws) | A list of cidr blocks for AWS public subnets                                                      |
| TF_VAR_private_subnets         |  create-k8s (aws) | A list of cidr blocks for AWS private subnets                                                     | 
| TF_VAR_eks_cluster_name        |  create-k8s (aws) | EKS cluster name                                                                                  |
| TF_VAR_eks_cluster_version     |  create-k8s (aws) | EKS cluster version, possible options: 1.18.16, 1.19.8, 1.20.7, 1.21.2                            |
| TF_VAR_eks_node_groupname      |  create-k8s (aws) | EKS worker node group name                                                                        |
| TF_VAR_eks_node_ami_type       |  create-k8s (aws) | EKS worker node group AMI type, possible options: AL2_x86_64, AL2_x86_64_GPU, AL2_ARM_64, CUSTOM  | 
| TF_VAR_eks_node_disk_size      |  create-k8s (aws) | disk size (GiB) for EKS worker nodes                                                              |
| TF_VAR_eks_node_instance_types |  create-k8s (aws) | A list of instance type for EKS worker nodes                                                      |
| TF_VAR_eks_node_min_size       |  create-k8s (aws) | minimum number of eks worker nodes                                                                |
| TF_VAR_eks_node_desired_size   |  create-k8s (aws) | desired number of eks worker nodes                                                                |
| TF_VAR_eks_node_max_size       |  create-k8s (aws) | maximum number of eks worker nodes                                                                |
| TF_VAR_eks_capacity_type       |  create-k8s (aws) | type of capacity associated with the eks node group, possible options: ON_DEMAND, SPOT            |
| TF_VAR_ec2_ssh_key             |  create-k8s (aws) | EC2 key Pair name for ssh access (must create this key pair in your AWS account before)           |


Notes:

- create-k8s refers to the [create-k8s.sh][create-k8s-sh] script

- fuel-core-deploy refers to [fuel-core-deploy][fuel-deploy-script] script

- for 'base64_github_auth_token' environment variable:

First create a [github access token][create-git-token] and make sure to select "read:packages" to pull the [fuel-core image][fuel-core-image].

You need to first Base64 encode "git-username:git-auth-token" string: 

```
  {"auths":{"ghcr.io":{"auth":"git-username:git-auth-token"}}}
```

At your command line, run (make sure to substitute your github username and personal access token here):

```
  echo -n "git-username:git-auth-token" | base64
```
Take the string output and insert back into the original place of "git-username:git-auth-token" in the auths json string.

Now base64 encode the {"auths": ...} json string, by running at your command line:

```
  echo -n  '{"auths":{"ghcr.io":{"auth":"<your-first-base64-output>"}}}' | base64
```

This final encoded value is your 'base64_github_auth_token' environment variable


### k8s Cluster Deployment

Once your env file is updated with your parameters, then run the [create-k8s.sh][create-k8s-sh] to create your k8s cluster on your cloud provider:

```bash
./create-k8s.sh
```
The script will read your "k8s_provider" from the env file and then terraform will start deploy your k8s cluster automatically to your specified cloud provider.

## Deploying Fuel Client on k8s

Now that your k8s cluster is setup - you can deploy the fuel-core helm chart via the [fuel-core-deploy][fuel-deploy-script]. 

```bash
  ./fuel-core-deploy.sh
```

To check that the helm chart is successful, run the following at your command line:

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
[docker-desktop]: https://docs.docker.com/engine/install/
[terraform]: https://learn.hashicorp.com/tutorials/terraform/install-cli
[kubectl-cli]: https://kubernetes.io/docs/tasks/tools/
[aws-cli]: https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html
[iam-auth]: https://docs.aws.amazon.com/eks/latest/userguide/install-aws-iam-authenticator.html
[gettext-cli]: https://www.gnu.org/software/gettext/
[tf-state]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/terraform/environments/aws/state.tf
[k8s-terraform]: https://github.com/FuelLabs/fuel-core/tree/roy-fuel-eks-helm-charts/deployment/terraform
[env-file]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/scripts/.env
[deploy-dir]: https://github.com/FuelLabs/fuel-core/tree/roy-fuel-eks-helm-charts/deployment
[fuel-helm-chart]: https://github.com/FuelLabs/fuel-core/tree/roy-fuel-eks-helm-charts/deployment/charts
[aws-eks]: https://aws.amazon.com/eks/
[main-tf]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/terraform/environments/aws/main.tf
[create-k8s-sh]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/scripts/create-k8s.sh
[create-git-token]:  https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token
[secrets-yaml]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/charts/templates/secrets.yaml
[fuel-core-image]: https://github.com/fuellabs/fuel-core/pkgs/container/fuel-core
[fuel-deploy-script]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/scripts/fuel-core-deploy.sh