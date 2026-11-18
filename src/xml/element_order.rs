//! Original document order of repeated sibling elements.
//!
//! The root's children are stored as `tag -> [elements]`, so `.key_order.json` can only
//! record the order of tags. When each sibling becomes its own shard file (the unique-id
//! strategy, or a grouped-by-tag `split` rule), reassembly used to read those shards in
//! filename order: alphabetical by unique id, or arbitrary for content-hash names. For
//! metadata where sibling order is meaningful (picklist values, page-layout sections,
//! approval steps), that silently reorders the document.
//!
//! Disassembly now writes `.element_order.json` next to `.key_order.json`, mapping each tag
//! to its shard paths (relative to the disassembled directory) in source order. Reassembly
//! sorts shards by that recorded position. Anything not listed -- a fragment added by hand,
//! or every fragment of a directory written before this file existed -- keeps the previous
//! filename order, after the listed ones. It is a separate file (rather than a new shape
//! for `.key_order.json`) so older versions reading newer output are unaffected.

use crate::xml::utils::normalize_path_unix;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;
use tokio::fs;

pub(crate) const ELEMENT_ORDER_FILE: &str = ".element_order.json";

/// Source order of siblings a grouped-by-tag `group` rule spread across several files (e.g.
/// `fieldPermissions` grouped by object under `-p`). Each group file keeps its own entries in
/// order, but the files are read back one after another, so siblings that alternated between
/// groups in the source (`A`, `B`, `A`) came back regrouped (`A`, `A`, `B`). A separate file, so
/// readers of `.element_order.json` (including older versions) are unaffected; written only when
/// the source order differs from that default.
pub(crate) const GROUP_ORDER_FILE: &str = ".group_order.json";

/// The recorded sequence for one group directory: runs of consecutive siblings from the same
/// group file, keyed by that file's item key (`RollupLog__c` for `RollupLog__c.fieldPermissions-meta.xml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GroupRecord {
    pub(crate) tag: String,
    pub(crate) runs: Vec<(String, usize)>,
}

/// Run-length encode `keys` (each sibling's group, in source order).
fn runs_of(keys: &[String]) -> Vec<(String, usize)> {
    let mut runs: Vec<(String, usize)> = Vec::new();
    for key in keys {
        match runs.last_mut() {
            Some((last, count)) if last == key => *count += 1,
            _ => runs.push((key.clone(), 1)),
        }
    }
    runs
}

/// Rebuild the source order from each group file's siblings (in file order) and the recorded
/// runs. Siblings beyond a run's recorded count, and groups not recorded at all (both from
/// edits), follow at the end in group-key order, so nothing is ever dropped.
pub(crate) fn interleave(
    record: &GroupRecord,
    mut queues: BTreeMap<String, VecDeque<Value>>,
) -> Vec<Value> {
    let mut out = Vec::new();
    for (key, count) in &record.runs {
        if let Some(queue) = queues.get_mut(key) {
            for _ in 0..*count {
                match queue.pop_front() {
                    Some(item) => out.push(item),
                    None => break,
                }
            }
        }
    }
    for (_, queue) in queues {
        out.extend(queue);
    }
    out
}

/// A shard's identity within its directory, stable between what disassembly writes and
/// what a later step may rename it to: `Account.Name__c.fieldPermissions-meta.xml` and a
/// multi-level item reassembled as `Account.Name__c.xml` both map to `Account.Name__c`.
pub(crate) fn item_key(file_name: &str) -> &str {
    let without_ext = match file_name.rfind("-meta.") {
        // `<item>.<tag>-meta.<ext>`: drop `-meta.<ext>`, then the `.<tag>` before it.
        Some(meta) => &file_name[..meta],
        None => file_name,
    };
    without_ext
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(without_ext)
}

/// `dir/item` (or just `item` at the top level), the lookup key shared by both sides.
fn rank_key(rel_dir: &str, file_name: &str) -> String {
    let item = item_key(file_name);
    if rel_dir.is_empty() {
        item.to_string()
    } else {
        format!("{rel_dir}/{item}")
    }
}

/// Collects shard paths per tag, in source order, while a file is disassembled.
#[derive(Debug, Default)]
pub(crate) struct ElementOrderRecorder {
    by_tag: BTreeMap<String, Vec<String>>,
    groups: BTreeMap<String, GroupRecord>,
}

impl ElementOrderRecorder {
    /// Record the source order of `tag` siblings a `group` rule wrote into `rel_dir` (relative
    /// to the disassembled directory), one group per sibling in `keys`. Nothing is recorded when
    /// reassembly's default -- each group file in turn, in file-name order -- already reproduces it.
    pub(crate) fn record_groups(&mut self, rel_dir: &str, tag: &str, keys: &[String], ext: &str) {
        let runs = runs_of(keys);
        let file_name = |key: &str| format!("{key}.{tag}-meta.{ext}");
        let mut default_order: Vec<&str> = runs.iter().map(|(key, _)| key.as_str()).collect();
        default_order.sort_by_key(|key| file_name(key));
        default_order.dedup();
        let is_default = default_order.len() == runs.len()
            && runs
                .iter()
                .map(|(key, _)| key.as_str())
                .eq(default_order.iter().copied());
        if !is_default {
            self.groups.insert(
                normalize_path_unix(rel_dir),
                GroupRecord {
                    tag: tag.to_string(),
                    runs,
                },
            );
        }
    }

    /// Record `shard_path` (somewhere under `disassembled_path`) as the next `tag` sibling.
    pub(crate) fn record(&mut self, tag: &str, disassembled_path: &str, shard_path: &Path) {
        let shard = normalize_path_unix(&shard_path.to_string_lossy());
        let root = normalize_path_unix(disassembled_path);
        let rel = shard
            .strip_prefix(&root)
            .map(|r| r.trim_start_matches('/'))
            .unwrap_or(&shard);
        self.by_tag
            .entry(tag.to_string())
            .or_default()
            .push(rel.to_string());
    }

    /// Write the order file, or remove a stale one when nothing was split into
    /// per-element files this time (e.g. re-disassembled with grouped-by-tag).
    /// Best-effort, like the other sidecars.
    pub(crate) async fn write(&self, disassembled_path: &str) {
        let path = Path::new(disassembled_path).join(ELEMENT_ORDER_FILE);
        if self.by_tag.is_empty() {
            let _ = fs::remove_file(path).await;
        } else if let Ok(json) = serde_json::to_string_pretty(&self.by_tag) {
            // Pretty-printed, one path per line, so a reorder shows up as a small diff.
            let _ = fs::write(path, json + "\n").await;
        }

        let path = Path::new(disassembled_path).join(GROUP_ORDER_FILE);
        if self.groups.is_empty() {
            let _ = fs::remove_file(path).await;
        } else if let Ok(json) = serde_json::to_string_pretty(&self.groups) {
            let _ = fs::write(path, json + "\n").await;
        }
    }
}

/// The recorded order for one disassembled directory, loaded at reassembly.
#[derive(Debug)]
pub(crate) struct ElementOrder {
    root: String,
    ranks: HashMap<String, usize>,
    groups: HashMap<String, GroupRecord>,
}

impl ElementOrder {
    /// Load `<disassembled_path>/.element_order.json` and `.group_order.json`. `None` when both
    /// are absent or unreadable, which keeps filename order for the whole directory.
    pub(crate) async fn load(disassembled_path: &str) -> Option<Self> {
        let dir = Path::new(disassembled_path);
        let by_tag: Option<BTreeMap<String, Vec<String>>> = fs::read(dir.join(ELEMENT_ORDER_FILE))
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let groups: Option<HashMap<String, GroupRecord>> = fs::read(dir.join(GROUP_ORDER_FILE))
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        if by_tag.is_none() && groups.is_none() {
            return None;
        }
        let mut ranks = HashMap::new();
        for paths in by_tag.unwrap_or_default().values() {
            for (index, rel) in paths.iter().enumerate() {
                let (rel_dir, name) = rel.rsplit_once('/').unwrap_or(("", rel));
                ranks.entry(rank_key(rel_dir, name)).or_insert(index);
            }
        }
        Some(Self {
            root: normalize_path_unix(disassembled_path),
            ranks,
            groups: groups.unwrap_or_default(),
        })
    }

    /// The recorded group sequence for the group directory `dir_path`, if any.
    pub(crate) fn group_for(&self, dir_path: &str) -> Option<&GroupRecord> {
        let rel_dir = dir_path.strip_prefix(&self.root)?.trim_start_matches('/');
        self.groups.get(rel_dir)
    }

    /// Recorded position of `file_name` inside `dir_path`, if it was listed.
    pub(crate) fn rank(&self, dir_path: &str, file_name: &str) -> Option<usize> {
        let rel_dir = dir_path.strip_prefix(&self.root)?.trim_start_matches('/');
        self.ranks.get(&rank_key(rel_dir, file_name)).copied()
    }
}

/// Sort key for a directory entry: recorded siblings first in their recorded order, then
/// everything else (unlisted files, subdirectories, leaf files) in filename order.
pub(crate) fn entry_sort_key(
    order: Option<&ElementOrder>,
    dir_path: &str,
    file_name: String,
) -> (bool, usize, String) {
    let rank = order.and_then(|o| o.rank(dir_path, &file_name));
    (rank.is_none(), rank.unwrap_or(0), file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_key_strips_tag_meta_and_extension() {
        assert_eq!(
            item_key("Account.Name__c.fieldPermissions-meta.xml"),
            "Account.Name__c"
        );
        assert_eq!(item_key("9647135a.layoutSections-meta.yaml"), "9647135a");
        assert_eq!(item_key("Welcome.xml"), "Welcome");
        assert_eq!(item_key("noext"), "noext");
    }

    #[tokio::test]
    async fn records_relative_paths_and_ranks_them() {
        let tmp = tempfile::tempdir().unwrap();
        let root = normalize_path_unix(&tmp.path().to_string_lossy());
        let mut rec = ElementOrderRecorder::default();
        for name in ["Zeta", "Alpha"] {
            let shard = tmp
                .path()
                .join("fieldPermissions")
                .join(format!("{name}.fieldPermissions-meta.xml"));
            rec.record("fieldPermissions", &root, &shard);
        }
        rec.write(&root).await;

        let order = ElementOrder::load(&root).await.expect("order file");
        let dir = format!("{root}/fieldPermissions");
        assert_eq!(order.rank(&dir, "Zeta.fieldPermissions-meta.xml"), Some(0));
        assert_eq!(order.rank(&dir, "Alpha.fieldPermissions-meta.xml"), Some(1));
        // A multi-level item reassembled under a different file name still ranks.
        assert_eq!(order.rank(&dir, "Alpha.xml"), Some(1));
        assert_eq!(order.rank(&dir, "Unknown.fieldPermissions-meta.xml"), None);
        // Outside the recorded root: never ranked.
        assert_eq!(
            order.rank("/elsewhere", "Zeta.fieldPermissions-meta.xml"),
            None
        );
    }

    #[tokio::test]
    async fn writing_an_empty_recorder_removes_a_stale_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = normalize_path_unix(&tmp.path().to_string_lossy());
        std::fs::write(tmp.path().join(ELEMENT_ORDER_FILE), "{}").unwrap();
        ElementOrderRecorder::default().write(&root).await;
        assert!(!tmp.path().join(ELEMENT_ORDER_FILE).exists());
    }

    #[test]
    fn entry_sort_key_puts_ranked_entries_first() {
        let order = ElementOrder {
            root: "/r".into(),
            ranks: HashMap::from([("t/b".to_string(), 0), ("t/a".to_string(), 1)]),
            groups: HashMap::new(),
        };
        let mut names = vec!["a.t-meta.xml", "c.t-meta.xml", "b.t-meta.xml"];
        names.sort_by_cached_key(|n| entry_sort_key(Some(&order), "/r/t", n.to_string()));
        assert_eq!(names, ["b.t-meta.xml", "a.t-meta.xml", "c.t-meta.xml"]);
        names.sort_by_cached_key(|n| entry_sort_key(None, "/r/t", n.to_string()));
        assert_eq!(names, ["a.t-meta.xml", "b.t-meta.xml", "c.t-meta.xml"]);
    }

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    fn run(key: &str, count: usize) -> (String, usize) {
        (key.to_string(), count)
    }

    #[test]
    fn runs_of_counts_consecutive_keys() {
        assert_eq!(
            runs_of(&keys(&["a", "b", "b", "a", "a", "a"])),
            [run("a", 1), run("b", 2), run("a", 3)]
        );
        assert!(runs_of(&[]).is_empty());
    }

    #[test]
    fn record_groups_skips_the_default_order() {
        let mut recorder = ElementOrderRecorder::default();
        recorder.record_groups("fp", "fp", &keys(&["a", "a", "b"]), "xml");
        recorder.record_groups("fp", "fp", &keys(&[]), "xml");
        assert!(recorder.groups.is_empty());
    }

    #[test]
    fn record_groups_records_interleaved_or_out_of_order_groups() {
        let mut recorder = ElementOrderRecorder::default();
        recorder.record_groups("one", "fp", &keys(&["a", "b", "a"]), "xml");
        recorder.record_groups("two", "fp", &keys(&["b", "a"]), "xml");
        assert_eq!(
            recorder.groups["one"],
            GroupRecord {
                tag: "fp".into(),
                runs: vec![run("a", 1), run("b", 1), run("a", 1)],
            }
        );
        assert_eq!(recorder.groups["two"].runs, [run("b", 1), run("a", 1)]);
    }

    #[test]
    fn record_groups_compares_file_names_not_bare_keys() {
        // `a.fp-meta.xml` sorts after `a-b.fp-meta.xml` ('.' > '-'), though "a" < "a-b".
        let mut recorder = ElementOrderRecorder::default();
        recorder.record_groups("fp", "fp", &keys(&["a-b", "a"]), "xml");
        assert!(recorder.groups.is_empty());
        recorder.record_groups("fp", "fp", &keys(&["a", "a-b"]), "xml");
        assert_eq!(recorder.groups["fp"].runs, [run("a", 1), run("a-b", 1)]);
    }

    fn queues(groups: &[(&str, &[i64])]) -> BTreeMap<String, VecDeque<Value>> {
        groups
            .iter()
            .map(|(key, items)| (key.to_string(), items.iter().map(|&i| i.into()).collect()))
            .collect()
    }

    #[test]
    fn interleave_follows_runs_then_appends_the_rest() {
        let record = GroupRecord {
            tag: "fp".into(),
            runs: vec![run("b", 1), run("a", 2), run("gone", 1), run("b", 2)],
        };
        // `a` holds one more item than recorded, `b` one fewer, `c` is new.
        let out = interleave(
            &record,
            queues(&[("a", &[1, 2, 3]), ("b", &[4, 5]), ("c", &[6])]),
        );
        assert_eq!(out, [4, 1, 2, 5, 3, 6].map(Value::from));
    }

    #[tokio::test]
    async fn group_records_round_trip_through_the_order_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = normalize_path_unix(&tmp.path().to_string_lossy());
        let mut recorder = ElementOrderRecorder::default();
        recorder.record_groups("fp", "fp", &keys(&["b", "a"]), "xml");
        recorder.write(&root).await;
        assert!(!tmp.path().join(ELEMENT_ORDER_FILE).exists());

        let order = ElementOrder::load(&root).await.expect("group order loads");
        assert_eq!(
            order.group_for(&format!("{root}/fp")).map(|g| &g.runs),
            Some(&vec![run("b", 1), run("a", 1)])
        );
        assert!(order.group_for(&format!("{root}/other")).is_none());
        assert!(order.group_for("/elsewhere/fp").is_none());

        ElementOrderRecorder::default().write(&root).await;
        assert!(!tmp.path().join(GROUP_ORDER_FILE).exists());
        assert!(ElementOrder::load(&root).await.is_none());
    }
}
