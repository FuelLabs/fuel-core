# General
variable "environment" {
  type = string
}

variable "account_id" {
  type = string
}

variable "region" {
  type = string
}

# EKS
variable "eks_cluster_name" {
  type = string
}

variable "eks_cluster_version" {
  type = string
}

variable "eks_node_groupname" {
  type = string
}

variable "eks_node_ami_type" {
  type = string
}

variable "eks_node_disk_size" {
  type = string
}

variable "eks_node_instance_types" {
  type = list(string)
}

variable "eks_node_min_size" {
  type = string
}

variable "eks_node_desired_size" {
  type = string
}

variable "eks_node_max_size" {
  type = string
}

variable "eks_capacity_type" {
  type = string
}

variable "azs" {
  type = list(string)
}

variable "private_subnets" {
  type = list(string)
}

variable "public_subnets" {
  type = list(string)
}

variable "vpc_cidr_block" {
  type = string
}

variable "ec2_ssh_key" {
  type = string
}


module "fuel-core-aws-deploy" {
  source = "../../modules/eks"

    # Environment
  
  environment = var.environment

  # AWS
  region              = var.region
  account_id          = var.account_id

  # Networking
  vpc_cidr_block      = var.vpc_cidr_block  
  azs                 = var.azs
  public_subnets      = var.public_subnets
  private_subnets     = var.private_subnets

  # EKS
  eks_cluster_name          = var.eks_cluster_name 
  eks_cluster_version       = var.eks_cluster_version 
  eks_node_groupname        = var.eks_node_groupname
  eks_node_ami_type         = var.eks_node_ami_type 
  eks_node_disk_size        = var.eks_node_disk_size
  eks_node_instance_types   = var.eks_node_instance_types
  eks_node_min_size         = var.eks_node_min_size
  eks_node_desired_size     = var.eks_node_desired_size
  eks_node_max_size         = var.eks_node_max_size 
  eks_capacity_type         = var.eks_capacity_type
  ec2_ssh_key               = var.ec2_ssh_key 

}

