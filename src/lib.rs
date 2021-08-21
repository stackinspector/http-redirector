use std::collections::HashMap;
use regex::Regex;

pub fn init(config: String) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();
    let re = Regex::new("\\s+").unwrap();
    for line in config.split("\n").filter(|line| *line != "") {
        let mut splited = re.split(line);
        let key = splited.next()?.to_string();
        let val = splited.next()?.to_string();
        match splited.next() {
            None => (),
            Some(_) => return None,
        };
        map.insert(key, val);
    }
    Some(map)
}

pub fn lookup(key: &str, map: &HashMap<String, String>) -> Option<String> {
    let mut key = key.chars();
    match key.next() {
        Some('/') => (),
        _ => return None,
    };
    match map.get(key.as_str()) {
        None => None,
        Some(val) => Some(format!("https://{}", val)),
    }
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
        let config = EXAMPLE_CONFIG.to_string();
        let map = init(config).unwrap();
        assert_eq!(
            lookup("/rust", &map),
            Some("https://www.rust-lang.org/".to_string())
        );
        assert_eq!(
            lookup("/trpl-cn", &map),
            Some("https://kaisery.github.io/trpl-zh-cn/".to_string())
        );
    }

    #[test]
    fn config_redundant_value() {
        let config = "key val redundance\nkey val".to_string();
        assert_eq!(init(config), None);
    }

    #[test]
    fn config_lack_value() {
        let config = "key val\nkey".to_string();
        assert_eq!(init(config), None);
    }

    #[test]
    fn path_no_prefix() {
        let config = EXAMPLE_CONFIG.to_string();
        let map = init(config).unwrap();
        assert_eq!(lookup("rust", &map), None);
    }
}
