use failure::Error;
use futures::stream::Stream;

use crate::message::Message;

/// A trait describing any medium capable of carrying pinreq messages.
pub trait ReqChannel {
    /// Send a pinreq `Message` to this channel
    fn send_msg(&self, msg: &Message) -> Result<(), Error>;
    ///// Receive a stream of parsed pinreq messages for processing
    fn listen(&self) -> Result<Box<Stream<Item = Vec<Message>, Error = Error>>, Error>;
}

/// A trait for settings -> channel conversion
pub trait ChannelSettings {
    /// Turn a freshly loaded config into a full-blown channel
    fn to_channel(&self) -> Result<Box<ReqChannel>, Error>;
}
