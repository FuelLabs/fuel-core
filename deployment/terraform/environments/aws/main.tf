module "fuel-core-aws-deploy" {
  source = "../../modules/aws"

  # General
  environment = "terraform-test"

  # AWS
  eks-cluster-name    = "fuelcore-test"
  region              = "us-east-1"
  account_id          = "756756437019"
  vpc_cidr_block      = "10.128.0.0/20"
  azs                 = ["us-east-1a", "us-east-1b", "us-east-1c"]
  public_subnets      = ["10.128.0.0/24", "10.128.1.0/24", "10.128.2.0/24"]
  private_subnets     = ["10.128.4.0/24", "10.128.5.0/24", "10.128.6.0/24"]
}
