#![feature(try_from)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate failure;

use clap::{App, Arg, ArgMatches, SubCommand};
use failure::Error;
use futures::future::Future;
use gpgme::{Context, Data, Protocol, SignMode};
use hyper::client::connect::Connect;
use ipfs_api::IpfsClient;
use log::LevelFilter;
use ruma_client::{
    api::r0::{membership::join_room_by_id_or_alias, send::send_message_event},
    Client,
};
use ruma_events::{
    room::message::{MessageEventContent, MessageType, TextMessageEventContent},
    EventType,
};
use tokio::runtime::current_thread;

use std::{
    convert::TryInto,
    env,
    fmt::Debug,
    io::{self, Write},
    process,
};

static DEFAULT_PINREQ_MATRIX_ROOM_ALIAS: &'static str = "ipfs-pinreq";

pub fn main() {
    // Set logging up
    match env::var("RUST_LOG") {
        Ok(_value) => env_logger::init(),
        Err(_e) => env_logger::Builder::new()
            .filter_level(LevelFilter::Info)
            .init(),
    }

    let common_args = &[
        Arg::with_name("homeserver")
            .short("s")
            .long("server")
            .value_name("HOMESERVER")
            .help("The Matrix homeserver to use; has to be HTTPS")
            .default_value("matrix.org"),
        Arg::with_name("room_alias")
            .help("The Matrix room to use; has to exist and be public")
            .short("r")
            .long("room")
            .value_name("ROOM_ALIAS")
            .default_value(DEFAULT_PINREQ_MATRIX_ROOM_ALIAS),
    ];

    let mut app = App::new("pinreq-matrix")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Request a pin for an IPFS hash")
        .args(common_args)
        .subcommand(
            SubCommand::with_name("request")
                .about("Request pinning of the specified hash")
                .arg(
                    Arg::with_name("ipfs_hash")
                        .help("The IPFS/IPNS hash to pin-request")
                        .required(true)
                        .index(1),
                ),
        )
        .subcommand(SubCommand::with_name("listen").about("Listen for pin requests"));

    let cli_matches = app.clone().get_matches();

    if let ("", _empty_matches) = cli_matches.subcommand() {
        error!("No subcommand specified");
        app.print_help().unwrap();
        process::exit(1);
    }

    // Safe cause there's a default
    let homeserver = cli_matches.value_of("homeserver").unwrap();
    let room_alias = cli_matches.value_of("room_alias").unwrap();

    // Get the username
    let mut user = String::new();
    print!("User: ");
    io::stdout().flush().unwrap();
    io::stdin()
        .read_line(&mut user)
        .expect("Could not prompt username");
    user = user.replace('\n', "");

    // Get the password sudo-style (with echo off)
    let pass = rpassword::prompt_password_stdout("Password: ").unwrap();

    let mut matrix = Client::new_https(format!("https://{}", homeserver).parse().unwrap()).unwrap();

    matrix.log_in(&user, pass, None).unwrap();

    match cli_matches.subcommand() {
        ("request", Some(matches)) => {
            let response = handle_request(matches, &matrix, room_alias, homeserver).unwrap();
            info!("Pin request sent successfully");
            debug!("Full event ID: {:#?}", response.event_id);
        }
        ("listen", Some(matches)) => {
            let ipfs = IpfsClient::default();

            let fut = ipfs
                .stats_repo()
                .map(|stats| debug!("Stats:\n{:#?}", stats));

            current_thread::block_on_all(fut).unwrap_or_else(|e| {
                error!("Could not contact the IPFS daemon. Are you sure `ipfs daemon --enable-pubsub-experiment` was run?");
                debug!("Underlying error: {:?}", e);
                process::exit(1);
            });
            handle_listen(matches, &matrix, room_alias, homeserver, &ipfs).unwrap();
        }
        (_other_cmd, _other_matches) => unreachable!(),
    }
}

pub fn handle_request<C: Connect + Debug + 'static>(
    matches: &ArgMatches,
    matrix: &Client<C>,
    room_alias: &str,
    homeserver: &str,
) -> Result<send_message_event::Response, Error> {
    debug!("Logged in:\n{:#?}", matrix);

    debug!("Joining: #{}:{}", room_alias, homeserver);
    let request_fut = join_room_by_id_or_alias::call(
        matrix,
        join_room_by_id_or_alias::Request {
            room_id_or_alias: format!("#{}:{}", room_alias, homeserver)
                .as_str()
                .try_into()
                .unwrap(),
            third_party_signed: None,
        },
    )
    .map(|res| {
        debug!("Room ID joined: {:?}", res.room_id);
        res.room_id
    })
    .and_then(|room_id| {
        send_message_event::call(
            matrix,
            send_message_event::Request {
                room_id: room_id,
                event_type: EventType::RoomMessage,
                txn_id: "1".to_owned(),
                data: MessageEventContent::Text(TextMessageEventContent {
                    body: "Halko".to_owned(),
                    msgtype: MessageType::Text,
                }),
            },
        )
    });

    current_thread::block_on_all(request_fut).map_err(|e| e.into())
}

pub fn handle_listen<C: Connect + Debug + 'static>(
    matches: &ArgMatches,
    matrix: &Client<C>,
    room_alias: &str,
    homeserver: &str,
    ipfs: &IpfsClient,
) -> Result<(), Error> {
    Ok(())
}
