use std::{collections::HashMap, sync::Arc, net::SocketAddr, io::Write};
use tokio::{spawn, sync::mpsc::{unbounded_channel, UnboundedSender as Sender}};
use warp::{http::Response, hyper::Body};

type Map = HashMap<String, String>;

pub fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis().try_into().unwrap()
}

pub fn build_client() -> hyper::Client<hyper_rustls::HttpsConnector<hyper::client::connect::HttpConnector>> {
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_only()
        .enable_http1()
        .build();
    hyper::Client::builder().build(connector)
}

#[derive(serde::Serialize)]
pub struct Init {
    pub time: u64,
    pub url: String,
}

#[derive(serde::Serialize, Debug)]
pub struct Record {
    pub time: u64,
    pub key: String,
    pub hit: bool,
    pub ip: Option<String>,
    pub xff: Option<Vec<String>>,
}

pub async fn get(url: &str) -> Option<String> {
    if url.starts_with("http") {
        let client = build_client();
        let resp = client.get(url.parse().ok()?).await.ok()?;
        if resp.status().as_u16() == 200 {
            let bytes = hyper::body::to_bytes(resp.into_body()).await.ok()?;
            String::from_utf8(bytes.as_ref().to_vec()).ok()
        } else {
            None
        }
    } else {
        tokio::fs::read_to_string(url).await.ok()
    }
}

pub fn init(config: &str, map: &mut Map) -> Option<()> {
    // map.clear();
    for line in config.lines().filter(|s| s.len() != 0) {
        let mut splited = line.split(' ').filter(|s| s.len() != 0);
        let key = splited.next()?.to_owned();
        let val = splited.next()?;
        let val = if val.starts_with("http://") {
            val.to_owned()
        } else {
            format!("https://{}", val)
        };
        if let Some(_) = splited.next() { return None };
        map.insert(key, val);
    }
    Some(())
}

pub async fn handle(
    key: String, ip: Option<SocketAddr>, xff: Option<String>, map: Arc<Map>, log_sender: Sender<Record>
) -> Result<impl warp::Reply, warp::Rejection> {
    let result = map.get(&key);
    log_sender.send(Record {
        time: now(),
        key,
        hit: result.is_some(),
        ip: ip.map(|val| val.to_string()),
        xff: xff.map(|val| val.split(',').map(|s| s.to_owned()).collect()),
    }).unwrap();
    Ok(match result {
        None => Response::builder().status(404).body(Body::empty()).unwrap(),
        Some(val) => Response::builder().status(307).header("Location", val).body(Body::empty()).unwrap(),
    })
}

pub fn log_thread<W: Write + 'static + Send>(url: String, mut writer: W) -> Sender<Record> {
    let (tx, mut rx) = unbounded_channel::<Record>();
    spawn(async move {
        let init = Init {
            time: now(),
            url,
        };
        serde_json::to_writer(&mut writer, &init).unwrap();
        writeln!(writer).unwrap();
        while let Some(record) = rx.recv().await {
            serde_json::to_writer(&mut writer, &record).unwrap();
            writeln!(writer).unwrap();
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE_CONFIG: &str = r#"
rust    www.rust-lang.org/

trpl    doc.rust-lang.org/stable/book/
trpl-cn kaisery.github.io/trpl-zh-cn/
"#;

    fn wrapped_init(config: &str) -> Option<Map> {
        let mut map = HashMap::new();
        init(config, &mut map)?;
        Some(map)
    }

    #[test]
    fn happypath() {
        let map = wrapped_init(EXAMPLE_CONFIG).unwrap();
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
        matches!(wrapped_init(config), None);
    }

    #[test]
    fn config_lack_value() {
        let config = "key val\nkey";
        matches!(wrapped_init(config), None);
    }
}
