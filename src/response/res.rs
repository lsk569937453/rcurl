use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use hyper::Response as HyperResponse;

use std::convert::Infallible;
#[derive(Debug)]
pub enum RcurlResponse {
    Ftp(()),
    Http(HyperResponse<BoxBody<Bytes, Infallible>>),
    Ping(()),
    DiskSize(()),
    Telnet(()),
}
