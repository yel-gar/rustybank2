use clap::Parser;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Parser)]
pub struct Args {
    #[clap(long, default_value_t = 8080)]
    pub port: u16,

    #[clap(long, default_value = "0.0.0.0")]
    pub host: IpAddr,

    #[clap(long)]
    pub disable_ip_limit: bool,

    #[clap(long, default_value = "./db.sqlite", env = "DB_FILE")]
    pub db_file: PathBuf,
}
