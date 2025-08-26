mod llama_parse_backend;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // Path to the config file. Defaults to ~/.parse_config.json
    #[clap(short, long)]
    parse_config: Option<String>,

    // The backend type to use for parsing. Defaults to `llama-parse`
    #[clap(short, long)]
    backend: Option<String>,
}

fn main() -> Result<()> {
    println!("Hello, world!");

    Ok(())
}
