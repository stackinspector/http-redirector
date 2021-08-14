use structopt::StructOpt;
use reqwest::blocking::get;
use tiny_http::{Header, Response, Server};
use http_redirector::init;

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
    let lookup = init(config).unwrap();
    let server = Server::http(format!("0.0.0.0:{}", args.port)).unwrap();

    for request in server.incoming_requests() {
        let result = lookup(request.url()).unwrap();
        let header = format!("Location: {}", result).parse::<Header>().unwrap();
        let response = Response::empty(301).with_header(header);
        request.respond(response).unwrap();
    }
}
