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
variable "eks-cluster-name" {
  type = string
}

variable "eks-cluster-version" {
  type = string
}

variable "eks-node-groupname" {
  type = string
}

variable "eks-node-ami-type" {
  type = string
}

variable "eks-node-disk-size" {
  type = string
}

variable "eks-node-instance-types" {
  type = list(string)
}

variable "eks-node-min-size" {
  type = string
}

variable "eks-node-desired-size" {
  type = string
}

variable "eks-node-max-size" {
  type = string
}

variable "eks-capacity-type" {
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

variable "ec2-ssh-key" {
  type = string
}


