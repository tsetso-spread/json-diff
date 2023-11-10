use diffs::{myers, Diff, Replace};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::enums::Error;
use serde_json::Map;
use serde_json::Value;

use crate::ds::key_node::KeyNode;
use crate::ds::mismatch::Mismatch;

pub fn compare_jsons(a: &str, b: &str) -> Result<Mismatch, Error> {
    let value1 = serde_json::from_str(a)?;
    let value2 = serde_json::from_str(b)?;
    Ok(match_json(&value1, &value2))
}
fn values_to_node(vec: Vec<(usize, &Value)>) -> KeyNode {
    if vec.is_empty() {
        KeyNode::Nil
    } else {
        KeyNode::Node(
            vec.into_iter()
                .map(|(id, val)| (format!("[l: {id}] - {}", val.to_string()), KeyNode::Nil))
                .collect(),
        )
    }
}

struct ListDiffHandler<'a> {
    replaced: &'a mut Vec<(usize, usize, usize, usize)>,
    deletion: &'a mut Vec<(usize, usize)>,
    insertion: &'a mut Vec<(usize, usize)>,
}
impl<'a> ListDiffHandler<'a> {
    pub fn new(
        replaced: &'a mut Vec<(usize, usize, usize, usize)>,
        deletion: &'a mut Vec<(usize, usize)>,
        insertion: &'a mut Vec<(usize, usize)>,
    ) -> Self {
        Self {
            replaced,
            deletion,
            insertion,
        }
    }
}
impl<'a> Diff for ListDiffHandler<'a> {
    type Error = ();
    fn delete(&mut self, old: usize, len: usize, _new: usize) -> Result<(), ()> {
        self.deletion.push((old, len));
        Ok(())
    }
    fn insert(&mut self, _o: usize, new: usize, len: usize) -> Result<(), ()> {
        self.insertion.push((new, len));
        Ok(())
    }
    fn replace(&mut self, old: usize, len: usize, new: usize, new_len: usize) -> Result<(), ()> {
        self.replaced.push((old, len, new, new_len));
        Ok(())
    }
}

pub fn match_json(value1: &Value, value2: &Value) -> Mismatch {
    match (value1, value2) {
        (Value::Object(a), Value::Object(b)) => {
            let diff = intersect_maps(a, b);
            let mut left_only_keys = get_map_of_keys(diff.left_only);
            let mut right_only_keys = get_map_of_keys(diff.right_only);
            let intersection_keys = diff.intersection;

            let mut unequal_keys = KeyNode::Nil;

            if let Some(intersection_keys) = intersection_keys {
                for key in intersection_keys {
                    let Mismatch {
                        left_only_keys: l,
                        right_only_keys: r,
                        keys_in_both: u,
                    } = match_json(a.get(&key).unwrap(), b.get(&key).unwrap());
                    left_only_keys = insert_child_key_map(left_only_keys, l, &key);
                    right_only_keys = insert_child_key_map(right_only_keys, r, &key);
                    unequal_keys = insert_child_key_map(unequal_keys, u, &key);
                }
            }
            Mismatch::new(left_only_keys, right_only_keys, unequal_keys)
        }
        // this clearly needs to be improved! myers algorithm or whatever?
        (Value::Array(a), Value::Array(b)) => {
            let mut replaced = Vec::new();
            let mut deleted = Vec::new();
            let mut inserted = Vec::new();

            let mut diff = Replace::new(ListDiffHandler::new(
                &mut replaced,
                &mut deleted,
                &mut inserted,
            ));
            myers::diff(&mut diff, a, 0, a.len(), b, 0, b.len()).unwrap();

            let mismatch: Vec<_> = replaced
                .into_iter()
                .flat_map(|(o, ol, n, nl)| {
                    let max_length = ol.max(nl);
                    (0..max_length).map(move |i| {
                        (
                            o + i,
                            match_json(
                                a.get(o + i).unwrap_or(&Value::Null),
                                b.get(n + i).unwrap_or(&Value::Null),
                            )
                            .keys_in_both,
                        )
                    })
                })
                .collect();

            let left_only_values: Vec<_> = deleted
                .into_iter()
                .flat_map(|(o, ol)| (o..o + ol).map(|i| (i, &a[i])))
                .collect();

            let right_only_values: Vec<_> = inserted
                .into_iter()
                .flat_map(|(n, nl)| (n..n + nl).map(|i| (i, &b[i])))
                .collect();

            let left_only_nodes = values_to_node(left_only_values);
            let right_only_nodes = values_to_node(right_only_values);
            let mismatch = if mismatch.is_empty() {
                KeyNode::Nil
            } else {
                KeyNode::Array(mismatch)
            };
            Mismatch::new(left_only_nodes, right_only_nodes, mismatch)
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

struct MapDifference {
    left_only: Option<HashSet<String>>,
    right_only: Option<HashSet<String>>,
    intersection: Option<HashSet<String>>,
}

impl MapDifference {
    pub fn new(
        left_only: Option<HashSet<String>>,
        right_only: Option<HashSet<String>>,
        intersection: Option<HashSet<String>>,
    ) -> Self {
        Self {
            right_only,
            left_only,
            intersection,
        }
    }
}

fn intersect_maps(a: &Map<String, Value>, b: &Map<String, Value>) -> MapDifference {
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
    let left = if left.is_empty() { None } else { Some(left) };
    let right = if right.is_empty() { None } else { Some(right) };
    let intersection = if intersection.is_empty() {
        None
    } else {
        Some(intersection)
    };
    MapDifference::new(left, right, intersection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use maplit::hashmap;
    use serde_json::json;

    #[test]
    fn test_arrays_simple_diff() {
        let data1 = r#"["a","b","c"]"#;
        let data2 = r#"["a","b","d"]"#;
        let diff = compare_jsons(data1, data2).unwrap();
        assert_eq!(diff.left_only_keys, KeyNode::Nil);
        assert_eq!(diff.right_only_keys, KeyNode::Nil);
        let diff = diff.keys_in_both.absolute_keys_to_vec(None);
        assert_eq!(diff.len(), 1);
        assert_eq!(
            diff.first().unwrap().to_string(),
            r#"[l: 2]  -> { "c" != "d" }"#
        );
    }

    #[test]
    fn test_arrays_more_complex_diff() {
        let data1 = r#"["a","b","c"]"#;
        let data2 = r#"["a","a","b","d"]"#;
        let diff = compare_jsons(data1, data2).unwrap();

        let changes_diff = diff.keys_in_both.absolute_keys_to_vec(None);
        assert_eq!(diff.left_only_keys, KeyNode::Nil);

        assert_eq!(changes_diff.len(), 1);
        assert_eq!(
            changes_diff.first().unwrap().to_string(),
            r#"[l: 2]  -> { "c" != "d" }"#
        );
        let insertions = diff.right_only_keys.absolute_keys_to_vec(None);
        assert_eq!(insertions.len(), 1);
        assert_eq!(insertions.first().unwrap().to_string(), r#" [l: 0] - "a""#);
    }

    #[test]
    fn test_arrays_extra_left() {
        let data1 = r#"["a","b","c"]"#;
        let data2 = r#"["a","b"]"#;
        let diff = compare_jsons(data1, data2).unwrap();

        let diffs = diff.left_only_keys.absolute_keys_to_vec(None);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs.first().unwrap().to_string(), r#" [l: 2] - "c""#);
        assert_eq!(diff.keys_in_both, KeyNode::Nil);
        assert_eq!(diff.right_only_keys, KeyNode::Nil);
    }

    #[test]
    fn test_arrays_extra_right() {
        let data1 = r#"["a","b"]"#;
        let data2 = r#"["a","b","c"]"#;
        let diff = compare_jsons(data1, data2).unwrap();

        let diffs = diff.right_only_keys.absolute_keys_to_vec(None);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs.first().unwrap().to_string(), r#" [l: 2] - "c""#);
        assert_eq!(diff.keys_in_both, KeyNode::Nil);
        assert_eq!(diff.left_only_keys, KeyNode::Nil);
    }

    #[test]
    fn long_insertion_modification() {
        let data1 = r#"["a","b","a"]"#;
        let data2 = r#"["a","c","c","c","a"]"#;
        let diff = compare_jsons(data1, data2).unwrap();
        let diffs = diff.keys_in_both.absolute_keys_to_vec(None);
        // 1. is b!=c, second is a!=c, third is missing in data1 but c in data2
        assert_eq!(diffs.len(), 3);
        assert_eq!(
            diffs.last().unwrap().to_string(),
            r#"[l: 3]  -> { null != "c" }"#
        );
        assert_eq!(diff.right_only_keys, KeyNode::Nil);
        assert_eq!(diff.left_only_keys, KeyNode::Nil);
    }

    #[test]
    fn test_arrays_object_extra() {
        let data1 = r#"["a","b"]"#;
        let data2 = r#"["a","b", {"c": {"d": "e"} }]"#;
        let diff = compare_jsons(data1, data2).unwrap();

        let diffs = diff.right_only_keys.absolute_keys_to_vec(None);
        assert_eq!(diffs.len(), 1);
        assert_eq!(
            diffs.first().unwrap().to_string(),
            r#" [l: 2] - {"c":{"d":"e"}}"#
        );
        assert_eq!(diff.keys_in_both, KeyNode::Nil);
        assert_eq!(diff.left_only_keys, KeyNode::Nil);
    }

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

        let mismatch = compare_jsons(data1, data2).unwrap();
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
            compare_jsons(data1, data2).unwrap(),
            Mismatch::new(KeyNode::Nil, KeyNode::Nil, KeyNode::Nil)
        );
    }

    #[test]
    fn no_json() {
        let data1 = r#"{}"#;
        let data2 = r#"{}"#;

        assert_eq!(
            compare_jsons(data1, data2).unwrap(),
            Mismatch::new(KeyNode::Nil, KeyNode::Nil, KeyNode::Nil)
        );
    }

    #[test]
    fn parse_err_source_one() {
        let invalid_json1 = r#"{invalid: json}"#;
        let valid_json2 = r#"{"a":"b"}"#;
        match compare_jsons(invalid_json1, valid_json2) {
            Ok(_) => panic!("This shouldn't be an Ok"),
            Err(err) => {
                matches!(err, Error::JSON(_));
            }
        };
    }

    #[test]
    fn parse_err_source_two() {
        let valid_json1 = r#"{"a":"b"}"#;
        let invalid_json2 = r#"{invalid: json}"#;
        match compare_jsons(valid_json1, invalid_json2) {
            Ok(_) => panic!("This shouldn't be an Ok"),
            Err(err) => {
                matches!(err, Error::JSON(_));
            }
        };
    }
}
