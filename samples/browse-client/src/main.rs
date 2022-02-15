// OPCUA for Rust
// SPDX-License-Identifier: MPL-2.0
// Copyright (C) 2017-2022 Adam Lock

//! This simple OPC UA client will do the following:
//!
//! 1. Create a client configuration
//! 2. Connect to an endpoint specified by the url with security None
//! 3. Subscribe to values and loop forever printing out their values
use std::sync::{Arc, RwLock};

use opcua_client::prelude::*;

struct Args {
    help: bool,
    url: String,
}

impl Args {
    pub fn parse_args() -> Result<Args, Box<dyn std::error::Error>> {
        let mut args = pico_args::Arguments::from_env();
        Ok(Args {
            help: args.contains(["-h", "--help"]),
            url: args
                .opt_value_from_str("--url")?
                .unwrap_or_else(|| String::from(DEFAULT_URL)),
        })
    }

    pub fn usage() {
        println!(
            r#"Simple Client
Usage:
  -h, --help   Show help
  --url [url]  Url to connect to (default: {})"#,
            DEFAULT_URL
        );
    }
}

const DEFAULT_URL: &str = "opc.tcp://localhost:4855";

fn main() -> Result<(), ()> {
    // Read command line arguments
    let args = Args::parse_args().map_err(|_| Args::usage())?;
    if args.help {
        Args::usage();
        return Ok(());
    }

    // Optional - enable OPC UA logging
    opcua_console_logging::init();

    println!("Creating client");
    // Make the client configuration
    let mut client = ClientBuilder::new()
        .application_name("Browse Client")
        .application_uri("urn:BrowseClient")
        .product_uri("urn:BrowseClient")
        .trust_server_certs(true)
        .create_sample_keypair(true)
        .session_retry_limit(3)
        .session_timeout(1000)
        .client()
        .unwrap();

    println!("Connecting to endpoint {}", args.url);
    let session = client.connect_to_endpoint(
        (
            args.url.as_ref(),
            SecurityPolicy::None.to_str(),
            MessageSecurityMode::None,
            UserTokenPolicy::anonymous(),
        ),
        IdentityToken::Anonymous,
    ).unwrap();
    println!("Connected");

    let root_id = ObjectId::RootFolder.into();
    let root = BrowseDescription {
        node_id: root_id,
        browse_direction: BrowseDirection::Forward,
        reference_type_id: ReferenceTypeId::References.into(),
        include_subtypes: true,
        node_class_mask: 0b00000011,
        result_mask: 0b111111
    };

    let mut stack = vec![root];

    while !stack.is_empty() {
        let mut reader = session.write().unwrap();
        let (maybe_reads, new_stack): (Vec<_>, Vec<_>) = stack
            .chunks(10)
            .flat_map(|chunk| reader.browse(chunk).map_err(|err| { println!("{}", err); err }))
            .flatten()
            .flatten()
            .flat_map(|result| {
                let cp = result.continuation_point;
                println!(">>> Next result | CONTINUATION POINT {:?}", cp);
                result.references.into_iter().flatten().map(|reference| {
                    let node_id = reference.node_id.node_id.clone();
                    let name = reference.display_name.text;

                    let is_likely_system_variable =
                        if let Identifier::String(ref id) = node_id.identifier {
                            if let Some(s) = id.value() {
                                s.starts_with('_') || s.contains("._")
                            } else {
                                false
                            }
                        } else {
                            true
                        };

                    let should_read = reference.node_class == NodeClass::Variable;
                    if should_read {
                        println!(">>> Node: {} ({})", name, node_id);
                    }

                    let read = if should_read {
                        let read = ReadValueId {
                            node_id: node_id.clone(),
                            attribute_id: AttributeId::Value as u32,
                            index_range: UAString::null(),
                            data_encoding: QualifiedName::null(),
                        };
                        Some((read, name))
                    } else {
                        None
                    };

                    let browse = BrowseDescription {
                        node_id,
                        browse_direction: BrowseDirection::Forward,
                        reference_type_id: ReferenceTypeId::References.into(),
                        include_subtypes: true,
                        node_class_mask: 0b00000011,
                        result_mask: 0b111111,
                    };

                    (read, browse)
                })
            })
            .unzip();

                let (meta, reads): (Vec<_>, Vec<_>) = maybe_reads
            .into_iter()
            .flatten()
            .map(|(r, name)| ((name, r.node_id.clone()), r))
            .unzip();

        let mut new_values: Vec<_> = reads
            .chunks(10)
            .flat_map(|chunk| reader.read(chunk).map_err(|err| println!("ERR {}", err)))
            .flatten()
            .zip(meta)
            .collect();

        //println!(">>> Values {:?}", new_values);
        stack = new_stack;
    }

    Ok(())
}

