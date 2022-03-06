locals {
  vpc_eks_tag_key = "kubernetes.io/cluster/${var.eks_cluster_name}"
  vpc_eks_tag = {
    (local.vpc_eks_tag_key) = "shared"
  }
  private_subnet_eks_tag = {
    "kubernetes.io/role/internal-elb" = "1"
  }
  public_subnet_eks_tag = {
    "kubernetes.io/role/elb" = "1"
  }
  environment_tag = {
    Environment = var.environment
  }
}

module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "3.11.0"

  name = var.environment
  cidr = var.vpc_cidr_block

  azs                 = var.azs
  public_subnets      = var.public_subnets
  public_subnet_tags  = local.public_subnet_eks_tag
  private_subnets     = var.private_subnets
  private_subnet_tags = local.private_subnet_eks_tag

  enable_nat_gateway = true
  single_nat_gateway = true
  tags               = merge(local.environment_tag, local.vpc_eks_tag)
}
