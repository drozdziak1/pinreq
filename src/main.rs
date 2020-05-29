#[macro_use]
extern crate log;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

mod config;
mod matrix;
mod message;
mod req_channel;
mod utils;

use clap::{App, Arg, ArgMatches, SubCommand, Values};
use failure::Error;
use futures::{Stream, StreamExt};
use gpgme::{Context, Protocol};
use log::LevelFilter;

use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, Write},
};

use config::Config;
use matrix::MatrixChannel;
use message::{Message, MessageKind};
use req_channel::{ChannelSettings, ReqChannel};

static DEFAULT_PINREQ_MATRIX_ROOM_ALIAS: &'static str = "#ipfs-pinreq:matrix.org";

#[tokio::main]
async fn main() -> Result<(), Error> {
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
            Arg::with_name("CHANNEL_IDS")
                .help("A comma-separated list of unique channel IDs")
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
                        .index(1)
                        .help("The hash to send a pin request for"),
                ),
        )
        .subcommand(
            SubCommand::with_name("listen")
                .about("Listen for pin requests and other events on a pinreq channel"),
        )
        .get_matches();

    let cfg = Config::from_file(
        matches
            .value_of("CONFIG_FILE")
            .ok_or(format_err!("INTERNAL: Expected CONFIG_FILE to be Some()"))?,
    )?;

    let requested_channels = matches.values_of("CHANNEL_IDS");

    debug!("Config: {:#?}", cfg);

    let cfg_map = cfg.to_map()?;

    // We assume that when requested_channels is None -a/--all was specified
    let channel_names: Vec<_> = if let Some(chans) = requested_channels {
        // Check if all channels exist beforehand
        chans
            .map(|c| {
                if cfg_map.contains_key(c) {
                    Ok(c)
                } else {
                    Err(format_err!("Channel {} not found", c))
                }
            })
            .collect::<Result<Vec<_>, Error>>()?
    } else {
        cfg_map.keys().map(|s| s.as_str()).collect()
    };

    match matches.subcommand() {
        ("listen", Some(matches)) => {
            handle_listen(matches, &cfg_map, channel_names.as_slice()).await?;
        }
        ("request", Some(matches)) => {
            handle_request(matches, &cfg_map, channel_names.as_slice()).await?;
        }
        _other => unreachable!(),
    }

    Ok(())
}

async fn handle_listen(
    matches: &ArgMatches<'_>,
    cfg_map: &HashMap<String, Box<impl ChannelSettings>>,
    channels: &[&str],
) -> Result<(), Error> {
    unimplemented!();
}

async fn handle_request(
    matches: &ArgMatches<'_>,
    cfg_map: &HashMap<String, Box<impl ChannelSettings>>,
    channels: &[&str],
) -> Result<(), Error> {
    let ipfs_hash = matches
        .value_of("IPFS_HASH")
        .ok_or(format_err!("INTERNAL: expected IPFS_HASH to be specified"))?;

    info!("Pinning {}", ipfs_hash);

    for ch_name in channels {
        let channel = cfg_map
            .get(ch_name.to_owned())
            .ok_or(format_err!("INTERNAL: Channel {} not found", ch_name))?
            .to_channel()?;

        let mut ctx = Context::from_protocol(Protocol::OpenPgp)?;
        let msg = Message::from_kind(MessageKind::Pin(ipfs_hash.to_owned()), &mut ctx)?;

        debug!("[{}] sending msg: {:#?}", ch_name, msg);

        channel.as_ref().send_msg(&msg).await?;
    }
    Ok(())
}
