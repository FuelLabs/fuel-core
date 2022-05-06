terraform {
  backend "s3" {
    bucket = "${TF_state_s3_bucket}"
    key    = "${TF_state_s3_bucket_key}"
    region = "${TF_VAR_aws_region}"
  }
}
