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
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use tokio::fs;

pub(crate) const ELEMENT_ORDER_FILE: &str = ".element_order.json";

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
}

impl ElementOrderRecorder {
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
            return;
        }
        // Pretty-printed, one path per line, so a reorder shows up as a small diff.
        if let Ok(json) = serde_json::to_string_pretty(&self.by_tag) {
            let _ = fs::write(path, json + "\n").await;
        }
    }
}

/// The recorded order for one disassembled directory, loaded at reassembly.
#[derive(Debug)]
pub(crate) struct ElementOrder {
    root: String,
    ranks: HashMap<String, usize>,
}

impl ElementOrder {
    /// Load `<disassembled_path>/.element_order.json`. `None` when it is absent or
    /// unreadable, which keeps filename order for the whole directory.
    pub(crate) async fn load(disassembled_path: &str) -> Option<Self> {
        let bytes = fs::read(Path::new(disassembled_path).join(ELEMENT_ORDER_FILE))
            .await
            .ok()?;
        let by_tag: BTreeMap<String, Vec<String>> = serde_json::from_slice(&bytes).ok()?;
        let mut ranks = HashMap::new();
        for paths in by_tag.values() {
            for (index, rel) in paths.iter().enumerate() {
                let (rel_dir, name) = rel.rsplit_once('/').unwrap_or(("", rel));
                ranks.entry(rank_key(rel_dir, name)).or_insert(index);
            }
        }
        Some(Self {
            root: normalize_path_unix(disassembled_path),
            ranks,
        })
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
        };
        let mut names = vec!["a.t-meta.xml", "c.t-meta.xml", "b.t-meta.xml"];
        names.sort_by_cached_key(|n| entry_sort_key(Some(&order), "/r/t", n.to_string()));
        assert_eq!(names, ["b.t-meta.xml", "a.t-meta.xml", "c.t-meta.xml"]);
        names.sort_by_cached_key(|n| entry_sort_key(None, "/r/t", n.to_string()));
        assert_eq!(names, ["a.t-meta.xml", "b.t-meta.xml", "c.t-meta.xml"]);
    }
}
