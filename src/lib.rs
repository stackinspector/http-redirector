use std::collections::HashMap;
use regex::Regex;

pub fn init(config: String) -> Option<impl Fn(&str) -> Option<String>> {
    let mut map = HashMap::new();
    let re = Regex::new("\\s+").unwrap();
    for line in config.split("\n").filter(|line| *line != "") {
        let mut splited = re.split(line);
        let key = splited.next()?.to_string();
        let val = splited.next()?.to_string();
        map.insert(key, val);
    }
    let map = map;
    Some(move |key: &str| {
        let mut key = key.chars();
        assert_eq!('/', key.next().unwrap());
        match map.get(key.as_str()) {
            Some(val) => Some(format!("https://{}", val)),
            None => None,
        }
    })
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
        let lookup = init(example_config.to_string()).unwrap();
        assert_eq!(
            lookup("/rust"),
            Some("https://www.rust-lang.org/".to_string())
        );
        assert_eq!(
            lookup("/trpl-cn"),
            Some("https://kaisery.github.io/trpl-zh-cn/".to_string())
        );
    }
}
