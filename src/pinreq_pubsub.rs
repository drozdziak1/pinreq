#[macro_use]
extern crate log;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;

mod message;

use base58::ToBase58;
use clap::{App, Arg, SubCommand};
use failure::Error;
use futures::{
    future::{self, Future},
    stream::Stream,
};
use gpgme::{Context, Data, Protocol, SignMode};
use ipfs_api::IpfsClient;
use log::LevelFilter;
use tokio::runtime::current_thread;

use std::{env, process};

use crate::message::{Message, MessageKind};

const DEFAULT_PINREQ_PUBSUB_TOPIC: &'static str = "pinreq";

pub fn main() -> Result<(), Error> {
    // Set logging up
    match env::var("RUST_LOG") {
        Ok(_value) => env_logger::init(),
        Err(_e) => env_logger::Builder::new()
            .filter_level(LevelFilter::Info)
            .init(),
    }

    let cli_matches = App::new("pinreq-pubsub")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand(
            SubCommand::with_name("request")
                .about("Request a pin for an IPFS hash")
                .arg(
                    Arg::with_name("ipfs_hash")
                        .help("The IPFS/IPNS hash to pin-request")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::with_name("topic")
                        .help("The topic to send the request to")
                        .short("t")
                        .long("topic")
                        .value_name("TOPIC")
                        .default_value(DEFAULT_PINREQ_PUBSUB_TOPIC),
                ),
        )
        .subcommand(
            SubCommand::with_name("listen")
                .about("Listen on pubsub for pin requests")
                .arg(
                    Arg::with_name("topic")
                        .help("The topic to listen on")
                        .short("t")
                        .long("topic")
                        .value_name("TOPIC")
                        .default_value(DEFAULT_PINREQ_PUBSUB_TOPIC)
                        .required(false)
                        .index(1),
                ),
        )
        .get_matches();

    let ipfs = IpfsClient::default();

    match current_thread::block_on_all(ipfs.stats_repo()) {
        Ok(stats) => debug!("Repo stats: {:#?}", stats),
        Err(_e) => {
            error!("Could not connect to IPFS. Are you sure `ipfs daemon` is running?");
            process::exit(1);
        }
    }

    // Create a GPGME context
    let mut ctx = Context::from_protocol(Protocol::OpenPgp).unwrap();
    ctx.set_armor(true);

    match cli_matches.subcommand() {
        ("request", Some(matches)) => {
            let ipfs_hash = matches.value_of("ipfs_hash").unwrap();
            let req = ipfs.pubsub_peers(Some(DEFAULT_PINREQ_PUBSUB_TOPIC));

            let response = current_thread::block_on_all(req).unwrap_or_else(|e| {
                error!("Could not get peers: {}", e.to_string());
                process::exit(1);
            });
            let peers = response.strings;
            debug!("Peer list: {:#?}", peers);
            info!("Requesting a pin for {} ({} peers)", ipfs_hash, peers.len());

            let buffer =
                serde_json::to_string(&Message::from_kind(MessageKind::Pin(ipfs_hash.to_owned()), &mut ctx)?)?;
            let buffer_data = Data::from_buffer(&buffer).unwrap();

            let mut signed_data: Vec<u8> = Vec::new();

            let sign_result = match ctx.sign(SignMode::Normal, buffer_data, &mut signed_data) {
                Ok(res) => res,
                Err(e) => {
                    error!("Could not sign the request: {}", e.description());
                    process::exit(1);
                }
            };

            if sign_result.invalid_signers().count() > 0 {
                error!("Got non-empty invalid signers");
            }

            for (idx, signature) in sign_result.new_signatures().enumerate() {
                trace!("Signature {}: {:#?}", idx, signature);
            }

            let signed_data_string = String::from_utf8(signed_data).unwrap();
            debug!("Signed data:\n{}", signed_data_string);

            let pub_req = ipfs.pubsub_pub(matches.value_of("topic").unwrap(), &signed_data_string);
            current_thread::block_on_all(pub_req).unwrap_or_else(|e| {
                error!("Could not send the publish request: {}", e.to_string());
                process::exit(1);
            });
            info!("Done.");
        }
        ("listen", Some(matches)) => {
            let sub_req = ipfs
                .pubsub_sub(matches.value_of("topic").unwrap(), true)
                .for_each(|item| {
                    trace!("Got response:\n{:#?}", item);

                    let mut data_raw = base64::decode(&item.data.unwrap_or(String::new())).unwrap();

                    let from = base64::decode(&item.from.unwrap_or(String::new()))
                        .unwrap()
                        .to_base58();

                    debug!(
                        "Decoded message from {}:\n{}",
                        from,
                        String::from_utf8(data_raw.clone()).unwrap_or("<binary blob>".to_owned())
                    );

                    let mut decrypted_raw = Vec::<u8>::new();

                    match ctx.verify_opaque(&mut data_raw, &mut decrypted_raw) {
                        Ok(_res) => {
                            let msg: Message = match serde_json::from_slice(&decrypted_raw) {
                                Ok(m) => m,
                                Err(e) => {
                                    error!("Could not parse decrypted content: {}", e.to_string());
                                    return future::ok(());
                                }
                            };
                            debug!("Successfully decrypted message: {:?}", msg);
                            match msg.kind {
                                MessageKind::Pin(address) => {
                                    info!("Pinning address {}", address);
                                    let req = ipfs.pin_add(&address, true).then(|result| {
                                        match result {
                                            Ok(success) => {
                                                info!("Pinned it! ({:?})", success);
                                            }
                                            Err(e) => {
                                                error!("Could not pin it :( ({:?})", e);
                                            }
                                        }
                                        future::ok(())
                                    });

                                    tokio::spawn(req);
                                }
                                MessageKind::Confirm(address) => {
                                    debug!("Ignoring confirmation for {}", address)
                                }
                            }
                        }
                        Err(e) => warn!("Could not decrypt at this time: {}", e.description()),
                    }
                    future::ok(())
                });

            current_thread::block_on_all(sub_req).unwrap_or_else(|e| {
                error!("Could not process the subscribe request: {}", e.to_string());
                process::exit(1);
            });
            info!("Done.");
        }
        _ => error!("Unknown command"),
    }
    Ok(())
}
