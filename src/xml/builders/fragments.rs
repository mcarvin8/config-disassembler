//! Fragment reassembly: merge shards without holding the merged document.
//!
//! Reassembly merges every shard's root content into one map, then writes it.
//! The merge only looks at the root's direct children, and each child item is
//! written on its own by `write_element(key, item, 1)`. So each shard is
//! serialized as soon as it is parsed -- every child item becomes a byte range in
//! the shard's buffer -- and its parsed tree is dropped. Only a skeleton is
//! merged: per root key, one item or a list of items, where an item is either a
//! byte range or (for attributes, content keys and primitives) a small value.
//!
//! The result is byte-identical to the tree path (`merge_xml_elements_owned` +
//! `reorder_root_keys_owned` + `build_xml_string`): the skeleton merge makes the
//! same map operations in the same order, including the swap-remove that
//! `serde_json::Map::remove` does under `preserve_order`. The tests check this
//! against the tree path on randomized shard sets.

use indexmap::IndexMap;
use quick_xml::events::{BytesText, Event};
use quick_xml::Writer;
use serde_json::{Map, Value};
use std::io::{BufWriter, Write};
use std::path::Path;

use super::build_xml_string::{is_child_key, write_element, write_object, Field};

/// A byte range in one shard's buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FragRef {
    shard: u32,
    start: u32,
    len: u32,
}

/// One item of a root child: pre-serialized bytes, or a value written as usual.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Item {
    Frag(FragRef),
    Val(Value),
}

/// A merged root child: one item, or a list of items (an array in the tree path).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Entry {
    One(Item),
    Many(Vec<Item>),
}

impl Entry {
    /// The value the tree path would hold for this entry. Only valid for entries
    /// without fragments (attributes and content keys are never fragmented).
    pub(crate) fn to_value(&self) -> Value {
        let item_value = |item: &Item| match item {
            Item::Val(v) => v.clone(),
            Item::Frag(_) => {
                debug_assert!(false, "fragment in an attribute or content key");
                Value::Null
            }
        };
        match self {
            Entry::One(item) => item_value(item),
            Entry::Many(items) => Value::Array(items.iter().map(item_value).collect()),
        }
    }
}

/// A root child as one shard holds it, before merging: the shape decides how the
/// merge combines it (see `merge_xml_elements::merge_element_content`).
#[derive(Debug)]
enum Source {
    /// The value was an array.
    List(Vec<Item>),
    /// The value was an object.
    Object(Item),
    /// Any other value.
    Primitive(Value),
}

/// One parsed shard, serialized: what the merge needs from it, plus its bytes.
#[derive(Debug, Default)]
pub(crate) struct ShardFragments {
    declaration: Option<Value>,
    /// The shard's first non-declaration key (the tree merge's root-key candidate).
    first_key: Option<String>,
    /// Content of each non-declaration key whose value is an object.
    roots: Vec<(String, Vec<(String, Source)>)>,
    bytes: Vec<u8>,
}

impl ShardFragments {
    /// Serialize a parsed shard. A non-object element contributes nothing, as in
    /// the tree merge.
    pub(crate) fn from_element(element: Value) -> Self {
        let Value::Object(obj) = element else {
            return Self::default();
        };
        let mut shard = Self::default();
        for (key, value) in obj {
            if key == "?xml" {
                shard.declaration = Some(value);
                continue;
            }
            if shard.first_key.is_none() {
                shard.first_key = Some(key.clone());
            }
            if let Value::Object(content) = value {
                let sources = fragment_content(content, &mut shard.bytes);
                shard.roots.push((key, sources));
            }
        }
        shard.bytes.shrink_to_fit();
        shard
    }
}

fn fragment_content(content: Map<String, Value>, bytes: &mut Vec<u8>) -> Vec<(String, Source)> {
    let mut sources = Vec::with_capacity(content.len());
    for (key, value) in content {
        let child = is_child_key(&key);
        let mut item = |v: Value| {
            if child {
                Item::Frag(serialize(&key, &v, bytes))
            } else {
                Item::Val(v)
            }
        };
        let source = match value {
            Value::Array(items) => Source::List(items.into_iter().map(&mut item).collect()),
            Value::Object(map) => Source::Object(item(Value::Object(map))),
            primitive => Source::Primitive(primitive),
        };
        sources.push((key, source));
    }
    sources
}

/// Append `write_element(name, value, 1)` -- a root child at its own level -- to
/// `bytes` and return its range (shard index filled in when merging).
fn serialize(name: &str, value: &Value, bytes: &mut Vec<u8>) -> FragRef {
    let start = bytes.len();
    let mut writer = Writer::new(&mut *bytes);
    write_element(&mut writer, name, value, 1).expect("writing to a Vec cannot fail");
    let len = bytes.len() - start;
    FragRef {
        shard: 0,
        start: u32::try_from(start).expect("shard over 4 GiB"),
        len: u32::try_from(len).expect("shard over 4 GiB"),
    }
}

/// The merged document: declaration, root key, and the root's merged children.
#[derive(Debug)]
pub(crate) struct MergedDocument {
    declaration: Value,
    root_key: String,
    content: IndexMap<String, Entry>,
}

/// Merge serialized shards, in order. Mirrors `merge_xml_elements_owned`: the
/// root key is the first shard's first key, the first declaration wins, and each
/// shard's content under the root key is merged in. Takes each shard's skeleton;
/// the shards keep their bytes for writing.
pub(crate) fn merge(shards: &mut [ShardFragments]) -> Option<MergedDocument> {
    if shards.is_empty() {
        log::error!("No elements to merge.");
        return None;
    }
    let root_key = shards.iter().find_map(|s| s.first_key.clone())?;

    let mut declaration = None;
    let mut content = IndexMap::new();
    for (index, shard) in shards.iter_mut().enumerate() {
        if declaration.is_none() {
            declaration = shard.declaration.take();
        }
        let roots = std::mem::take(&mut shard.roots);
        if let Some((_, sources)) = roots.into_iter().find(|(key, _)| *key == root_key) {
            let shard_index = u32::try_from(index).expect("over 4 billion shards");
            merge_sources(&mut content, sources, shard_index);
        }
    }

    Some(MergedDocument {
        declaration: declaration.unwrap_or_else(default_declaration),
        root_key,
        content,
    })
}

fn default_declaration() -> Value {
    let mut decl = Map::new();
    decl.insert("@version".to_string(), Value::String("1.0".to_string()));
    decl.insert("@encoding".to_string(), Value::String("UTF-8".to_string()));
    Value::Object(decl)
}

fn in_shard(item: Item, shard: u32) -> Item {
    match item {
        Item::Frag(frag) => Item::Frag(FragRef { shard, ..frag }),
        other => other,
    }
}

/// Same map operations, in the same order, as `merge_element_content`.
fn merge_sources(target: &mut IndexMap<String, Entry>, sources: Vec<(String, Source)>, shard: u32) {
    for (key, source) in sources {
        match source {
            Source::List(items) => {
                let items = items.into_iter().map(|i| in_shard(i, shard));
                match target.get_mut(&key) {
                    None => {
                        target.insert(key, Entry::Many(items.collect()));
                    }
                    Some(Entry::Many(existing)) => existing.extend(items),
                    Some(Entry::One(_)) => {
                        // `Map::remove` is a swap-remove under `preserve_order`: the
                        // last key moves into this one's slot, and this key is
                        // re-inserted at the end.
                        let Some(Entry::One(existing)) = target.swap_remove(&key) else {
                            unreachable!("checked above");
                        };
                        let mut combined = vec![existing];
                        combined.extend(items);
                        target.insert(key, Entry::Many(combined));
                    }
                }
            }
            Source::Object(item) => {
                let item = in_shard(item, shard);
                match target.get_mut(&key) {
                    Some(Entry::Many(existing)) => existing.push(item),
                    Some(entry @ Entry::One(_)) => {
                        let Entry::One(previous) = std::mem::replace(entry, Entry::Many(vec![]))
                        else {
                            unreachable!("matched above");
                        };
                        *entry = Entry::Many(vec![previous, item]);
                    }
                    None => {
                        target.insert(key, Entry::One(item));
                    }
                }
            }
            Source::Primitive(value) => {
                if !target.contains_key(&key) {
                    target.insert(key, Entry::One(Item::Val(value)));
                }
            }
        }
    }
}

impl MergedDocument {
    /// Set a root child to `value`, keeping its position if it exists (sidecar
    /// injection; same as `Map::insert`).
    pub(crate) fn insert(&mut self, key: String, value: Value) {
        self.content.insert(key, Entry::One(Item::Val(value)));
    }

    /// Reorder the root's children: keys in `key_order` first (first occurrence
    /// wins), then the rest in their current order. Same as
    /// `reorder_root_keys_owned`.
    pub(crate) fn reorder(&mut self, key_order: &[String]) {
        let mut remaining: IndexMap<String, Option<Entry>> = std::mem::take(&mut self.content)
            .into_iter()
            .map(|(k, v)| (k, Some(v)))
            .collect();
        let mut reordered = IndexMap::with_capacity(remaining.len());
        for key in key_order {
            if !reordered.contains_key(key) {
                if let Some(slot) = remaining.get_mut(key) {
                    if let Some(entry) = slot.take() {
                        reordered.insert(key.clone(), entry);
                    }
                }
            }
        }
        for (key, slot) in remaining {
            if let Some(entry) = slot {
                reordered.entry(key).or_insert(entry);
            }
        }
        self.content = reordered;
    }

    /// Whether this document must be written from the parsed tree instead: a
    /// compact root (`#compact`, from `mark_compact_elements`) writes its single
    /// child element inline at the root's own indent, which a child serialized for
    /// the indented layout can't reproduce.
    pub(crate) fn needs_tree_path(&self) -> bool {
        let compact = matches!(
            self.content.get("#compact"),
            Some(Entry::One(Item::Val(Value::Bool(true))))
        );
        if !compact {
            return false;
        }
        let mut children = self.content.iter().filter(|(k, _)| is_child_key(k));
        matches!(
            (children.next(), children.next()),
            (Some((_, Entry::One(Item::Frag(_)))), None)
        )
    }

    /// Serialize into `out`: the same bytes `build_xml_string` writes for the tree
    /// path's merged element.
    pub(crate) fn write<W: Write>(
        &self,
        shards: &[ShardFragments],
        out: W,
    ) -> Result<W, Box<dyn std::error::Error + Send + Sync>> {
        let fragments = Fragments { shards };
        let mut writer = Writer::new(out);
        // The root is an object, so it always writes output and the declaration's
        // separator is always kept (see `build_xml_string::write_xml`).
        if let Some(obj) = self.declaration.as_object() {
            let version = obj
                .get("@version")
                .and_then(|v| v.as_str())
                .unwrap_or("1.0");
            let encoding = obj.get("@encoding").and_then(|v| v.as_str());
            let standalone = obj.get("@standalone").and_then(|v| v.as_str());
            writer.write_event(Event::Decl(quick_xml::events::BytesDecl::new(
                version, encoding, standalone,
            )))?;
            writer.write_event(Event::Text(BytesText::new("\n")))?;
        }
        write_object(
            &mut writer,
            &self.root_key,
            self.content
                .iter()
                .map(|(k, e)| (k.as_str(), Field::Entry(e, &fragments))),
            0,
        )?;
        Ok(writer.into_inner())
    }

    /// Serialize straight to a file at `path` (created or truncated), followed by
    /// a `\n` when `trailing_newline` is set. See `write_xml_file`.
    pub(crate) fn write_file(
        &self,
        shards: &[ShardFragments],
        path: &Path,
        trailing_newline: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file = std::fs::File::create(path)?;
        let mut out = self.write(shards, BufWriter::new(file))?;
        if trailing_newline {
            out.write_all(b"\n")?;
        }
        out.flush()?;
        Ok(())
    }
}

/// Resolves fragment references to their bytes.
pub(crate) struct Fragments<'a> {
    shards: &'a [ShardFragments],
}

impl Fragments<'_> {
    pub(crate) fn bytes(&self, frag: &FragRef) -> &[u8] {
        let start = frag.start as usize;
        &self.shards[frag.shard as usize].bytes[start..start + frag.len as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::builders::{
        build_xml_string, merge_xml_elements_owned, reorder_root_keys_owned,
    };
    use serde_json::json;

    /// Small deterministic PRNG (xorshift64*), so failures reproduce.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        fn chance(&mut self, percent: u64) -> bool {
            self.below(100) < percent
        }
        fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
            items[self.below(items.len() as u64) as usize]
        }
    }

    const TEXTS: &[&str] = &[
        "x",
        "a < b & \"c\"",
        "  padded  ",
        "",
        "line\n    two",
        "42",
    ];
    const NESTED_KEYS: &[&str] = &["p", "q", "@id", "#text", "#cdata", "#comment", "#compact"];
    const ROOT_KEYS: &[&str] = &[
        "a",
        "b",
        "c",
        "d",
        "@xmlns",
        "@x",
        "#text",
        "#cdata",
        "#comment",
        "#text-tail",
        "#raw-text",
        "#compact",
    ];

    fn primitive(rng: &mut Rng) -> Value {
        match rng.below(5) {
            0 => Value::Null,
            1 => json!(rng.below(100)),
            2 => Value::Bool(rng.chance(50)),
            _ => Value::String(rng.pick(TEXTS).to_string()),
        }
    }

    fn value(rng: &mut Rng, depth: u32) -> Value {
        match if depth == 0 { 0 } else { rng.below(4) } {
            0 | 1 => primitive(rng),
            2 => {
                let mut map = Map::new();
                for _ in 0..rng.below(4) {
                    let key = rng.pick(NESTED_KEYS);
                    let v = if key == "#compact" {
                        Value::Bool(rng.chance(50))
                    } else {
                        value(rng, depth - 1)
                    };
                    map.insert(key.to_string(), v);
                }
                Value::Object(map)
            }
            _ => Value::Array((0..rng.below(4)).map(|_| value(rng, depth - 1)).collect()),
        }
    }

    fn shard(rng: &mut Rng) -> Value {
        if rng.chance(3) {
            return Value::String("not an object".to_string());
        }
        let mut obj = Map::new();
        if rng.chance(70) {
            let decl = match rng.below(4) {
                0 => json!("odd"),
                1 => json!({ "@version": "1.0" }),
                _ => json!({ "@version": "1.0", "@encoding": "UTF-8" }),
            };
            obj.insert("?xml".to_string(), decl);
        }
        let root = match rng.below(20) {
            0 => None,
            1 => Some("Other"),
            _ => Some("Root"),
        };
        if let Some(root) = root {
            let content = if rng.chance(5) {
                primitive(rng)
            } else {
                let mut map = Map::new();
                for _ in 0..rng.below(7) {
                    let key = rng.pick(ROOT_KEYS);
                    let v = match key {
                        "#compact" => Value::Bool(rng.chance(70)),
                        k if !is_child_key(k) && rng.chance(80) => primitive(rng),
                        _ => value(rng, 3),
                    };
                    map.insert(key.to_string(), v);
                }
                Value::Object(map)
            };
            obj.insert(root.to_string(), content);
        }
        Value::Object(obj)
    }

    /// The tree path: merge, inject sidecars, reorder, serialize.
    fn via_tree(
        shards: Vec<Value>,
        sidecars: &[(String, Value)],
        order: Option<&[String]>,
    ) -> Option<String> {
        let mut merged = merge_xml_elements_owned(shards)?;
        let root_key = merged
            .as_object()
            .and_then(|o| o.keys().find(|k| *k != "?xml").cloned())?;
        let root = merged.get_mut(&root_key)?.as_object_mut()?;
        for (k, v) in sidecars {
            root.insert(k.clone(), v.clone());
        }
        if let Some(order) = order {
            merged = reorder_root_keys_owned(merged, order);
        }
        Some(build_xml_string(&merged))
    }

    /// The fragment path. `None` for "no document"; `Some(None)` when the
    /// document needs the tree path.
    fn via_fragments(
        shards: Vec<Value>,
        sidecars: &[(String, Value)],
        order: Option<&[String]>,
    ) -> Option<Option<String>> {
        let mut frags: Vec<ShardFragments> = shards
            .into_iter()
            .map(ShardFragments::from_element)
            .collect();
        let mut merged = merge(&mut frags)?;
        for (k, v) in sidecars {
            merged.insert(k.clone(), v.clone());
        }
        if let Some(order) = order {
            merged.reorder(order);
        }
        if merged.needs_tree_path() {
            return Some(None);
        }
        let bytes = merged.write(&frags, Vec::new()).expect("write");
        Some(Some(String::from_utf8(bytes).unwrap()))
    }

    #[test]
    fn fragment_path_matches_tree_path_on_random_shard_sets() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let (mut compared, mut tree_only) = (0, 0);
        for case in 0..4000 {
            let shards: Vec<Value> = (0..1 + rng.below(5)).map(|_| shard(&mut rng)).collect();
            let sidecars: Vec<(String, Value)> = (0..rng.below(3))
                .map(|_| {
                    let key = rng.pick(&["a", "b", "side"]).to_string();
                    (key, json!({ "#raw-text": rng.pick(TEXTS) }))
                })
                .collect();
            let order: Option<Vec<String>> = rng.chance(60).then(|| {
                (0..rng.below(6))
                    .map(|_| {
                        rng.pick(&["a", "b", "c", "d", "@x", "#text", "zz"])
                            .to_string()
                    })
                    .collect()
            });

            let tree = via_tree(shards.clone(), &sidecars, order.as_deref());
            let frag = via_fragments(shards.clone(), &sidecars, order.as_deref());
            match (tree, frag) {
                (None, None) => {}
                (Some(_), Some(None)) => tree_only += 1,
                (Some(t), Some(Some(f))) => {
                    compared += 1;
                    assert_eq!(f, t, "case {case}: shards {shards:#?}");
                }
                (t, f) => panic!("case {case}: tree {t:?} vs fragments {f:?}; shards {shards:#?}"),
            }
        }
        // The generator must actually exercise the comparison.
        assert!(compared > 3000, "only {compared} cases compared");
        assert!(tree_only > 0, "no case exercised the tree-path fallback");
    }

    #[test]
    fn swap_remove_moves_last_key_like_serde_json() {
        // "a" starts as an object, then a later shard has an array under "a": the
        // tree merge swap-removes "a" (moving "c" into its slot) and re-inserts it
        // last. Element order in the output must follow.
        let shards = vec![
            json!({ "Root": { "a": { "p": "1" }, "b": "x", "c": { "p": "2" } } }),
            json!({ "Root": { "a": [{ "p": "3" }] } }),
        ];
        let tree = via_tree(shards.clone(), &[], None).unwrap();
        let frag = via_fragments(shards, &[], None).unwrap().unwrap();
        assert_eq!(frag, tree);
        let (c, b, a) = (frag.find("<c>"), frag.find("<b>"), frag.find("<a>"));
        assert!(c < b && b < a, "expected c, b, a order: {frag}");
    }

    #[test]
    fn needs_tree_path_only_for_compact_root_with_one_serialized_child() {
        let check = |root: Value| {
            let mut frags = vec![ShardFragments::from_element(json!({ "Root": root }))];
            merge(&mut frags).unwrap().needs_tree_path()
        };
        assert!(check(json!({ "#compact": true, "only": { "p": "1" } })));
        assert!(!check(json!({ "#compact": false, "only": { "p": "1" } })));
        assert!(!check(json!({ "only": { "p": "1" } })));
        assert!(!check(
            json!({ "#compact": true, "a": { "p": "1" }, "b": { "p": "2" } })
        ));
        // A list or a primitive child is written the same either way.
        assert!(!check(json!({ "#compact": true, "only": [{ "p": "1" }] })));
        assert!(!check(json!({ "#compact": true, "only": "text" })));
        // Attributes and content keys are not child elements.
        assert!(check(
            json!({ "#compact": true, "@x": "1", "#text": "t", "only": { "p": "1" } })
        ));
    }

    #[test]
    fn write_refuses_compact_root_with_serialized_child() {
        let mut frags = vec![ShardFragments::from_element(
            json!({ "Root": { "#compact": true, "only": { "p": "1" } } }),
        )];
        let merged = merge(&mut frags).unwrap();
        assert!(merged.write(&frags, Vec::new()).is_err());
    }

    #[test]
    fn merge_of_no_shards_or_no_root_is_none() {
        assert!(merge(&mut []).is_none());
        let mut frags = vec![
            ShardFragments::from_element(json!({ "?xml": { "@version": "1.0" } })),
            ShardFragments::from_element(json!("scalar")),
        ];
        assert!(merge(&mut frags).is_none());
    }

    #[test]
    fn shard_bytes_hold_only_serialized_children() {
        let shard = ShardFragments::from_element(json!({
            "?xml": { "@version": "1.0" },
            "Root": { "@xmlns": "ns", "#text": "t", "flag": "true", "item": [{ "p": "1" }] }
        }));
        let bytes = String::from_utf8(shard.bytes.clone()).unwrap();
        // Only the array item is serialized; attributes, content keys and
        // primitives stay values.
        assert_eq!(bytes, "<item>\n        <p>1</p>\n    </item>");
        assert_eq!(
            bytes.len(),
            shard.bytes.capacity(),
            "buffer is shrunk to fit"
        );
    }

    #[test]
    fn entry_to_value_rebuilds_arrays() {
        let entry = Entry::Many(vec![Item::Val(json!("a")), Item::Val(json!(1))]);
        assert_eq!(entry.to_value(), json!(["a", 1]));
        assert_eq!(Entry::One(Item::Val(json!("x"))).to_value(), json!("x"));
    }

    #[test]
    fn list_value_items_are_written_like_tree_array_items() {
        // The merge only puts primitives in a list as values today, but a value
        // item must be written exactly as the tree path writes an array item: at
        // the child's own indent level.
        let entry = Entry::Many(vec![
            Item::Val(json!({ "p": { "q": "1" } })),
            Item::Val(json!("x")),
        ]);
        let merged = MergedDocument {
            declaration: Value::Null,
            root_key: "Root".to_string(),
            content: IndexMap::from([("item".to_string(), entry.clone())]),
        };
        let written = String::from_utf8(merged.write(&[], Vec::new()).unwrap()).unwrap();
        let tree = build_xml_string(&json!({ "Root": { "item": entry.to_value() } }));
        assert_eq!(written, tree);
        assert!(written.contains("\n        <p>"), "{written}");
    }
}
