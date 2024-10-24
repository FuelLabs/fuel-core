use strum_macros::EnumIter;

pub(crate) const V1_REQUEST_RESPONSE_PROTOCOL_ID: &str = "/fuel/req_res/0.0.1";
pub(crate) const V2_REQUEST_RESPONSE_PROTOCOL_ID: &str = "/fuel/req_res/0.0.2";

#[derive(Debug, Default, Clone, EnumIter)]
pub enum RequestResponseProtocol {
    #[default]
    V1,
    V2,
}

impl AsRef<str> for RequestResponseProtocol {
    fn as_ref(&self) -> &str {
        match self {
            RequestResponseProtocol::V1 => V1_REQUEST_RESPONSE_PROTOCOL_ID,
            RequestResponseProtocol::V2 => V2_REQUEST_RESPONSE_PROTOCOL_ID,
        }
    }
}
