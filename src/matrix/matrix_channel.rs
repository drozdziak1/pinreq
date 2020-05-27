use failure::Error;
use futures::{
    future::{self, Future},
    stream::{self, Stream, TryStream},
};
use hyper::client::{HttpConnector};
use ruma_client::{Client, Session};
use ruma_identifiers::RoomAliasId;
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
    pub room_id: RoomAliasId,
    pub session: Option<Session>,
}

impl MatrixChannel {
    pub fn new(name: &str, homeserver: Url, room_alias: RoomAliasId) -> Result<Self, Error> {
        Ok(Self {
            settings: MatrixChannelSettings {
                name: name.to_owned(),
                homeserver,
                room_id: RoomAliasId::try_from(room_alias)?,
                session: None,
            },
        })
    }

    /// Attempts to log onto `self.homeserver`. The `password` requires ownership for extra
    /// confidence that the password is dropped after use. (or cloned intentionally if need be);
    /// fills `self.session` on success.
    pub fn log_in(&mut self, username: &str, password: String) -> Result<(), Error> {
        unimplemented!();
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
    async fn alias2id(&self, room_alias: &str) -> Result<String, Error> {
        unimplemented!();
    }
}

impl ReqChannel for MatrixChannel {
    fn send_msg(&self, msg: &Message) -> Result<(), Error> {
        unimplemented!();
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
