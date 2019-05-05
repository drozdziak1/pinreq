use failure::Error;


use crate::message::Message;

/// A trait describing any medium capable of carrying pinreq messages.
pub trait ReqChannel {
    /// Send a pinreq `Message` to this channel
    fn send_msg(&self, msg: &Message) -> Result<(), Error>;
    ///// Receive a stream of parsed pinreq messages for processing
    //fn listen(&self) -> Box<Stream<Item = Message, Error = Error>>;
}
