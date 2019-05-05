#[macro_use]
extern crate log;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

mod matrix_channel;
mod message;
mod req_channel;

use failure::Error;
use gpgme::{Context, Protocol};
use log::LevelFilter;


use std::{
    env,
    io::{self, Write},
};

use matrix_channel::MatrixChannel;
use message::{Message, MessageKind};
use req_channel::ReqChannel;

static DEFAULT_PINREQ_MATRIX_ROOM_ALIAS: &'static str = "%23ipfs-pinreq:matrix.org";

pub fn main() -> Result<(), Error> {
    // Set logging up
    match env::var("RUST_LOG") {
        Ok(_value) => env_logger::init(),
        Err(_e) => env_logger::Builder::new()
            .filter_level(LevelFilter::Info)
            .init(),
    }

    print!("Username: ");
    io::stdout().flush()?;
    let mut username = String::new();
    let stdin = io::stdin();
    stdin.read_line(&mut username)?;
    username = username.trim().to_owned();
    let mut channel = MatrixChannel::new("matrix.org", DEFAULT_PINREQ_MATRIX_ROOM_ALIAS)?;

    channel.log_in(&username, rpassword::prompt_password_stderr("Password: ")?)?;

    info!("Token is {:?}", channel.access_token);

    channel.check_room()?;

    let mut ctx = Context::from_protocol(Protocol::OpenPgp)?;

    let msg = Message::from_kind(MessageKind::Pin("/ipfs/QmHenlo".to_owned()), &mut ctx)?;

    info!("msg: {:#?}", msg);

    channel.send_msg(&msg)?;

    Ok(())
}
