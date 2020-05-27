use failure::Error;
use futures::{
    future::{self, Future},
    stream::{self, Stream, TryStream},
};
use serde_json::Value;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use crate::{
    matrix::MatrixError,
    message::Message,
    req_channel::{ChannelSettings, ReqChannel},
    utils::{ ErrBox},
};

pub struct MatrixChannel {
    /// A hyper client instance
    pub settings: MatrixChannelSettings,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MatrixChannelSettings {
    /// Human-readable name of this Matrix channel
    pub name: String,
    pub homeserver: String,
    pub room_id: String,
    pub access_token: Option<String>,
    /// A timestamp of last sync
    pub last_sync: Option<String>,
}

impl MatrixChannel {
    pub fn new(name: &str, homeserver: &str, room_alias: &str) -> Result<Self, Error> {
        unimplemented!();
    }

    /// Attempts to log onto `self.homeserver`. The `password` requires ownership for extra
    /// confidence that the password is dropped after use. (or cloned intentionally if need be);
    /// fills `self.access_token` on success.
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
