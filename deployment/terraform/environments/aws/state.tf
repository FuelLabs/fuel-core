terraform {
  backend "s3" {
    bucket = "terraform-deploy"
    key    = "eks-deploy"
    region = "us-east-1"
  }
}

