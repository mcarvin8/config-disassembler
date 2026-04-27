//! Type definitions for XML element representation.
//!
//! Uses serde_json::Value for flexible representation matching the TypeScript structure:
//! - Object with keys: element names, @attr for attributes, #text for text content, ?xml for declaration
//! - Values: string, nested object, or array of objects/strings

use serde_json::Value as JsonValue;

/// XmlElement is a flexible representation of XML - equivalent to TypeScript's XmlElement type.
/// Uses serde_json::Value for compatibility with quickxml_to_serde output.
pub type XmlElement = JsonValue;

/// Parameters for parsing an element during disassembly.
#[derive(Debug, Clone)]
pub struct XmlElementParams<'a> {
    pub element: XmlElement,
    pub disassembled_path: &'a str,
    pub unique_id_elements: Option<&'a str>,
    pub root_element_name: &'a str,
    pub root_attributes: XmlElement,
    pub key: &'a str,
    pub leaf_content: XmlElement,
    pub leaf_count: usize,
    pub has_nested_elements: bool,
    pub format: &'a str,
    pub xml_declaration: Option<XmlElement>,
    pub strategy: &'a str,
}

/// Options for building a single disassembled file.
#[derive(Debug, Clone)]
pub struct BuildDisassembledFileOptions<'a> {
    pub content: XmlElement,
    pub disassembled_path: &'a str,
    pub output_file_name: Option<&'a str>,
    pub subdirectory: Option<&'a str>,
    pub wrap_key: Option<&'a str>,
    pub is_grouped_array: bool,
    pub root_element_name: &'a str,
    pub root_attributes: XmlElement,
    pub format: &'a str,
    pub xml_declaration: Option<XmlElement>,
    pub unique_id_elements: Option<&'a str>,
}

/// Result from unified element parsing.
#[derive(Debug, Clone, Default)]
pub struct UnifiedParseResult {
    pub leaf_content: XmlElement,
    pub leaf_count: usize,
    pub has_nested_elements: bool,
    pub nested_groups: Option<XmlElementArrayMap>,
}

/// Map of tag name to array of elements.
pub type XmlElementArrayMap = std::collections::HashMap<String, Vec<XmlElement>>;

/// Rule for decomposing a nested tag when using grouped-by-tag strategy.
/// E.g. write each &lt;objectPermissions&gt; to its own file, or group &lt;fieldPermissions&gt; by object.
#[derive(Debug, Clone)]
pub struct DecomposeRule {
    /// Element tag to decompose (e.g. "objectPermissions", "fieldPermissions").
    pub tag: String,
    /// Subdirectory under disassembled path (defaults to tag if empty).
    pub path_segment: String,
    /// "split" = one file per array item (filename from field); "group" = group by field, one file per group.
    pub mode: String,
    /// Field name: for split, used for filename; for group, used to group items.
    pub field: String,
}

/// Options for building disassembled files from a source file.
#[derive(Debug, Clone)]
pub struct BuildDisassembledFilesOptions<'a> {
    pub file_path: &'a str,
    pub disassembled_path: &'a str,
    pub base_name: &'a str,
    pub post_purge: bool,
    pub format: &'a str,
    pub unique_id_elements: Option<&'a str>,
    pub strategy: &'a str,
    /// When strategy is grouped-by-tag, optionally decompose specific tags (split or group by field).
    pub decompose_rules: Option<&'a [DecomposeRule]>,
}

/// Parameters for writing leaf content.
#[derive(Debug, Clone)]
pub struct LeafWriteParams<'a> {
    pub leaf_count: usize,
    pub leaf_content: XmlElement,
    pub strategy: &'a str,
    pub key_order: Vec<String>,
    pub options: LeafWriteOptions<'a>,
}

#[derive(Debug, Clone)]
pub struct LeafWriteOptions<'a> {
    pub disassembled_path: &'a str,
    pub output_file_name: &'a str,
    pub root_element_name: &'a str,
    pub root_attributes: XmlElement,
    pub xml_declaration: Option<XmlElement>,
    pub format: &'a str,
}

/// Rule for multi-level disassembly: which files to further disassemble and how.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MultiLevelRule {
    /// File name pattern (e.g. "programProcesses-meta"); any XML file whose name contains this is processed.
    pub file_pattern: String,
    /// Root element to strip (e.g. "LoyaltyProgramSetup"); its inner content becomes the new document.
    pub root_to_strip: String,
    /// Comma-separated unique-id elements for the second-level disassembly (e.g. "parameterName,ruleName").
    pub unique_id_elements: String,
    /// Path segment under the disassembly root for reassembly (e.g. "programProcesses").
    #[serde(default)]
    pub path_segment: String,
    /// Root element name to wrap reassembled files with (defaults to root_to_strip).
    #[serde(default)]
    pub wrap_root_element: String,
    /// xmlns value for the wrap root (optional; captured from original when stripping).
    #[serde(default)]
    pub wrap_xmlns: String,
}

/// Persisted config for multi-level reassembly (stored as .multi_level.json in the disassembly root).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MultiLevelConfig {
    pub rules: Vec<MultiLevelRule>,
}
