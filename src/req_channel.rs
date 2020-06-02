use async_trait::async_trait;
use failure::Error;
use futures::{stream::Stream, future::Future};

use crate::message::Message;

/// A trait describing any medium capable of carrying pinreq messages.
#[async_trait]
pub trait ReqChannel {
    /// Send a pinreq `Message` to this channel
    async fn send_msg(&self, msg: &Message) -> Result<(), Error>;
    /// Receive a stream of parsed pinreq messages for processing
    // async fn listen(&self) -> Result<Box<dyn Stream<Item = Result<Vec<Message>, Error>>>, Error>;
    async fn listen(&self) -> Result<Box<dyn Future<Output = ()>>, Error>;
}

/// A trait for settings -> channel conversion
pub trait ChannelSettings {
    /// Turn a freshly loaded config into a full-blown channel
    fn to_channel(&self) -> Result<Box<dyn ReqChannel>, Error>;
}
