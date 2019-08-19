use failure::Error;
use futures::{Async, Future, Poll, Stream};
use hyper::{client::HttpConnector, Body, Client, Request, Uri};
use hyper_tls::HttpsConnector;
use serde_json::Value;
use tokio::runtime::current_thread;

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{matrix::MatrixError, message::Message, req_channel::ReqChannel};

/// A `Stream` implementation for continuous fetching of pinreq messages.
pub struct MatrixStream {
    pub client: Client<HttpsConnector<HttpConnector>>,
    pub homeserver: String,
    pub room_id: String,
    pub access_token: String,
    /// When to continue syncing from, None means first sync
    pub since: Option<String>,
    /// Current request future
    pub fut: Option<Box<Future<Item = Option<Vec<Message>>, Error = Error>>>,
}

impl Stream for MatrixStream {
    type Item = Vec<Message>;
    type Error = Error;
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // Check if there's a request future already in progress
        if let Some(fut) = self.fut.as_mut() {
            return fut.poll();
        }

        let filter = json!({
            "room": {
                "account_data": {
                    "not_types": ["*"]
                },
                "ephemeral": {
                    "not_types": ["*"]
                },
                "rooms": [self.room_id],
                "state": {
                    "not_types": ["*"]
                },
                "timeline": {
                    "types": ["m.room.message"]
                },
            },
            "event_fields": [
                "type",
                "content"
            ],
            "presence": {
                "not_types": "m.*"
            },
            "account_data": {
                "not_types": "m.*"
            },
        });

        debug!("Listening with filter: {:#?}", filter);

        let serialized = serde_json::to_string(&filter)?;
        trace!("Serialized filter: {:?}", serialized);

        let req_body = json!({
            "filter": serialized,
        })
        .to_string();

        let uri = format!(
            "https://{}/_matrix/client/r0/sync?access_token={}",
            self.homeserver, self.access_token
        )
        .parse::<Uri>()
        .map_err(|e| format_err!("Could not parse URI: {:?}", e))?;

        debug!("Hitting URI {}", uri);

        let req = Request::builder()
            .method("GET")
            .header("Contnent-Type", "application/json")
            .uri(uri.clone())
            .body(Body::from(req_body))?;

        let room_id = self.room_id.clone();

        let fut = self
            .client
            .request(req)
            .from_err::<Error>()
            .and_then(|res| {
                trace!("Got response {:?}", res);
                res.into_body()
                    .concat2()
                    .from_err()
                    .and_then(move |chunks| {
                        trace!("Got chunks: {:?}", chunks);
                        let parsed: Value = serde_json::from_slice(&*chunks)?;
                        debug!("Parsed: {:?}", parsed);
                        if let Some(obj) = parsed.as_object() {
                            let keys: Vec<_> = obj.keys().collect();

                            debug!("{} response keys: {:?}", keys.len(), keys);
                        }

                        let rooms = parsed
                            .get("rooms")
                            .ok_or(format_err!("Could not find key `rooms` in response object"))?;
                        let join = rooms.get("join").ok_or(format_err!(
                            "Could not find key `join` in response object `rooms`"
                        ))?;

                        if let Some(obj) = join.as_object() {
                            let keys: Vec<_> = obj.keys().collect();

                            debug!("{} rooms keys: {:?}", keys.len(), keys);
                        }

                        let join_room_id = join.get(&room_id).ok_or(format_err!(
                            "{}: Could not find the room's entry in response object",
                            room_id
                        ))?;

                        if let Some(obj) = join_room_id.as_object() {
                            let keys: Vec<_> = obj.keys().collect();

                            debug!("{} {} keys: {:?}", keys.len(), room_id, keys);
                        }

                        let timeline = join_room_id
                            .get("timeline")
                            .ok_or(format_err!("{}: Could not find `timeline`", room_id))?;

                        if let Some(obj) = timeline.as_object() {
                            let keys: Vec<_> = obj.keys().collect();

                            debug!("{} timeline keys: {:?}", keys.len(), keys);
                        }

                        let events = timeline
                            .get("events")
                            .ok_or(format_err!("Could not reach `events`"))?
                            .as_array()
                            .ok_or(format_err!("Could not view `events` as array"))?;

                        let msgs: Vec<Message> = events
                            .iter()
                            .map(|e| -> Result<Message, Error> {
                                let content = e
                                    .get("content")
                                    .ok_or(format_err!("Could not get event contents"))?
                                    .as_object()
                                    .ok_or(format_err!("Could not view `content` as object"))?;
                                let msgtype = content
                                    .get("msgtype")
                                    .ok_or(format_err!("Could not check msgtype"))?
                                    .as_str()
                                    .ok_or(format_err!("Could not view `msgtype` as string"))?;

                                let msg: Message = match msgtype {
                                    "m.text" => {
                                        let body = content
                                            .get("body")
                                            .ok_or(format_err!("Could not get message body"))?
                                            .as_str()
                                            .ok_or(format_err!("Could not view body as string"))?;
                                        debug!("Parsing message: {}", body);
                                        serde_json::from_slice(body.as_bytes())?
                                    }
                                    other => bail!("Got unknown message type {}", other),
                                };

                                info!("Parsed event: {:#?}", msg);

                                Ok(msg)
                            })
                            .collect::<Result<Vec<Message>, Error>>()?;

                        Ok(Some(msgs))
                    })
            });

        self.fut = Some(Box::new(fut));

        self.fut.as_mut().unwrap().poll()
    }
}
