use failure::Error;
use futures::{
    future::{self, Future},
    stream::Stream,
};
use hyper::{client::HttpConnector, Body, Client, Request, Uri};
use hyper_tls::HttpsConnector;
use serde_json::Value;
use tokio::runtime::current_thread;

use crate::{message::Message, req_channel::ReqChannel};

#[derive(Debug, Fail)]
pub enum MatrixChannelError {
    #[fail(display = "You need an active auth token to use a Matrix channel")]
    NotAuthenticated,
}

pub struct MatrixChannel {
    /// A hyper client instance
    pub client: Client<HttpsConnector<HttpConnector>>,
    pub homeserver: String,
    pub room_name: String,
    pub access_token: Option<String>,
}

impl MatrixChannel {
    pub fn new(homeserver: &str, room_name: &str) -> Result<Self, Error> {
        let homeserver = homeserver.to_owned();
        let room_name = room_name.to_owned();

        let https = HttpsConnector::new(4)?;
        let client = Client::builder()
            .keep_alive(false)
            .build::<_, hyper::Body>(https);

        Ok(Self {
            client,
            homeserver,
            room_name,
            access_token: None,
        })
    }

    /// Attempts to log onto `self.homeserver`. The `password` requires ownership for extra
    /// confidence that the password is dropped after use. (or cloned intentionally if need be)
    pub fn log_in(
        &mut self,
        username: &str,
        password: String,
    ) -> Result<(), Error> {
        let uri =
            match format!("https://{}/_matrix/client/r0/login", self.homeserver).parse::<Uri>() {
                Ok(u) => u,
                Err(e) => {
                    bail!(
                        "Could not parse server URI: {:?}",
                        e
                    );
                }
            };

        let req_body = json!({
            "type": "m.login.password",
            "user": username,
            "password": password
        })
        .to_string();

        let req = match Request::builder()
            .method("POST")
            .header("Content-Type", "application/json")
            .uri(uri.clone())
            .body(Body::from(req_body))
        {
            Ok(r) => r,
            Err(e) => return Err(e.into()),
        };

        let fut = self.client.request(req).from_err().and_then(|res| {
            info!("Got response {:?}", res);
            res.into_body().concat2().from_err().and_then(|chunks| {
                info!("Got chunks: {:?}", chunks);
                let parsed: Value = serde_json::from_slice(&*chunks)?;

                let access_token = match parsed.get("access_token") {
                    Some(Value::String(t)) => t.clone(),
                    Some(other) => bail!("access_token is not a String! (Got {})", other),
                    None => bail!("Could not get access_token from request"),
                };
                Ok(access_token)
            })
        });

        self.access_token = Some(current_thread::block_on_all(fut)?);

        Ok(())
    }
}
