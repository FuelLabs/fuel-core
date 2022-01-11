module "fuel-core-aws-deploy" {
  source = "../../modules/eks"

  # Environment
  environment = "terraform-test"

  # AWS
  region              = "us-east-1"
  account_id          = "756756437019"

  # Networking
  vpc_cidr_block      = "10.128.0.0/20"
  azs                 = ["us-east-1a", "us-east-1b", "us-east-1c"]
  public_subnets      = ["10.128.0.0/24", "10.128.1.0/24", "10.128.2.0/24"]
  private_subnets     = ["10.128.4.0/24", "10.128.5.0/24", "10.128.6.0/24"]

  # EKS
  eks-cluster-name          = "fuelcore-deploy"
  eks-cluster-version       = "1.21"
  eks-node-ami-type         = "AL2_x86_64"
  eks-node-disk-size        = "100"
  eks-node-instance-types   = ["t3.xlarge"]
  eks-node-min-size         = "1"
  eks-node-desired-size     = "1"
  eks-node-max-size         = "1"
  eks-capacity-type          = "ON_DEMAND"

}
