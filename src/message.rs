use failure::Error;
use gpgme::{Context, Data, SignMode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
/// A type for differentiating between
pub enum MessageKind {
    /// "Please pin this for me"
    Pin(String),
    /// "I have pinned this"
    Confirm(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
/// The full message type
pub struct Message {
    /// What the message says
    pub kind: MessageKind,
    /// An ASCII-armored detached signature
    signature: String,
}

impl Message {
    pub fn from_kind(kind: MessageKind, ctx: &mut Context) -> Result<Self, Error> {
        ctx.set_armor(true);
        let encoded_kind = serde_json::to_string(&kind)?;
        info!("Encoded kind:\n{}", encoded_kind);

        let mut signature: Vec<u8> = Vec::new();

        let kind_data = Data::from_buffer(&encoded_kind)?;

        ctx.sign(SignMode::Detached, kind_data, &mut signature)?;

        Ok(Self {
            kind,
            signature: String::from_utf8(signature)?,
        })
    }
}
