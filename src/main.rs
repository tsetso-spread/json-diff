use colored::*;
use serde_json;
use serde_json::Map;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::process;
use std::str::FromStr;
use structopt::StructOpt;
use constants::Message;

mod constants;

const HELP: &str = r#"
Example:
json_diff f source1.json source2.json
json_diff d '{...}' '{...}'

Option:
f   :   read input from json files
d   :   read input from command line"#;

#[derive(Debug)]
struct AppError {
    message: Message,
}
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

enum InputReadMode {
    D,
    F,
}
impl FromStr for InputReadMode {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "d" => Ok(InputReadMode::D),
            "f" => Ok(InputReadMode::F),
            _ => Err(Self::Err {
                message: Message::BadOption,
            }),
        }
    }
}

#[derive(StructOpt)]
#[structopt(about = HELP)]
struct Cli {
    read_mode: InputReadMode,
    source1: String,
    source2: String,
}

fn error_exit(message: constants::Message) -> ! {
    eprintln!("{}", message);
    process::exit(1);
}

fn main() {
    let args = Cli::from_args();

    let (data1, data2) = match args.read_mode {
        InputReadMode::D => (args.source1, args.source2),
        InputReadMode::F => {
            if let Ok(d1) = fs::read_to_string(args.source1) {
                if let Ok(d2) = fs::read_to_string(args.source2) {
                    (d1, d2)
                } else {
                    error_exit(Message::SOURCE2);
                }
            } else {
                error_exit(Message::SOURCE1);
            }
        }
    };
    display_output(compare_jsons(&data1, &data2));
}

fn display_output(result: Mismatch) {
    let no_mismatch = Mismatch {
        left_only_keys: KeyNode::Nil,
        right_only_keys: KeyNode::Nil,
        keys_in_both: KeyNode::Nil,
    };
    if no_mismatch == result {
        println!("{}", Message::NoMismatch);
    } else {
        match result.keys_in_both {
            KeyNode::Node(_) => {
                let mut keys = Vec::new();
                result.keys_in_both.absolute_keys(&mut keys, None);
                println!("{}:", Message::Mismatch);
                for key in keys {
                    println!("{}", key);
                }
            }
            KeyNode::Value(_, _) => println!("{}", Message::RootMismatch),
            KeyNode::Nil => (),
        }
        match result.left_only_keys {
            KeyNode::Node(_) => {
                let mut keys = Vec::new();
                result.left_only_keys.absolute_keys(&mut keys, None);
                println!("{}:", Message::LeftExtra);
                for key in keys {
                    println!("{}", key.red().bold());
                }
            }
            KeyNode::Value(_, _) => error_exit(Message::UnknownError),
            KeyNode::Nil => (),
        }
        match result.right_only_keys {
            KeyNode::Node(_) => {
                let mut keys = Vec::new();
                result.right_only_keys.absolute_keys(&mut keys, None);
                println!("{}:", Message::RightExtra);
                for key in keys {
                    println!("{}", key.green().bold());
                }
            }
            KeyNode::Value(_, _) => error_exit(Message::UnknownError),
            KeyNode::Nil => (),
        }
    }
}

#[derive(Debug, PartialEq)] // TODO check: do we need PartiaEq ?
enum KeyNode {
    Nil,
    Value(Value, Value),
    Node(HashMap<String, KeyNode>),
}

impl KeyNode {
    fn absolute_keys(&self, keys: &mut Vec<String>, key_from_root: Option<String>) {
        let val_key = |key: Option<String>| {
            key.map(|mut s| {
                s.push_str(" ->");
                s
            })
            .unwrap_or(String::new())
        };
        let nil_key = |key: Option<String>| key.unwrap_or(String::new());
        match self {
            KeyNode::Nil => keys.push(nil_key(key_from_root)),
            KeyNode::Value(a, b) => keys.push(format!(
                "{} [ {} :: {} ]",
                val_key(key_from_root),
                a.to_string().blue().bold(),
                b.to_string().cyan().bold()
            )),
            KeyNode::Node(map) => {
                for (key, value) in map {
                    value.absolute_keys(
                        keys,
                        Some(format!("{} {}", val_key(key_from_root.clone()), key)),
                    )
                }
            }
        }
    }
}

#[derive(Debug, PartialEq)]
struct Mismatch {
    left_only_keys: KeyNode,
    right_only_keys: KeyNode,
    keys_in_both: KeyNode,
}

impl Mismatch {
    fn new(l: KeyNode, r: KeyNode, u: KeyNode) -> Mismatch {
        Mismatch {
            left_only_keys: l,
            right_only_keys: r,
            keys_in_both: u,
        }
    }
}

fn compare_jsons(a: &str, b: &str) -> Mismatch {
    if let Ok(value1) = serde_json::from_str(a) {
        if let Ok(value2) = serde_json::from_str(b) {
            match_json(&value1, &value2)
        } else {
            error_exit(Message::JSON2);
        }
    } else {
        error_exit(Message::JSON1);
    }
}

fn match_json(value1: &Value, value2: &Value) -> Mismatch {
    match (value1, value2) {
        (Value::Object(a), Value::Object(b)) => {
            let (left_only_keys, right_only_keys, intersection_keys) = intersect_maps(&a, &b);

            let mut unequal_keys = KeyNode::Nil;
            let mut left_only_keys = get_map_of_keys(left_only_keys);
            let mut right_only_keys = get_map_of_keys(right_only_keys);

            if let Some(intersection_keys) = intersection_keys {
                for key in intersection_keys {
                    let Mismatch {
                        left_only_keys: l,
                        right_only_keys: r,
                        keys_in_both: u,
                    } = match_json(&a.get(&key).unwrap(), &b.get(&key).unwrap());
                    left_only_keys = insert_child_key_map(left_only_keys, l, &key);
                    right_only_keys = insert_child_key_map(right_only_keys, r, &key);
                    unequal_keys = insert_child_key_map(unequal_keys, u, &key);
                }
            }
            Mismatch::new(left_only_keys, right_only_keys, unequal_keys)
        }
        (a, b) => {
            if a == b {
                Mismatch::new(KeyNode::Nil, KeyNode::Nil, KeyNode::Nil)
            } else {
                Mismatch::new(
                    KeyNode::Nil,
                    KeyNode::Nil,
                    KeyNode::Value(a.clone(), b.clone()),
                )
            }
        }
    }
}

fn get_map_of_keys(set: Option<HashSet<String>>) -> KeyNode {
    if let Some(set) = set {
        KeyNode::Node(
            set.iter()
                .map(|key| (String::from(key), KeyNode::Nil))
                .collect(),
        )
    } else {
        KeyNode::Nil
    }
}

fn insert_child_key_map(parent: KeyNode, child: KeyNode, key: &String) -> KeyNode {
    if child == KeyNode::Nil {
        return parent;
    }
    if let KeyNode::Node(mut map) = parent {
        map.insert(String::from(key), child);
        KeyNode::Node(map) // This is weird! I just wanted to return back `parent` here
    } else if let KeyNode::Nil = parent {
        let mut map = HashMap::new();
        map.insert(String::from(key), child);
        KeyNode::Node(map)
    } else {
        parent // TODO Trying to insert child node in a Value variant : Should not happen => Throw an error instead.
    }
}

fn intersect_maps(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
) -> (
    Option<HashSet<String>>,
    Option<HashSet<String>>,
    Option<HashSet<String>>,
) {
    let mut intersection = HashSet::new();
    let mut left = HashSet::new();
    let mut right = HashSet::new();
    for a_key in a.keys() {
        if b.contains_key(a_key) {
            intersection.insert(String::from(a_key));
        } else {
            left.insert(String::from(a_key));
        }
    }
    for b_key in b.keys() {
        if !a.contains_key(b_key) {
            right.insert(String::from(b_key));
        }
    }
    let left = if left.len() == 0 { None } else { Some(left) };
    let right = if right.len() == 0 { None } else { Some(right) };
    let intersection = if intersection.len() == 0 {
        None
    } else {
        Some(intersection)
    };
    (left, right, intersection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use maplit::hashmap;
    use serde_json::json;

    #[test]
    fn nested_diff() {
        let data1 = r#"{
            "a":"b", 
            "b":{
                "c":{
                    "d":true,
                    "e":5,
                    "f":9,
                    "h":{
                        "i":true,
                        "j":false
                    }
                }
            }
        }"#;
        let data2 = r#"{
            "a":"b",
            "b":{
                "c":{
                    "d":true,
                    "e":6,
                    "g":0,
                    "h":{
                        "i":false,
                        "k":false
                    }
                }
            }
        }"#;

        let mismatch = compare_jsons(data1, data2);
        let expected_left = KeyNode::Node(hashmap! {
        "b".to_string() => KeyNode::Node(hashmap! {
                "c".to_string() => KeyNode::Node(hashmap! {
                        "f".to_string() => KeyNode::Nil,
                        "h".to_string() => KeyNode::Node( hashmap! {
                                "j".to_string() => KeyNode::Nil,
                            }
                        ),
                }
                ),
            }),
        });
        let expected_right = KeyNode::Node(hashmap! {
            "b".to_string() => KeyNode::Node(hashmap! {
                    "c".to_string() => KeyNode::Node(hashmap! {
                            "g".to_string() => KeyNode::Nil,
                            "h".to_string() => KeyNode::Node(hashmap! {
                                    "k".to_string() => KeyNode::Nil,
                                }
                            )
                        }
                    )
                }
            )
        });
        let expected_uneq = KeyNode::Node(hashmap! {
            "b".to_string() => KeyNode::Node(hashmap! {
                    "c".to_string() => KeyNode::Node(hashmap! {
                            "e".to_string() => KeyNode::Value(json!(5), json!(6)),
                            "h".to_string() => KeyNode::Node(hashmap! {
                                    "i".to_string() => KeyNode::Value(json!(true), json!(false)),
                                }
                            )
                        }
                    )
                }
            )
        });
        let expected = Mismatch::new(expected_left, expected_right, expected_uneq);
        assert_eq!(mismatch, expected, "Diff was incorrect.");
    }

    #[test]
    fn no_diff() {
        let data1 = r#"{
            "a":"b", 
            "b":{
                "c":{
                    "d":true,
                    "e":5,
                    "f":9,
                    "h":{
                        "i":true,
                        "j":false
                    }
                }
            }
        }"#;
        let data2 = r#"{
            "a":"b", 
            "b":{
                "c":{
                    "d":true,
                    "e":5,
                    "f":9,
                    "h":{
                        "i":true,
                        "j":false
                    }
                }
            }
        }"#;

        assert_eq!(
            compare_jsons(data1, data2),
            Mismatch::new(KeyNode::Nil, KeyNode::Nil, KeyNode::Nil)
        );
    }

    #[test]
    fn no_json() {
        let data1 = r#"{}"#;
        let data2 = r#"{}"#;

        assert_eq!(
            compare_jsons(data1, data2),
            Mismatch::new(KeyNode::Nil, KeyNode::Nil, KeyNode::Nil)
        );
    }
}
