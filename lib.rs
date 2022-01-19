use std::{collections::HashMap, sync::Arc, net::SocketAddr};
use warp::{http::Response, hyper::Body};
use regex::Regex;
use sled::{Tree, Config};

type Map = HashMap<String, String>;

#[derive(serde::Serialize)]
pub struct Init {
    pub time: i64,
    pub url: String,
}

#[derive(serde::Serialize)]
pub struct Record {
    pub time: i64,
    pub key: String,
    pub raw_ip: Option<String>,
    pub x_raw_ip: Option<String>,
}

pub async fn get(url: &str) -> Option<String> {
    if url.starts_with("http") {
        let resp = reqwest::get(url).await.ok()?;
        match resp.status().as_u16() {
            200 => resp.text().await.ok(),
            _ => None,
        }
    } else {
        tokio::fs::read_to_string(url).await.ok()
    }
}

pub fn init(config: &str, map: &mut Map) -> Option<()> {
    // map.clear();
    let re = Regex::new("\\s+").unwrap();
    for line in config.lines().filter(|line| line.len() != 0) {
        let mut splited = re.split(line);
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

pub fn time() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn open_storage(path: String, url: String) -> Tree {
    let db = Config::new().path(path).open().unwrap();
    let init = Init {
        time: time(),
        url,
    };
    db.open_tree(serde_json::to_string(&init).unwrap().as_str()).unwrap()
}

pub async fn handle(
    key: String, raw_ip: Option<SocketAddr>, x_raw_ip: Option<String>, map: Arc<Map>, storage: Arc<Tree>
) -> Result<impl warp::Reply, warp::Rejection> {
    let result = map.get(&key);
    if let Some(_) = result {
        let time = time();
        let record = Record {
            time,
            key,
            raw_ip: raw_ip.map(|val| val.to_string()),
            x_raw_ip,
        };
        storage.insert(
            time.to_be_bytes(),
            serde_json::to_string(&record).unwrap().as_str()
        ).unwrap();
    }
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

    fn wrapped_init(config: &str) -> Option<Map> {
        let mut map = HashMap::new();
        init(config, &mut map)?;
        Some(map)
    }

    #[test]
    fn happypath() {
        let config = EXAMPLE_CONFIG;
        let map = wrapped_init(config).unwrap();
        assert_eq!(
            map.get("/rust"),
            Some("www.rust-lang.org/")
        );
        assert_eq!(
            map.get("/trpl-cn"),
            Some("kaisery.github.io/trpl-zh-cn/")
        );
    }

    #[test]
    fn config_redundant_value() {
        let config = "key val redundance\nkey val";
        assert_eq!(wrapped_init(config), None);
    }

    #[test]
    fn config_lack_value() {
        let config = "key val\nkey";
        assert_eq!(wrapped_init(config), None);
    }

    #[test]
    fn path_no_prefix() {
        let config = EXAMPLE_CONFIG;
        let map = wrapped_init(config).unwrap();
        assert_eq!(map.get("rust"), None);
    }
}
