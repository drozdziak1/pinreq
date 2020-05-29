use async_trait::async_trait;
use chrono::{DateTime, Utc};
use failure::Error;
use futures::{
    future::{self, Future},
    stream::{self, Stream, TryStream},
};
use hyper::client::HttpConnector;
use ruma_client::{
    api::r0,
    events::{
        room::message::{TextMessageEventContent, MessageEventContent},
        EventType,
    },
    identifiers::{RoomAliasId, RoomId},
    Client, Session,
};
use serde_json::value::to_raw_value as to_raw_json_value;
use url::Url;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    convert::TryFrom,
    sync::{Arc, Mutex},
};

use crate::{
    matrix::MatrixError,
    message::Message,
    req_channel::{ChannelSettings, ReqChannel},
    utils::ErrBox,
};

pub struct MatrixChannel {
    pub settings: MatrixChannelSettings,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MatrixChannelSettings {
    /// Human-readable name of this Matrix channel
    pub name: String,
    pub homeserver: Url,
    pub room_alias: RoomAliasId,
    pub session: Option<Session>,
}

impl MatrixChannel {
    pub fn new(name: &str, homeserver: Url, room_alias: RoomAliasId) -> Result<Self, Error> {
        Ok(Self {
            settings: MatrixChannelSettings {
                name: name.to_owned(),
                homeserver,
                room_alias: RoomAliasId::try_from(room_alias)?,
                session: None,
            },
        })
    }

    /// Attempts to log onto `self.homeserver`. The `password` requires ownership for extra
    /// confidence that the password is dropped after use. (or cloned intentionally if need be)
    /// fills `self.session` on success. If `self.session` is `Some` a new session overwrites the
    /// present one.
    pub async fn log_in(&mut self, username: &str, password: String) -> Result<(), Error> {
        let client = Client::https(self.settings.homeserver.clone(), None);

        self.settings.session = Some(
            client
                .log_in(username.to_owned(), password, None, None)
                .await?,
        );
        Ok(())
    }

    pub fn get_session<'a>(&'a self) -> Result<&'a Session, Error> {
        Ok(self.settings.session.as_ref().ok_or_else(|| {
            format_err!(
                "No session established for Matrix channel {}",
                self.settings.name
            )
        })?)
    }

    /// Verify that a given Matrix room is available
    pub fn check_room(&self) -> Result<(), Error> {
        unimplemented!();
    }

    /// List all joined rooms.
    async fn joined_rooms(&self) -> Result<HashSet<String>, Error> {
        unimplemented!();
    }

    /// Dereference an alias to a room ID; used by `MatrixChannel::new()`
    async fn alias2id(&self, room_alias: RoomAliasId) -> Result<RoomId, Error> {
        let session = self.get_session()?;
        let settings = &self.settings;

        let client = Client::https(settings.homeserver.clone(), Some(session.clone()));

        let res = client.request(r0::alias::get_alias::Request {
            room_alias: room_alias,
        }).await?;

        Ok(res.room_id)
    }
}

#[async_trait]
impl ReqChannel for MatrixChannel {
    async fn send_msg(&self, msg: &Message) -> Result<(), Error> {
        let session = self.get_session()?;
        let settings = &self.settings;

        let client = Client::https(settings.homeserver.clone(), Some(session.clone()));

        let room_id = self.alias2id(settings.room_alias.clone()).await?;

        let response = client
            .request(r0::message::create_message_event::Request {
                room_id,
                event_type: EventType::RoomMessage,
                // Matrix's measure for request idempotency; must be unique
                txn_id: format!("{:?}:{}", msg.kind, Utc::now().to_rfc3339()),
                data: to_raw_json_value(&MessageEventContent::Text(TextMessageEventContent {
                    body: serde_json::to_string(msg)?,
                    format: None,
                    formatted_body: None,
                    relates_to: None,
                }))?,
            })
            .await?;

        debug!("Got response: {:?}", response);

        Ok(())
    }

    fn listen(&self) -> Result<Box<dyn Stream<Item = Vec<Message>>>, Error> {
        unimplemented!();
    }
}

impl ChannelSettings for MatrixChannelSettings {
    fn to_channel(&self) -> Result<Box<ReqChannel>, Error> {
        Ok(Box::new(MatrixChannel {
            settings: self.clone(),
        }))
    }
}
