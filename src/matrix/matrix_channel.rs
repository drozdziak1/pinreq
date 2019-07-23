use failure::Error;
use futures::{
    future::{self, Future},
    stream::{self, Stream},
};
use hyper::{client::HttpConnector, Body, Client, Request, Uri};
use hyper_tls::HttpsConnector;
use serde_json::Value;
use tokio::runtime::current_thread;

use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    matrix::{MatrixError, MatrixStream},
    message::Message,
    req_channel::{ChannelSettings, ReqChannel},
    utils::new_https_client,
};

pub struct MatrixChannel {
    /// A hyper client instance
    pub client: Client<HttpsConnector<HttpConnector>>,
    pub settings: MatrixChannelSettings,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MatrixChannelSettings {
    /// Human-readable name of this Matrix channel
    pub name: String,
    pub homeserver: String,
    pub room_id: String,
    pub access_token: Option<String>,
}

impl MatrixChannel {
    pub fn new(name: &str, homeserver: &str, room_alias: &str) -> Result<Self, Error> {
        let homeserver = homeserver.to_owned();

        let client = new_https_client()?;

        let mut new_self = Self {
            client,
            settings: MatrixChannelSettings {
                name: name.to_owned(),
                homeserver,
                room_id: "".to_owned(),
                access_token: None,
            },
        };

        let room_id = current_thread::block_on_all(new_self.alias2id(room_alias))?;

        new_self.settings.room_id = room_id;

        Ok(new_self)
    }

    /// Attempts to log onto `self.homeserver`. The `password` requires ownership for extra
    /// confidence that the password is dropped after use. (or cloned intentionally if need be);
    /// fills `self.access_token` on success.
    pub fn log_in(&mut self, username: &str, password: String) -> Result<(), Error> {
        let uri = match format!(
            "https://{}/_matrix/client/r0/login",
            self.settings.homeserver
        )
        .parse::<Uri>()
        {
            Ok(u) => u,
            Err(e) => {
                bail!("Could not parse server URI: {:?}", e);
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
            debug!("Got response {:?}", res);
            res.into_body().concat2().from_err().and_then(|chunks| {
                debug!("Got chunks: {:?}", chunks);
                let parsed: Value = serde_json::from_slice(&*chunks)?;

                let access_token = match parsed.get("access_token") {
                    Some(Value::String(t)) => t.clone(),
                    Some(other) => bail!("access_token is not a String! (Got {})", other),
                    None => bail!("Could not get access_token from response"),
                };
                Ok(access_token)
            })
        });

        self.settings.access_token = Some(current_thread::block_on_all(fut)?);

        Ok(())
    }

    /// Verify that a given Matrix room is available
    pub fn check_room(&self) -> Result<(), Error> {
        let id = self.settings.room_id.clone();
        let fut = self
            .joined_rooms()
            .and_then(|rooms| Ok(rooms.contains(&id)));

        let room_is_joined = current_thread::block_on_all(fut)?;

        debug!("{} joined: {}", self.settings.room_id, room_is_joined);

        if room_is_joined {
            Ok(())
        } else {
            Err(MatrixError::RoomNotJoined(self.settings.room_id.clone()).into())
        }
    }

    /// List all joined rooms.
    pub fn joined_rooms(&self) -> Box<Future<Item = HashSet<String>, Error = Error>> {
        let access_token = match self.settings.access_token.as_ref() {
            Some(t) => t.clone(),
            None => return Box::new(future::err(MatrixError::NotAuthenticated.into())),
        };

        let rooms_uri = match format!(
            "https://{}/_matrix/client/r0/joined_rooms?access_token={}",
            self.settings.homeserver, access_token
        )
        .parse::<Uri>()
        {
            Ok(uri) => uri,
            Err(e) => return Box::new(future::err(format_err!("Could not parse URI: {:?}", e))),
        };

        debug!("Hitting URI {}", rooms_uri);

        let fut = self
            .client
            .get(rooms_uri)
            .from_err::<Error>()
            .and_then(|res| {
                res.into_body()
                    .concat2()
                    .from_err::<Error>()
                    .and_then(|chunks| {
                        trace!("Got chunks: {:?}", chunks);
                        let parsed: HashMap<String, HashSet<String>> =
                            serde_json::from_slice(&*chunks)?;

                        trace!("Parsed: {:#?}", parsed);

                        let rooms = parsed
                            .get("joined_rooms")
                            .ok_or_else(|| {
                                MatrixError::ResponseNotUnderstood(
                                    "No joined_rooms in response object".to_owned(),
                                )
                            })?
                            .clone();

                        Ok(rooms)
                    })
                    .from_err()
            });

        Box::new(future::ok::<_, Error>(fut).flatten())
    }

    /// Dereference an alias to a room ID; used by `MatrixChannel::new()`
    fn alias2id(&self, room_alias: &str) -> Box<Future<Item = String, Error = Error>> {
        let alias2id_uri = match format!(
            "https://{}/_matrix/client/r0/directory/room/{}",
            self.settings.homeserver, room_alias
        )
        .parse::<Uri>()
        {
            Ok(u) => u,
            Err(e) => return Box::new(future::err(format_err!("Could not parse URI: {:?}", e))),
        };

        debug!("Hitting URI {}", alias2id_uri);
        let fut = self
            .client
            .get(alias2id_uri)
            .from_err::<Error>()
            .and_then(|res| {
                res.into_body()
                    .concat2()
                    .from_err::<Error>()
                    .and_then(|chunks| {
                        trace!("Got chunks: {:?}", chunks);
                        let parsed: Value = serde_json::from_slice(&*chunks)?;

                        let id = parsed
                            .get("room_id")
                            .ok_or_else::<Error, _>(|| {
                                MatrixError::ResponseNotUnderstood(
                                    "No room_id in response object".to_owned(),
                                )
                                .into()
                            })?
                            .clone();

                        trace!("Parsed: {:#?}", parsed);

                        Ok(id
                            .as_str()
                            .ok_or_else(|| {
                                MatrixError::ResponseNotUnderstood(
                                    "Cannot deserialize room_id as string".to_owned(),
                                )
                            })?
                            .to_owned())
                    })
                    .from_err()
            });

        Box::new(future::ok::<_, Error>(fut).flatten())
    }

    pub fn listen(&self) -> Result<MatrixStream, Error> {
        Ok(MatrixStream {
            client: self.client.clone(),
            homeserver: self.settings.homeserver.clone(),
            room_id: self.settings.room_id.clone(),
            access_token: self
                .settings
                .access_token
                .as_ref()
                .ok_or_else(|| MatrixError::NotAuthenticated)?
                .to_owned(),
            since: None,
            messages: Default::default(),
        })
    }
}

impl ReqChannel for MatrixChannel {
    fn send_msg(&self, msg: &Message) -> Result<(), Error> {
        self.check_room()?;

        let access_token = self
            .settings
            .access_token
            .as_ref()
            .ok_or_else(|| MatrixError::NotAuthenticated)?
            .clone();

        let req_body = json!({
            "msgtype": "m.text",
            "body": serde_json::to_string(msg)?
        })
        .to_string();

        trace!("Serialized message: {:?}", req_body);

        let uri = format!(
            "https://{}/_matrix/client/r0/rooms/{}/send/m.room.message?access_token={}",
            self.settings.homeserver, self.settings.room_id, access_token
        )
        .parse::<Uri>()
        .map_err(|e| format_err!("Could not parse URI: {:?}", e))?;

        debug!("Hitting URI {}", uri);

        let req = Request::builder()
            .method("POST")
            .header("Content-Type", "application/json")
            .uri(uri.clone())
            .body(Body::from(req_body))?;

        let fut = self.client.request(req).from_err().and_then(|res| {
            trace!("Got response {:?}", res);
            res.into_body().concat2().from_err().and_then(|chunks| {
                debug!("Got chunks: {:?}", chunks);
                let parsed: Value = serde_json::from_slice(&*chunks)?;

                let event_id = match parsed.get("event_id") {
                    Some(Value::String(i)) => i.clone(),
                    Some(other) => bail!("event_id is not a String! (Got {})", other),
                    None => bail!("Could not get event_id from response"),
                };
                debug!(
                    "Sent the message successfully, Matrix event ID: {}",
                    event_id
                );

                Ok(())
            })
        });

        current_thread::block_on_all(fut)
    }
}

impl ChannelSettings for MatrixChannelSettings {
    fn to_channel(&self) -> Result<Box<ReqChannel>, Error> {
        Ok(Box::new(MatrixChannel {
            client: new_https_client()?,
            settings: self.clone(),
        }))
    }
}
