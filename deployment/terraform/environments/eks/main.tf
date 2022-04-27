# AWS General
variable "aws_environment" {
  type = string
}

variable "aws_account_id" {
  type = string
}

variable "aws_region" {
  type = string
}

# AWS EKS
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

variable "aws_azs" {
  type = list(string)
}

variable "aws_private_subnets" {
  type = list(string)
}

variable "aws_public_subnets" {
  type = list(string)
}

variable "aws_vpc_cidr_block" {
  type = string
}

variable "ec2_ssh_key" {
  type = string
}


module "fuel-core-aws-deploy" {
  source = "../../modules/eks"

  # AWS
  aws_environment             = var.aws_environment
  aws_region                  = var.aws_region
  aws_account_id              = var.aws_account_id

  # Networking
  aws_vpc_cidr_block          = var.aws_vpc_cidr_block  
  aws_azs                     = var.aws_azs
  aws_public_subnets          = var.aws_public_subnets
  aws_private_subnets         = var.aws_private_subnets

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

