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
use dialoguer::{Input, Password, Select};
use failure::Error;
use futures::prelude::*;
use gpgme::{Context, Protocol};
use log::LevelFilter;
use ruma_client::{identifiers::RoomAliasId, Client};
use url::Url;

use std::{
    collections::HashMap,
    convert::TryFrom,
    env,
    fs::File,
    io::{self, Write},
};

use crate::{
    config::Config,
    matrix::MatrixChannel,
    message::{Message, MessageKind},
    req_channel::{ChannelSettings, ReqChannel},
};

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

    let main_matches = App::new("pinreq")
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
                .required_unless_one(&["all", "gen-matrix"])
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
        .subcommand(SubCommand::with_name("gen-matrix").about("Generate a Matrix channel config"))
        .get_matches();

    match main_matches.subcommand() {
        ("listen", Some(matches)) => {
            let (channel_names, cfg_map) = load_config_map(&main_matches)?;
            handle_listen(matches, &cfg_map, channel_names.as_slice()).await?;
        }
        ("request", Some(matches)) => {
            let (channel_names, cfg_map) = load_config_map(&main_matches)?;
            handle_request(matches, &cfg_map, channel_names.as_slice()).await?;
        }
        ("gen-matrix", Some(matches)) => {
            handle_gen_matrix().await?;
        }
        _other => unreachable!(),
    }

    Ok(())
}

async fn handle_listen(
    matches: &ArgMatches<'_>,
    cfg_map: &HashMap<String, Box<impl ChannelSettings>>,
    channels: &[String],
) -> Result<(), Error> {
    for ch_name in channels {
        let mut channel = cfg_map
            .get(ch_name)
            .ok_or(format_err!("INTERNAL: Channel {} not found", ch_name))?
            .to_channel()?;

        // Process messages from channel
        channel
            .as_ref()
            .listen()
            .await?
            .try_for_each(|msgs| async move {
                info!("{}: Got {} new messages", ch_name, msgs.len());
                Ok(())
            })
            .await?;
    }
    Ok(())
}

async fn handle_request(
    matches: &ArgMatches<'_>,
    cfg_map: &HashMap<String, Box<impl ChannelSettings>>,
    channels: &[String],
) -> Result<(), Error> {
    let ipfs_hash = matches
        .value_of("IPFS_HASH")
        .ok_or(format_err!("INTERNAL: expected IPFS_HASH to be specified"))?;

    info!("Pinning {}", ipfs_hash);

    for ch_name in channels {
        let channel = cfg_map
            .get(ch_name)
            .ok_or(format_err!("INTERNAL: Channel {} not found", ch_name))?
            .to_channel()?;

        let mut ctx = Context::from_protocol(Protocol::OpenPgp)?;
        let msg = Message::from_kind(MessageKind::Pin(ipfs_hash.to_owned()), &mut ctx)?;

        debug!("[{}] sending msg: {:#?}", ch_name, msg);

        channel.as_ref().send_msg(&msg).await?;
    }
    Ok(())
}

async fn handle_gen_matrix() -> Result<(), Error> {
    let name = Input::<String>::new()
        .with_prompt("Human-readable channel name")
        .interact()?;

    let homeserver = Input::<Url>::new()
        .with_prompt("Homeserver URL")
        .default(Url::parse("https://matrix.org")?)
        .interact()?;

    let room_alias: RoomAliasId = RoomAliasId::try_from(
        Input::<String>::new()
            .with_prompt("Room alias")
            .default(DEFAULT_PINREQ_MATRIX_ROOM_ALIAS.parse()?)
            .interact()?,
    )?;

    let initial_backlog_size: u32 = Input::<u32>::new()
        .with_prompt("Initial backlog size")
        .default(100)
        .interact()?;

    let username = Input::<String>::new().with_prompt("Username").interact()?;

    let mut channel = MatrixChannel::new(&name, homeserver, room_alias, initial_backlog_size)?;

    {
        let pass = Password::new().with_prompt("Password").interact()?;
        channel.log_in(&username, pass).await?;
    }

    let cfg = Config {
        matrix: vec![channel.settings],
    };

    info!("Created room:\n{}", toml::to_string(&cfg)?);

    Ok(())
}

/// Returns (selected channels, available channels hashmap)
fn load_config_map(
    matches: &ArgMatches<'_>,
) -> Result<(Vec<String>, HashMap<String, Box<impl ChannelSettings>>), Error> {
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
                    Ok(c.to_owned())
                } else {
                    Err(format_err!("Channel {} not found", c))
                }
            })
            .collect::<Result<Vec<_>, Error>>()?
    } else {
        cfg_map.keys().cloned().collect()
    };

    return Ok((channel_names, cfg_map));
}
