use std::{collections::HashMap, io::{self, Write}, fs, path::PathBuf, net::SocketAddr, sync::Arc};
use tokio::{spawn, sync::mpsc::{unbounded_channel, UnboundedSender}};
use warp::{http::Response, hyper::Body};

fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis().try_into().unwrap()
}

fn split_kv<'a, I: Iterator<Item = &'a str>>(mut iter: I) -> Option<(&'a str, &'a str)> {
    let key = iter.next()?;
    let val = iter.next()?;
    if let Some(_) = iter.next() { return None };
    Some((key, val))
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct Zone {
    pub url: String,
    pub map: HashMap<String, String>,
}

pub type State = HashMap<String, Zone>;
pub type WrappedState = Arc<State>;

#[derive(serde::Serialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    Init {
        ver: String,
        state: State,
    },
    Get {
        from: Vec<String>,
        zone: String,
        key: String,
        hit: bool,
    },
    Update {
        from: Vec<String>,
        state: State,
    },
}

#[derive(serde::Serialize)]
struct WrappedEvent {
    time: u64,
    event: Event,
}

type HttpClient = hyper::Client<hyper_rustls::HttpsConnector<hyper::client::connect::HttpConnector>>;

fn build_http_client() -> HttpClient {
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_only()
        .enable_http1()
        .build();
    hyper::Client::builder().build(connector)
}

pub async fn get(url: &str) -> anyhow::Result<String> {
    if url.starts_with("http") {
        let client = build_http_client();
        let resp = client.get(url.parse()?).await?;
        if resp.status().as_u16() == 200 {
            let bytes = hyper::body::to_bytes(resp.into_body()).await?;
            Ok(String::from_utf8(bytes.as_ref().to_vec())?)
        } else {
            Err(anyhow::anyhow!("http request returns non 200 response"))
        }
    } else {
        Ok(tokio::fs::read_to_string(url).await?)
    }
}

pub type LogSender = UnboundedSender<Event>;

pub fn log_thread<'a, W: Write + 'static + Send>(mut writer: W) -> LogSender {
    let (tx, mut rx) = unbounded_channel::<Event>();
    spawn(async move {
        while let Some(event) = rx.recv().await {
            serde_json::to_writer(&mut writer, &WrappedEvent { time: now(), event }).unwrap();
            writeln!(writer).unwrap();
        }
    });
    tx
}

fn init_map(config: &str) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();
    for line in config.lines().filter(|s| s.len() != 0) {
        let (key, val) = split_kv(line.split(' ').filter(|s| s.len() != 0))?;
        let val = if val.starts_with("http://") {
            val.to_owned()
        } else {
            format!("https://{}", val)
        };
        map.insert(key.to_owned(), val);
    }
    Some(map)
}

pub async fn init(input: String, log_path: Option<PathBuf>) -> anyhow::Result<(WrappedState, LogSender)> {
    let mut state = HashMap::new();
    for pair in input.split(';') {
        let (zone_name, url) = split_kv(pair.split(',')).ok_or_else(|| anyhow::anyhow!("parsing input error"))?;
        if zone_name == "__update__" {
            return Err(anyhow::anyhow!("zone name should not be \"__update__\""))
        }
        let config = get(url).await?;
        let map = init_map(config.as_str()).ok_or_else(|| anyhow::anyhow!("parsing config {} error", url))?;
        state.insert(zone_name.to_owned(), Zone { url: url.to_owned(), map });
    }
    let log_sender = match log_path {
        Some(path) => log_thread(fs::OpenOptions::new().write(true).create(true).append(true).open(path)?),
        None => log_thread(io::stdout()),
    };
    log_sender.send(Event::Init {
        ver: env!("CARGO_PKG_VERSION").to_owned(),
        state: state.clone(),
    })?;
    Ok((Arc::new(state), log_sender))
}

pub fn handle_ip(ip: Option<SocketAddr>, xff: Option<String>) -> Vec<String> {
    let mut from = Vec::new();
    if let Some(xff) = xff {
        for _ip in xff.split(',') {
            from.push(_ip.to_owned())
        }
    };
    if let Some(ip) = ip {
        from.push(ip.to_string())
    };
    from
}

pub async fn handle(
    zone: String, key: String, ip: Option<SocketAddr>, xff: Option<String>, state: WrappedState, log_sender: LogSender
) -> Result<impl warp::Reply, warp::Rejection> {
    let result = state.get(&zone).and_then(|zone| zone.map.get(&key));
    let from = handle_ip(ip, xff);
    let hit = result.is_some();
    log_sender.send(Event::Get { from, zone, key, hit }).unwrap();
    Ok(match result {
        None => Response::builder().status(404).body(Body::empty()).unwrap(),
        Some(val) => Response::builder().status(307).header("Location", val).body(Body::empty()).unwrap(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE_CONFIG: &str = r#"
rust    www.rust-lang.org/

trpl    doc.rust-lang.org/stable/book/
trpl-cn kaisery.github.io/trpl-zh-cn/
"#;

    #[test]
    fn happypath() {
        let map = init_map(EXAMPLE_CONFIG).unwrap();
        assert_eq!(
            map.get("rust").unwrap().as_str(),
            "https://www.rust-lang.org/"
        );
        assert_eq!(
            map.get("trpl-cn").unwrap().as_str(),
            "https://kaisery.github.io/trpl-zh-cn/"
        );
    }

    #[test]
    fn config_redundant_value() {
        let config = "key val redundance\nkey val";
        matches!(init_map(config), None);
    }

    #[test]
    fn config_lack_value() {
        let config = "key val\nkey";
        matches!(init_map(config), None);
    }
}
