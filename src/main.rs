#[macro_use]
extern crate log;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

mod matrix;
mod message;
mod req_channel;

use clap::{App, Arg, ArgMatches, SubCommand};
use failure::Error;
use futures::Stream;
use gpgme::{Context, Protocol};
use log::LevelFilter;
use tokio::runtime::current_thread;

use std::{
    env,
    io::{self, Write},
};

use matrix::MatrixChannel;
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

    let matches = App::new("pinreq")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Stan Drozd <drozdziak1@gmail.com>")
        .about("The IPFS authenticated pin request system")
        .arg(
            Arg::with_name("all")
                .help("Use all configured rooms")
                .required(false)
                .takes_value(false)
                .short("a")
                .long("all"),
        )
        .arg(
            Arg::with_name("CHANNEL_ID")
                .min_values(1)
                .required_unless("all")
                .use_delimiter(true),
        )
        .arg(
            Arg::with_name("CONFIG_FILE")
                .help("Config file to use in this run of pinreq")
                .default_value("pinreq.toml")
                .takes_value(true)
                .short("c")
                .long("config"),
        )
        .subcommand(
            SubCommand::with_name("request")
                .about("Send a pin request to configured channels (all by default)")
                .arg(
                    Arg::with_name("IPFS_HASH")
                        .required(true)
                        .index(0)
                        .help("The hash to send a pin request for"),
                ),
        )
        .subcommand(
            SubCommand::with_name("listen")
                .about("Listen for pin requests and other events on a pinreq channel"),
        )
        .get_matches();

    print!("Username: ");
    io::stdout().flush()?;
    let mut username = String::new();
    let stdin = io::stdin();
    stdin.read_line(&mut username)?;
    username = username.trim().to_owned();
    let mut channel =
        MatrixChannel::new("default", "matrix.org", DEFAULT_PINREQ_MATRIX_ROOM_ALIAS)?;

    channel.log_in(&username, rpassword::prompt_password_stderr("Password: ")?)?;

    info!("Token is {:?}", channel.access_token);

    channel.check_room()?;

    let mut ctx = Context::from_protocol(Protocol::OpenPgp)?;

    let msg = Message::from_kind(MessageKind::Pin("/ipfs/QmHenlo".to_owned()), &mut ctx)?;

    info!("msg: {:#?}", msg);

    channel.send_msg(&msg)?;

    let fut = channel.listen()?.for_each(|msg| {
        println!("Got message: {:?}", msg);
        Ok(())
    });

    current_thread::block_on_all(fut)?;

    Ok(())
}
