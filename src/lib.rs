use std::collections::HashMap;
use regex::Regex;

pub fn init(config: String) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();
    let re = Regex::new("\\s+").unwrap();
    for line in config.split("\n").filter(|line| *line != "") {
        let mut splited = re.split(line);
        let key = splited.next()?.to_string();
        let val = splited.next()?.to_string();
        assert_eq!(splited.next(), None);
        map.insert(key, val);
    }
    Some(map)
}

pub fn lookup(key: &str, map: &HashMap<String, String>) -> Option<String> {
    let mut key = key.chars();
    assert_eq!(Some('/'), key.next());
    match map.get(key.as_str()) {
        None => None,
        Some(val) => Some(format!("https://{}", val)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let example_config = r#"
rust    www.rust-lang.org/

trpl    doc.rust-lang.org/stable/book/
trpl-cn kaisery.github.io/trpl-zh-cn/
"#;
        let map = init(example_config.to_string()).unwrap();
        assert_eq!(
            lookup("/rust", &map),
            Some("https://www.rust-lang.org/".to_string())
        );
        assert_eq!(
            lookup("/trpl-cn", &map),
            Some("https://kaisery.github.io/trpl-zh-cn/".to_string())
        );
    }
}
