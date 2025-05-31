use clap;
use std::env;

use UserAgent::aimodels::deepseek;
use UserAgent::communication::aws_utils;
use UserAgent::database::mysql_utils;
use UserAgent::trading::longportmaster;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = "HOME";
    match env::var(key) {
        Ok(val) => println!("{}: {:?}", key, val),
        Err(_e) => println!("Couldn't interpret {}: {}", key, _e),
    }

    let cmd = clap::Command::new("agentcli")
        .bin_name("agentcli")
        .styles(CLAP_STYLING)
        .subcommand_required(true)
        .subcommand(clap::command!("aws_create_template").arg(
            clap::arg!(--"file_path" <PATH>).value_parser(clap::value_parser!(std::path::PathBuf)),
        ))
        .subcommand(clap::command!("database").arg(
            clap::arg!(--"db_url" <PATH>).value_parser(clap::value_parser!(std::path::PathBuf)),
        ))
        .subcommand(
            clap::command!("aimodels")
                .arg(clap::arg!(--"message" <String>).value_parser(clap::value_parser!(String))),
        )
        .subcommand(
            clap::command!("trading")
                .arg(clap::arg!(--"pairs" <String>).value_parser(clap::value_parser!(String))),
        );
    let matches = cmd.get_matches();
    let matches = match matches.subcommand() {
        Some(("aws_create_template", matches)) => {
            // This is where you would write business logic for
            // the detected subcommand "aws_create_template". For example:
            //
            println!("aws_create_template command detected!");

            let manifest_path = matches.get_one::<std::path::PathBuf>("file_path");

            let mut Eman = aws_utils::EmailManager::new().await;
            Eman.file_path = manifest_path.map(|p| p.to_path_buf()).unwrap_or_default();
            Eman.createTemplate().await;

            matches
        }
        Some(("database", matches)) => {
            // This is where you would write business logic for
            // the detected subcommand "aws_create_template". For example:
            //

            let manifest_path = matches.get_one::<std::path::PathBuf>("db_url");

            println!("database command detected!");
            // let mut Eman = aws_utils::EmailManager::new().await;
            // Eman.file_path = manifest_path.map(|p| p.to_path_buf()).unwrap_or_default();
            // Eman.createTemplate().await;

            matches
        }

        Some(("aimodels", matches)) => {
            //println!("admodels command detected!");
            // Call deepseek with input message
            let input_message = matches.get_one::<String>("message");
            if let Some(msg) = input_message {
                deepseek::deepseekai(msg).await.unwrap();
            } else {
                println!("No input message provided for deepseek.");
            }

            matches
        }

        Some(("trading", matches)) => {
            let pairs_name = matches.get_one::<String>("pairs");
            let methods = matches.get_one::<String>("method");
            let mut lp = longportmaster::LongPort::new().await.unwrap();

            // if let Some(pair) = pairs_name {
            //     lp.quote_pairs_once(pair).await.unwrap();
            // } else if let Some(method) = methods {
            //     lp.quote_pairs_continue(symbol)
            // } else {
            //     println!("No input pair provided for trading.");
            // }
            match (pairs_name, methods.map(|s| s.as_str())) {
                (Some(pair), Some("once")) => {
                    lp.quote_pairs_once(pair).await.unwrap();
                }
                (Some(pair), Some("continue")) => {
                    lp.quote_pairs_continue(pair).await.unwrap();
                }
                (Some(_), Some(other)) => {
                    println!("Unknown method '{}'. Use 'once' or 'continue'.", other);
                }
                (None, _) => {
                    println!("No input pair provided for trading.");
                }
                (_, None) => {
                    println!("No method provided. Use --method once|continue.");
                }
            }

            matches
        }

        _ => unreachable!("clap should ensure we don't get here"),
    };
    // let manifest_path = matches.get_one::<std::path::PathBuf>("file_path");
    // println!("here right -- {manifest_path:?}");

    Ok(())
}

// See also `clap_cargo::style::CLAP_STYLING`
pub const CLAP_STYLING: clap::builder::styling::Styles = clap::builder::styling::Styles::styled()
    .header(clap_cargo::style::HEADER)
    .usage(clap_cargo::style::USAGE)
    .literal(clap_cargo::style::LITERAL)
    .placeholder(clap_cargo::style::PLACEHOLDER)
    .error(clap_cargo::style::ERROR)
    .valid(clap_cargo::style::VALID)
    .invalid(clap_cargo::style::INVALID);
