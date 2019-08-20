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
use futures::Stream;
use gpgme::{Context, Protocol};
use log::LevelFilter;
use tokio::runtime::current_thread;

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
            handle_listen(matches, &cfg_map, channel_names.as_slice())?;
        }
        ("request", Some(matches)) => {
            handle_request(matches, &cfg_map, channel_names.as_slice())?;
        }
        _other => unreachable!(),
    }

    /*
     *    print!("Username: ");
     *    io::stdout().flush()?;
     *    let mut username = String::new();
     *    let stdin = io::stdin();
     *    stdin.read_line(&mut username)?;
     *    username = username.trim().to_owned();
     *    let mut channel =
     *        MatrixChannel::new("default", "matrix.org", DEFAULT_PINREQ_MATRIX_ROOM_ALIAS)?;
     *
     *    channel.log_in(&username, rpassword::prompt_password_stderr("Password: ")?)?;
     */
    /*    // The ReqChannel type doesn't have listen() yet
     *    let fut = channel.listen()?.for_each(|msg| {
     *        println!("Got message: {:?}", msg);
     *        Ok(())
     *    });
     *
     *    current_thread::block_on_all(fut)?;
     */

    Ok(())
}

fn handle_listen(
    matches: &ArgMatches,
    cfg_map: &HashMap<String, Box<impl ChannelSettings>>,
    channels: &[&str],
) -> Result<(), Error> {
    info!("Listening on channels: {:?}", channels);

    let ch_name = channels[0];

    let channel = cfg_map
        .get(ch_name)
        .ok_or(format_err!("INTERNAL: Channel {} not found", ch_name))?
        .to_channel()?;

    /// Process the parsed messages
    let fut = channel.as_ref().listen()?.for_each(|msgs| {
        let mut ctx = Context::from_protocol(Protocol::OpenPgp)?;

        if msgs.len() == 0 {
            debug!("No messages on this run");
            return Ok(());
        }

        for msg in msgs {
            // Verify the signature
            let verif_res =
                ctx.verify_detached(msg.signature.as_bytes(), serde_json::to_vec(&msg.kind)?)?;

            for sig in verif_res.signatures() {
                match sig.status() {
                    Ok(_) => debug!("Message signature OK"),
                    Err(e) => {
                        warn!("Message signature invalid: {}", e.to_string());
                        continue;
                    }
                }

                // TODO: Check for presence of required fingerprints
            }

            // TODO: Parse and pin the requested hash
        }
        Ok(())
    });

    current_thread::block_on_all(fut)?;

    Ok(())
}

fn handle_request(
    matches: &ArgMatches,
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

        info!("msg: {:#?}", msg);

        channel.as_ref().send_msg(&msg)?;
    }
    Ok(())
}
