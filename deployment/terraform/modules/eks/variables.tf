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



