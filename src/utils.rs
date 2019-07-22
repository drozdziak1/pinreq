use failure::Error;
use hyper::{client::HttpConnector, Body, Client, Request, Uri};
use hyper_tls::HttpsConnector;

/// Creates a new HTTPS hyper client
pub fn new_https_client() -> Result<Client<HttpsConnector<HttpConnector>>, Error> {
    let https = HttpsConnector::new(4)?;
    Ok(Client::builder()
        .keep_alive(false)
        .build::<_, hyper::Body>(https))
}
