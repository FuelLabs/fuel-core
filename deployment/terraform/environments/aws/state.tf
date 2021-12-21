terraform {
  backend "s3" {
    bucket = "fuel-terraform"
    key    = "test"
    region = "us-east-1"
  }
}

