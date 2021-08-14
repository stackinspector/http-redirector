use std::collections::HashMap;
use regex::Regex;

pub fn parse_map(src: String) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();
    let re = Regex::new("\\s+").unwrap();
    for line in src.split("\n").filter(|line| *line != "") {
        let mut splited = re.split(line);
        let key = splited.next()?.to_string();
        let val = splited.next()?.to_string();
        map.insert(key, val);
    }
    Some(map)
}

pub fn lookup(key: String, map: &HashMap<String, String>) -> Option<String> {
    let mut key = key.chars();
    assert_eq!('/', key.next().unwrap());
    match map.get(key.as_str()) {
        Some(val) => Some(format!("https://{}", val)),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let example_config = r#"
rust            www.rust-lang.org/
crates          crates.io/
docgen          docs.rs/
rust-github     github.com/rust-lang/

trpl            doc.rust-lang.org/stable/book/
trpl-cn         kaisery.github.io/trpl-zh-cn/
rust-cookbook   rust-lang-nursery.github.io/rust-cookbook/
rust-by-example doc.rust-lang.org/rust-by-example/
cargo-book      doc.rust-lang.org/cargo/

tokio-guide     tokio.rs/tokio/tutorial
actix-docs      actix.rs/docs/

"#;
        let map = parse_map(example_config.to_string()).unwrap();
        assert_eq!(
            map.get("rust"),
            Some(&"www.rust-lang.org/".to_string())
        );
        assert_eq!(
            map.get("rust-by-example"),
            Some(&"doc.rust-lang.org/rust-by-example/".to_string())
        );
        assert_eq!(
            lookup("/crates".to_string(), &map),
            Some("https://crates.io/".to_string())
        );
        assert_eq!(
            lookup("/trpl-cn".to_string(), &map),
            Some("https://kaisery.github.io/trpl-zh-cn/".to_string())
        );
    }
}
