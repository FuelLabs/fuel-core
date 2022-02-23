# Fuel Client Deployment with Docker

## Prerequisites

Before proceeding make sure to have these software packages installed on your machine:

1) [Docker Desktop][docker-desktop]: Install latest version of Docker Desktop on your OS

## Deploying Fuel Client with Docker

Navigate to the root of the fuel-core git repo and run at your command line:

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

4) [gettext][gettext-cli]: Install gettext for your OS

4) AWS (for EKS only):
- [aws cli v2][aws-cli]: Install latest version of aws cli v2

- [aws-iam-authenticator][iam-auth]: Install to authenticate to EKS cluster via AWS IAM

- IAM user with AWS access keys with following IAM access:
```json
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Sid": "VisualEditor0",
            "Effect": "Allow",
            "Action": [
                "iam:CreateInstanceProfile",
                "iam:GetPolicyVersion",
                "iam:PutRolePermissionsBoundary",
                "iam:DeletePolicy",
                "iam:CreateRole",
                "iam:AttachRolePolicy",
                "iam:PutRolePolicy",
                "iam:DeleteRolePermissionsBoundary",
                "iam:CreateLoginProfile",
                "iam:ListInstanceProfilesForRole",
                "iam:PassRole",
                "iam:DetachRolePolicy",
                "iam:DeleteRolePolicy",
                "iam:ListAttachedRolePolicies",
                "iam:ListRolePolicies",
                "iam:CreatePolicyVersion",
                "iam:DeleteInstanceProfile",
                "iam:GetRole",
                "iam:GetInstanceProfile",
                "iam:GetPolicy",
                "iam:ListRoles",
                "iam:DeleteRole",
                "iam:CreatePolicy",
                "iam:ListPolicyVersions",
                "iam:UpdateRole",
                "iam:DeleteServiceLinkedRole",
                "iam:GetRolePolicy",
                "iam:DeletePolicyVersion",
                "logs:*",
                "s3:*",
                "autoscaling:*",
                "cloudwatch:*",
                "elasticloadbalancing:*",
                "ec2:*",
                "eks:*"
            ],
            "Resource": "*"
        }
    ]
}
```

Note: Currently only Linux and Unix operating systems are supported for terraform creation of a k8s cluster.

## Deploying k8s Cluster

Currently Fuel Core support terraform based k8s cluster environment deployments for:

1) AWS Elastic Kubernetes Service ([EKS][aws-eks])

### k8s Cluster Configuration

The current k8s cluster configuration is based on a single [env][env-file] file.

You will need to customize the following environment variables as needed (for variables not needed - keep the defaults):

| ENV Variable                   |  Script Usage     | Description                                                                                       |
|--------------------------------|-------------------|---------------------------------------------------------------------------------------------------|
| k8s_provider                   |  create-k8s (all) | your kubernetes provider name, possible options: eks                                              |
| k8s_namespace                  |  fuel-core-deploy | your kubernetes namespace for fuel-core deployment  
| fuel_core_image_repository     |  fuel-core-deploy | fuel-core ghcr image registry URI                                                                 |   
| fuel_core_image_tag            |  fuel-core-deploy | fuel-core ghcr image tag                                                                          | 
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
| TF_VAR_eks_node_instance_types |  create-k8s (aws) | A list of instance types for the EKS worker nodes                                                 |
| TF_VAR_eks_node_min_size       |  create-k8s (aws) | minimum number of eks worker nodes                                                                |
| TF_VAR_eks_node_desired_size   |  create-k8s (aws) | desired number of eks worker nodes                                                                |
| TF_VAR_eks_node_max_size       |  create-k8s (aws) | maximum number of eks worker nodes                                                                |
| TF_VAR_eks_capacity_type       |  create-k8s (aws) | type of capacity associated with the eks node group, possible options: ON_DEMAND, SPOT            |
| TF_VAR_ec2_ssh_key             |  create-k8s (aws) | ec2 key Pair name for ssh access (must create this key pair in your AWS account before)           |

Notes:

- create-k8s refers to the [create-k8s.sh][create-k8s-sh] script

- fuel-core-deploy refers to [fuel-core-deploy][fuel-deploy-script] script

### k8s Cluster Deployment

Once your env file is updated with your parameters, then run the [create-k8s.sh][create-k8s-sh] to deploy the k8s cluster to your cloud provider:

```bash
./create-k8s.sh
```
The script will read the "k8s_provider" from the env file and then terraform will automatically create the k8s cluster.

### k8s Cluster Delete

If you need to tear down your k8s cluster, just run the [delete-k8s.sh][delete-k8s-sh] script:

```bash
./delete-k8s.sh
```

## Deploying Fuel Client on k8s

Now that the k8s cluster is setup - you can deploy the fuel-core helm chart via the [fuel-core-deploy][fuel-deploy-script]. 

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

If the helm chart deployments fails and/or the fuel-core pod(s) are not healthy, then you will need to delete the helm chart via [fuel-core-delete][fuel-delete-script] script:

```bash
  ./fuel-core-delete.sh
```

Then re-run the fuel-core-deploy script.

[aws-cli]: https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html
[aws-eks]: https://aws.amazon.com/eks/
[create-git-token]:  https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token
[create-k8s-sh]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/scripts/create-k8s.sh
[delete-k8s-sh]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/scripts/delete-k8s.sh
[docker-desktop]: https://docs.docker.com/engine/install/
[env-file]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/scripts/.env
[fuel-core-image]: https://github.com/fuellabs/fuel-core/pkgs/container/fuel-core
[fuel-delete-script]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/scripts/fuel-core-delete.sh
[fuel-deploy-script]: https://github.com/FuelLabs/fuel-core/blob/roy-fuel-eks-helm-charts/deployment/scripts/fuel-core-deploy.sh
[fuel-helm-chart]: https://github.com/FuelLabs/fuel-core/tree/roy-fuel-eks-helm-charts/deployment/charts
[gettext-cli]: https://www.gnu.org/software/gettext/
[helm]: https://helm.sh/docs/intro/install/
[iam-auth]: https://docs.aws.amazon.com/eks/latest/userguide/install-aws-iam-authenticator.html
[k8s-terraform]: https://github.com/FuelLabs/fuel-core/tree/roy-fuel-eks-helm-charts/deployment/terraform
[kubectl-cli]: https://kubernetes.io/docs/tasks/tools/
[terraform]: https://learn.hashicorp.com/tutorials/terraform/install-cli