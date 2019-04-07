use failure::Error;
use futures::stream::Stream;

use crate::message::Message;

/// A trait describing any medium capable of carrying pinreq messages.
pub trait ReqChannel {
    /// Send a pinreq `Message` to this channel
    fn send_msg(msg: &Message) -> Result<(), Error>;
    /// Receive a stream of parsed pinreq messages for processing
    fn listen() -> Stream<Item = Message, Error = Error>;
    /// Inform everyone in the channel that you have pinned `ipfs_hash`
    fn confirm(ipfs_hash: &str) -> Result<(), Error>;
}
