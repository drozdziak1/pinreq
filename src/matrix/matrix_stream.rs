use failure::Error;
use futures::{Async, Future, Poll, Stream};
use hyper::{client::HttpConnector, Body, Client, Request, Uri};
use hyper_tls::HttpsConnector;
use serde_json::Value;
use tokio::runtime::current_thread;
use url::Url;

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
    pub last_sync: Arc<Mutex<Option<String>>>,
    /// Current request future
    pub fut: Option<Box<Future<Item = Option<Vec<Message>>, Error = Error>>>,
}

impl Stream for MatrixStream {
    type Item = Vec<Message>;
    type Error = Error;
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // Check if there's a request future already in progress
        if let Some(fut) = self.fut.as_mut() {
            let poll_res = fut.poll();

            if let Ok(Async::Ready(_)) = poll_res.as_ref() {
                self.fut = None;
            }

            return poll_res;
        }

        let filter = json!({
            "room": {
                /*
                 *"account_data": {
                 *    "not_types": ["*"]
                 *},
                 *"ephemeral": {
                 *    "not_types": ["*"]
                 *},
                 */
                "rooms": [self.room_id],
                /*
                 *"state": {
                 *    "not_types": ["*"]
                 *},
                 */
                "timeline": {
                    "types": ["m.room.message"]
                },
            },
            "event_fields": [
                "type",
                "content"
            ],
        });

        debug!("Listening with filter: {:#?}", filter);

        let serialized = serde_json::to_string(&filter)?;
        debug!("Serialized filter: {:?}", serialized);

        let sync_opt = self
            .last_sync
            .lock()
            .map_err(|_| format_err!("Could not lock last_sync"))?
            .clone();
        debug!("Requesting batch {:?}", sync_opt);

        let mut url = Url::parse_with_params(
            format!("https://{}/_matrix/client/r0/sync", self.homeserver).as_str(),
            &[
            ("access_token", self.access_token.as_str()),
            ("filter", serialized.as_str()),
            ("full_state", "false"),
            ],
            )?;
        if let Some(since) = sync_opt {
            url.query_pairs_mut().append_pair("since", &since);
        }

        debug!("Hitting URI {}", url.as_str());

        let uri = url
            .as_str()
            .parse::<Uri>()
            .map_err(|e| format_err!("Could not parse URI: {:?}", e))?;

        let req = Request::builder()
            .method("GET")
            .header("Contnent-Type", "application/json")
            .uri(uri.clone())
            .body(Body::empty())?;

        let room_id = self.room_id.clone();

        let mut last_sync = self.last_sync.clone();

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
                        trace!("Parsed: {:?}", parsed);
                        if let Some(obj) = parsed.as_object() {
                            let keys: Vec<_> = obj.keys().collect();

                            debug!("{} response keys: {:?}", keys.len(), keys);
                        }

                        let rooms = if let Some(rooms) = parsed.get("rooms") {
                            rooms
                        } else {
                            return Ok(Some(Vec::new()));
                        };

                        let join = if let Some(join) = rooms.get("join") {
                            join
                        } else {
                            return Ok(Some(Vec::new()));
                        };

                        if let Some(obj) = join.as_object() {
                            let keys: Vec<_> = obj.keys().collect();

                            debug!("{} rooms keys: {:?}", keys.len(), keys);
                        }

                        let join_room_id = if let Some(join_room_id) = join.get(&room_id) {
                            join_room_id
                        } else {
                            return Ok(Some(Vec::new()));
                        };

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

                                debug!("Parsed event: {:#?}", msg);

                                Ok(msg)
                            })
                        .collect::<Result<Vec<Message>, Error>>()?;

                        let next_batch = parsed
                            .get("next_batch")
                            .ok_or(format_err!("Could not reach `next-batch`"))?
                            .as_str()
                            .ok_or(format_err!("Could not view next_batch as string"))?;

                        trace!("Next batch: {}", next_batch);

                        let mut last_sync_lock = last_sync
                            .lock()
                            .map_err(|_| format_err!("Could not lock last_sync"))?;

                        *last_sync_lock = Some(next_batch.to_owned());
                        Ok(Some(msgs))
                    })
            });

        self.fut = Some(Box::new(fut));

        self.fut.as_mut().unwrap().poll()
    }
}
