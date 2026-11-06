//! Deterministic fixture generators shared by the benchmark targets.
//!
//! Shapes mirror sf-decomposer's `scripts/gen-perf-fixtures.ts` (the largest
//! downstream consumer of this crate), but every identifier is counter-based,
//! so no fixture bytes are checked in and every run sees identical input.

// Each bench target compiles this module separately and uses a subset of it.
#![allow(dead_code)]

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde_json::Value;

/// Unique-id fields sf-decomposer passes for `PermissionSet` (its defaults
/// `fullName,name` plus the per-type list in `src/metadata/uniqueIdElements.ts`).
pub const PERMISSION_SET_UNIQUE_IDS: &str =
    "fullName,name,application,apexClass,externalDataSource,flow,object,apexPage,recordType,tab,field";

/// Path for an ignore file that never exists, so every disassemble call takes
/// the same "no ignore rules" path.
pub const NO_IGNORE_FILE: &str = ".config-disassembler-bench-ignore";

const XML_HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
const NS: &str = " xmlns=\"http://soap.sforce.com/2006/04/metadata\"";

/// A permission set whose sibling counts all scale with `n`. `n` is the number
/// of `<fieldPermissions>` (the dominant sibling list in real orgs); the other
/// lists are fixed fractions of it, so doubling `n` doubles the whole document.
pub fn permission_set_xml(n: usize) -> String {
    let frac = |d: usize| (n / d).max(1);
    let mut s = String::with_capacity(n * 200);
    s.push_str(XML_HEADER);
    let _ = writeln!(s, "<PermissionSet{NS}>");

    for i in 1..=frac(20) {
        let _ = write!(
            s,
            "    <applicationVisibilities>\n        <application>Sample_App_{i:04}</application>\n        <visible>{}</visible>\n    </applicationVisibilities>\n",
            i % 2 == 0
        );
    }
    for i in 1..=frac(4) {
        let _ = write!(
            s,
            "    <classAccesses>\n        <apexClass>Sample_Class_{i:05}</apexClass>\n        <enabled>true</enabled>\n    </classAccesses>\n"
        );
    }
    for i in 1..=n {
        let obj = (i % 200) + 1;
        let _ = write!(
            s,
            "    <fieldPermissions>\n        <editable>true</editable>\n        <field>Sample_Object_{obj:03}__c.Sample_Field_{i:06}__c</field>\n        <readable>true</readable>\n    </fieldPermissions>\n"
        );
    }
    s.push_str("    <description>Synthetic permission set for benchmarking.</description>\n");
    s.push_str("    <hasActivationRequired>false</hasActivationRequired>\n");
    s.push_str("    <label>Bench Permission Set</label>\n");
    s.push_str("    <license>Salesforce</license>\n");
    for i in 1..=frac(10) {
        let _ = write!(
            s,
            "    <objectPermissions>\n        <allowCreate>true</allowCreate>\n        <allowDelete>true</allowDelete>\n        <allowEdit>true</allowEdit>\n        <allowRead>true</allowRead>\n        <modifyAllRecords>false</modifyAllRecords>\n        <object>Sample_Object_{i:03}__c</object>\n        <viewAllRecords>false</viewAllRecords>\n    </objectPermissions>\n"
        );
    }
    for i in 1..=frac(10) {
        let _ = write!(
            s,
            "    <pageAccesses>\n        <apexPage>Sample_Page_{i:05}</apexPage>\n        <enabled>true</enabled>\n    </pageAccesses>\n"
        );
    }
    for i in 1..=frac(20) {
        let obj = (i % 50) + 1;
        let _ = write!(
            s,
            "    <recordTypeVisibilities>\n        <recordType>Sample_Object_{obj:03}__c.Sample_Record_Type_{i:04}</recordType>\n        <visible>true</visible>\n    </recordTypeVisibilities>\n"
        );
    }
    for i in 1..=frac(20) {
        let _ = write!(
            s,
            "    <tabSettings>\n        <tab>Sample_Tab_{i:04}</tab>\n        <visibility>Available</visibility>\n    </tabSettings>\n"
        );
    }
    s.push_str("    <userPermissions>\n        <enabled>true</enabled>\n        <name>ApiEnabled</name>\n    </userPermissions>\n");
    s.push_str("</PermissionSet>\n");
    s
}

/// `n` small shard files' contents, shaped like what `disassemble` writes for a
/// permission set's `<fieldPermissions>` (declaration + root + one element).
/// Reassemble parses thousands of these per component, so per-shard fixed
/// costs dominate there in a way whole-document benches can't show.
pub fn field_permission_shards(n: usize) -> Vec<String> {
    (1..=n)
        .map(|i| {
            let obj = (i % 200) + 1;
            format!(
                "{XML_HEADER}<PermissionSet{NS}>\n    <fieldPermissions>\n        <editable>true</editable>\n        <field>Sample_Object_{obj:03}__c.Sample_Field_{i:06}__c</field>\n        <readable>true</readable>\n    </fieldPermissions>\n</PermissionSet>\n"
            )
        })
        .collect()
}

/// Writes `files` permission sets of size `n` into `dir` -- the many-small-files
/// shape that exercises the directory-mode concurrent fan-out in
/// `DisassembleXmlFileHandler::handle_directory`.
pub fn write_many_files(dir: &Path, files: usize, n: usize) {
    fs::create_dir_all(dir).expect("create fixture dir");
    let xml = permission_set_xml(n);
    for i in 1..=files {
        fs::write(dir.join(format!("PS_{i:04}.permissionset-meta.xml")), &xml)
            .expect("write fixture file");
    }
}

/// Recursively copies `src` into `dst` (std-only, so it can run in criterion's
/// synchronous setup closures even when the timed routine is async).
pub fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create copy dir");
    for entry in fs::read_dir(src).expect("read fixture dir") {
        let entry = entry.expect("dir entry");
        let target = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy fixture file");
        }
    }
}

/// The longest array of sibling elements anywhere in a parsed document. For
/// `permission_set_xml(n)` that is the `n` `<fieldPermissions>` entries.
pub fn largest_sibling_array(value: &Value) -> &[Value] {
    fn walk<'a>(v: &'a Value, best: &mut &'a [Value]) {
        match v {
            Value::Array(items) => {
                if items.len() > best.len() {
                    *best = items.as_slice();
                }
                items.iter().for_each(|i| walk(i, best));
            }
            Value::Object(map) => map.values().for_each(|i| walk(i, best)),
            _ => {}
        }
    }
    let mut best: &[Value] = &[];
    walk(value, &mut best);
    best
}
