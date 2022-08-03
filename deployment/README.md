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

Note: Linux, Unix, and Windows operating systems are supported for docker-compose deployment option.

## Fuel Client Deployment on Kubernetes (k8s)

In order to deploy Fuel Client on k8s you must:

1) Deploy [fuel-core helm chart][fuel-helm-chart] to your k8s cluster

### Prerequisites

Before proceeding make sure to have these software packages installed on your machine:

1) [Helm][helm]: Install latest version of Helm3 for your OS

2) [Terraform][terraform]: Install latest version of Terraform for your OS

3) [kubectl][kubectl-cli]: Install latest version of kubectl

4) [gettext][gettext-cli]: Install gettext for your OS

5) AWS (for EKS deployment only):

- [aws cli v2][aws-cli]: Install latest version of aws cli v2

- [aws-iam-authenticator][iam-auth]: Install to authenticate to EKS cluster via AWS IAM

- IAM user(s) with AWS access keys with following IAM access:

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

### Deploying to k8s Cluster

#### k8s Cluster Configuration

The current k8s cluster configuration is based on a single [env][env-file] file.

You will need to customize the following environment variables as needed (for variables not needed - keep the defaults):

| ENV Variable                | Script Usage             | Description                                          |
|-----------------------------|--------------------------|------------------------------------------------------|
| k8s_provider                | all                      | your kubernetes provider name, possible options: eks |
| k8s_namespace               | fuel-core-deploy         | your kubernetes namespace for fuel-core deployment   |
| fuel_core_ingress_dns       | fuel-core-ingress-deploy | the custom dns address for the fuel-core ingress     |
| fuel_core_ingress_http_port | fuel-core-ingress-deploy | the custom port for the fuel-core ingress            |
| fuel_core_image_repository  | fuel-core-deploy         | fuel-core ghcr image registry URI                    |
| fuel_core_image_tag         | fuel-core-deploy         | fuel-core ghcr image tag                             |
| fuel_core_pod_replicas      | fuel-core-deploy         | number of fuel-core pod replicas                     |
| pvc_storage_class           | fuel-core-deploy         | Storage class for the persistent volume              |
| pvc_storage_requests        | fuel-core-deploy         | Th size of the request for the persistent volume     |

Notes:

- fuel-core-deploy refers to [fuel-core-deploy][fuel-deploy-script] script

### Deploying Fuel Client on k8s

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

### Deploying Fuel Ingress on k8s

In order to serve external HTTP and/or HTTPS traffic, provide load balancing, and SSL termination to your newly deployed fuel-core pod, an [ingress][ingress-def] k8s object must be deployed.

Before deploying your fuel ingress, you must select a 'fuel_core_ingress_dns' env that will serve as the DNS address for external access of your fuel-core application. The DNS address must be in a DNS domain that you currently own and have access to.

Additionally 'fuel_core_ingress_http_port' env parameter must be selected for the http port, the default is port 80.

In order to support SSL certificate creation for your custom ingress DNS, you must select an 'letsencrypt_email' env which is an email address you have access to renew your letsencrypt certificate when needed. [Certificate manager][cert-manager] is used to issue the custom certificate via letsencrypt

For fuel ingress is deployed via [fuel-core-ingress-deploy][fuel-core-ingress-deploy] script:

```bash
  ./fuel-core-ingress-deploy.sh
```

It will take several minutes for ingress to be deployed and an external address to show up:

```bash
% kubectl get ingress -n fuel-core
NAME                CLASS    HOSTS              ADDRESS                                                    PORTS     AGE
fuel-core-ingress   <none>   node.example.io   xxxxxxxxxxxxxxxxxxxxxxxxxxxxx.elb.us-east-1.amazonaws.com   80, 443   3d21h
```

Create a DNS record based on that ADDRESS value in your DNS registrar and your fuel-core application will now be served at your DNS address.

If you need to cleanup your existing ingress resource, run the [fuel-core-ingress-delete][fuel-core-ingress-delete]:

```bash
  ./fuel-core-ingress-delete.sh
```

[add-users-aws-auth]: https://docs.aws.amazon.com/eks/latest/userguide/add-user-role.html
[aws-cli]: https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html
[aws-eks]: https://aws.amazon.com/eks/
[cert-manager]: https://cert-manager.io/docs/configuration/acme/
[create-git-token]:  https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token
[docker-desktop]: https://docs.docker.com/engine/install/
[env-file]: https://github.com/FuelLabs/fuel-core/blob/master/deployment/scripts/.env
[fuel-core-image]: https://github.com/fuellabs/fuel-core/pkgs/container/fuel-core
[fuel-core-ingress-delete]: https://github.com/FuelLabs/fuel-core/blob/master/deployment/scripts/fuel-core-ingress-delete.sh
[fuel-core-ingress-deploy]: https://github.com/FuelLabs/fuel-core/blob/master/deployment/scripts/fuel-core-ingress-deploy.sh
[fuel-delete-script]: https://github.com/FuelLabs/fuel-core/blob/master/deployment/scripts/fuel-core-delete.sh
[fuel-deploy-script]: https://github.com/FuelLabs/fuel-core/blob/master/deployment/scripts/fuel-core-deploy.sh
[fuel-helm-chart]: https://github.com/FuelLabs/fuel-core/tree/master/deployment/charts
[gettext-cli]: https://www.gnu.org/software/gettext/
[grafana]: https://grafana.com/
[helm]: https://helm.sh/docs/intro/install/
[iam-auth]: https://docs.aws.amazon.com/eks/latest/userguide/install-aws-iam-authenticator.html
[ingress-controller]: https://github.com/kubernetes/ingress-nginx
[ingress-def]: https://kubernetes.io/docs/concepts/services-networking/ingress/
[kubectl-cli]: https://kubernetes.io/docs/tasks/tools/
[terraform]: https://learn.hashicorp.com/tutorials/terraform/install-cli
