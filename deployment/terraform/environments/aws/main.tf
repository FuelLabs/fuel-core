module "fuel-core-aws-deploy" {
  source = "../../modules/eks"

  # Environment
  environment = "production"

  # AWS
  region              = "us-east-1"
  account_id          = "123456789123"

  # Networking
  vpc_cidr_block      = "10.128.0.0/20"
  azs                 = ["us-east-1a", "us-east-1b", "us-east-1c"]
  public_subnets      = ["10.128.0.0/24", "10.128.1.0/24", "10.128.2.0/24"]
  private_subnets     = ["10.128.4.0/24", "10.128.5.0/24", "10.128.6.0/24"]

  # EKS
  eks-cluster-name          = "fuel-core-k8s-cluster"
  eks-cluster-version       = "1.21"
  eks-node-groupname        = "nodes"
  eks-node-ami-type         = "AL2_x86_64"
  eks-node-disk-size        = "100"
  eks-node-instance-types   = ["t3.xlarge"]
  eks-node-min-size         = "2"
  eks-node-desired-size     = "2"
  eks-node-max-size         = "3"
  eks-capacity-type         = "ON_DEMAND"
  ec2-ssh-key               = "fuel-core-ssh-key"

}
