use std::{collections::HashMap, sync::Arc};
use warp::{http::Response, hyper::Body};
use regex::Regex;

type Map = HashMap<String, String>;

pub async fn get(url: &str) -> Option<String> {
    let req = reqwest::get(url).await.ok()?;
    if req.status() != 200 { return None };
    Some(req.text().await.ok()?)
}

pub fn init(config: String, map: &mut Map) -> Option<()> {
    // map.clear();
    let re = Regex::new("\\s+").unwrap();
    for line in config.split("\n").filter(|line| line.len() != 0) {
        let mut splited = re.split(line);
        let key = splited.next()?.to_string();
        let val = splited.next()?.to_string();
        if let Some(_) = splited.next() { return None };
        map.insert(key, val);
    }
    Some(())
}

pub fn lookup(key: String, map: &Map) -> Option<String> {
    match map.get(&key) {
        None => None,
        Some(val) => Some(format!("https://{}", val)),
    }
}

pub async fn handle_lookup(key: String, map: Arc<Map>) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(match lookup(key, &map) {
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

    fn wrapped_init(config: String) -> Option<Map> {
        let mut map = HashMap::new();
        init(config, &mut map)?;
        Some(map)
    }

    #[test]
    fn happypath() {
        let config = EXAMPLE_CONFIG.to_string();
        let map = wrapped_init(config).unwrap();
        assert_eq!(
            lookup("/rust".to_owned(), &map),
            Some("https://www.rust-lang.org/".to_string())
        );
        assert_eq!(
            lookup("/trpl-cn".to_owned(), &map),
            Some("https://kaisery.github.io/trpl-zh-cn/".to_string())
        );
    }

    #[test]
    fn config_redundant_value() {
        let config = "key val redundance\nkey val".to_string();
        assert_eq!(wrapped_init(config), None);
    }

    #[test]
    fn config_lack_value() {
        let config = "key val\nkey".to_string();
        assert_eq!(wrapped_init(config), None);
    }

    #[test]
    fn path_no_prefix() {
        let config = EXAMPLE_CONFIG.to_string();
        let map = wrapped_init(config).unwrap();
        assert_eq!(lookup("rust".to_owned(), &map), None);
    }
}
