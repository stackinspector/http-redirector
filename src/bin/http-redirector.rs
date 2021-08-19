use structopt::StructOpt;
use reqwest::blocking::get;
use tiny_http::{Header, Response, Server};
use http_redirector::{init, lookup};

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
    let map = init(config).unwrap();
    let server = Server::http(format!("0.0.0.0:{}", args.port)).unwrap();

    for request in server.incoming_requests() {
        let response = match lookup(request.url(), &map) {
            None => Response::empty(404),
            Some(result) => Response::empty(301)
                .with_header(format!("Location: {}", result).parse::<Header>().unwrap()),
        };
        request.respond(response).unwrap();
    }
}
