use structopt::StructOpt;
use reqwest::blocking::get;
use http_redirector::{parse_map};

#[derive(StructOpt, Debug)]
#[structopt(name = "http-redirector")]
struct Args {
    #[structopt(short = "p", long, default_value = "8080")]
    port: u16,
    #[structopt(short = "c", long)]
    config: String,
}

fn main() {
    let args = Args::from_args();
    let config = get(args.config).unwrap().text().unwrap();
    let config = parse_map(config).unwrap();
    println!("port = {}", args.port);
    for (key, val) in config {
        println!("{} => {}", key, val);
    }
}
