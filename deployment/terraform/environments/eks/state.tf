terraform {
  backend "s3" {
    bucket = "${TF_state_bucket}"
    key    = "${TF_state_bucket_key}"
    region = "${TF_VAR_region}"
  }
}
