# General
variable "environment" {
  type = string
}

# AWS
variable "eks-cluster-name" {
  type = string
}

variable "account_id" {
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

variable "region" {
  type = string
}

variable "vpc_cidr_block" {
  type = string
}



