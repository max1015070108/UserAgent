
use clap;
use std::env;


use UserAgent::communication::aws_utils;
use UserAgent::database::mysql_utils;

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
        .subcommand(
            clap::command!("aws_create_template").arg(
                clap::arg!(--"file_path" <PATH>)
                    .value_parser(clap::value_parser!(std::path::PathBuf)),
            )
        ).subcommand(
            clap::command!("database").arg(
                clap::arg!(--"db_url" <PATH>)
                    .value_parser(clap::value_parser!(std::path::PathBuf)),
            )
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
        },
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
        },

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
