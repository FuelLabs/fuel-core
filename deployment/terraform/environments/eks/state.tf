terraform {
  backend "s3" {
    bucket = "s3-bucket-name"
    key    = "s3-bucket-key"
    region = "us-east-1"
  }
}
