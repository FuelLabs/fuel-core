# EKS Cluster
module "eks" {
  source = "terraform-aws-modules/eks/aws"

  cluster_name                    = "${var.eks-cluster-name}"
  cluster_version                 = "${var.eks-cluster-version}"
  cluster_endpoint_private_access = true
  cluster_endpoint_public_access  = true

  cluster_addons = {
    kube-proxy = {}
    vpc-cni = {
      resolve_conflicts = "OVERWRITE"
    }
  }

  vpc_id     = module.vpc.vpc_id
  subnet_ids = concat(module.vpc.public_subnets, module.vpc.private_subnets)
  create_iam_role = false
  iam_role_arn = aws_iam_role.eks-cluster-iam-role.arn
  create_cluster_security_group= false
  cluster_security_group_id = aws_security_group.eks-cluster-sg.id

  ## EKS Managed Node Groups
  eks_managed_node_groups = {
    blue = {
      ami_type               = var.eks-node-ami-type
      instance_types         = var.eks-node-instance-types
      disk_size              = 100
      min_size               = var.eks-node-min-size
      max_size               = var.eks-node-max-size
      desired_size           = var.eks-node-desired-size

      capacity_type  = var.eks-capacity-type 
      subnet_ids = module.vpc.private_subnets
      create_node_security_group = false
      node_security_group_id = aws_security_group.eks-node-sg.id
      create_iam_role = true
      iam_role_additional_policies  = ["arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore","arn:aws:iam::aws:policy/AmazonEKSWorkerNodePolicy","arn:aws:iam::aws:policy/AmazonEKS_CNI_Policy","arn:aws:iam::aws:policy/AmazonEC2ContainerRegistryReadOnly","arn:aws:iam::aws:policy/CloudWatchAgentServerPolicy"]
    }
  }
}

# EKS Cluster CoreDNS Cluster AddOn 

resource "aws_eks_addon" "core_dns" {
  cluster_name = module.eks.cluster_id
  addon_name        = "coredns"
  addon_version     = "v1.8.4-eksbuild.1"
  resolve_conflicts = "OVERWRITE"
}

# EKS Cluster IAM Role

resource "aws_iam_role" "eks-cluster-iam-role" {
  name = "eks-cluster-iam-role"

  assume_role_policy = jsonencode({
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "eks.amazonaws.com"
      }
    }]
    Version = "2012-10-17"
  })
}

resource "aws_iam_role_policy_attachment" "eks-cluster-AmazonEKSClusterPolicy" {
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKSClusterPolicy"
  role       = aws_iam_role.eks-cluster-iam-role.name
}

resource "aws_iam_role_policy_attachment" "eks-node-AmazonEKSWorkerNodePolicy" {
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKSWorkerNodePolicy"
  role       = aws_iam_role.eks-cluster-iam-role.name
}

resource "aws_iam_role_policy_attachment" "eks-node-AmazonEKS_CNI_Policy" {
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKS_CNI_Policy"
  role       = aws_iam_role.eks-cluster-iam-role.name
}

resource "aws_iam_role_policy_attachment" "eks-node-AmazonEC2ContainerRegistryReadOnly" {
  policy_arn = "arn:aws:iam::aws:policy/AmazonEC2ContainerRegistryReadOnly"
  role       = aws_iam_role.eks-cluster-iam-role.name
}

# EKS Security Groups 

## EKS Cluster Security Group
resource "aws_security_group" "eks-cluster-sg" {
  name          = "eks-cluster"
  description   = "Allow EKS Cluster Traffic"
  vpc_id        = module.vpc.vpc_id

  ingress {
    description = "Allow Public Traffic"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks =  ["0.0.0.0/0"]
  }

  ingress {
    description = "Allow VPC Traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["${var.vpc_cidr_block}"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

## EKS Node Security Group
resource "aws_security_group" "eks-node-sg" {
  name          = "eks-node"
  description   = "Allow EKS Node Traffic"
  vpc_id        = module.vpc.vpc_id

ingress {
    description = "Allow VPC Traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["${var.vpc_cidr_block}"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}