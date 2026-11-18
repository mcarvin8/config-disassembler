//! How an extracted sidecar element is written back, so it comes back as the text it was.
//!
//! A sidecar holds the element's content converted to the sidecar's format (a JSON `<schema>`
//! stored as YAML, for example). Reassembly converts it back, which used to always produce pretty
//! JSON written with raw `"` quotes, while Salesforce retrieves compact JSON with `&quot;`. The
//! data survived; the bytes didn't.
//!
//! Disassembly now records the [`SourceForm`] that turns the sidecar back into the element's
//! source text, in `.sidecars.json` next to each spec (`json_layout`, `escape`), checked by
//! simulating the reassembly. An element no form reproduces stays in the XML instead. Both fields
//! default to the earlier behavior, so directories written before them reassemble as they did.

use crate::xml::types::SidecarSpec;
use quick_xml::escape::{escape, partial_escape};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use tokio::fs;

pub(crate) const SIDECARS_FILE: &str = ".sidecars.json";

/// Layout of JSON content converted back from its sidecar.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum JsonLayout {
    #[default]
    Pretty,
    Compact,
}

/// Which characters of the content are written as entities.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TextEscape {
    /// Only what XML requires (`&`, `<`, `>`): quotes stay raw.
    #[default]
    Raw,
    /// Quotes too (`&quot;`, `&apos;`), as Salesforce writes them.
    Entities,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceForm {
    pub(crate) json_layout: JsonLayout,
    pub(crate) escape: TextEscape,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// One `.sidecars.json` entry: the spec plus the source form, omitted when it is the default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RecordedSidecar {
    #[serde(flatten)]
    pub(crate) spec: SidecarSpec,
    #[serde(default, skip_serializing_if = "is_default")]
    pub(crate) json_layout: JsonLayout,
    #[serde(default, skip_serializing_if = "is_default")]
    pub(crate) escape: TextEscape,
}

impl RecordedSidecar {
    pub(crate) fn new(spec: SidecarSpec, form: SourceForm) -> Self {
        Self {
            spec,
            json_layout: form.json_layout,
            escape: form.escape,
        }
    }

    pub(crate) fn form(&self) -> SourceForm {
        SourceForm {
            json_layout: self.json_layout,
            escape: self.escape,
        }
    }
}

/// Read `<dir>/.sidecars.json`. `None` when absent or unreadable.
pub(crate) async fn read_recorded_sidecars(dir: &str) -> Option<Vec<RecordedSidecar>> {
    let content = fs::read_to_string(Path::new(dir).join(SIDECARS_FILE))
        .await
        .ok()?;
    serde_json::from_str(&content).ok()
}

/// Convert sidecar `content` back to the element's original format (`"json"` / `"yaml"`).
///
/// - `json`: parsed as YAML (a superset of JSON), written in `layout`
/// - `yaml` / `yml`: converted only when the content is strict JSON; YAML passes through
/// - anything else, or no original format: passes through unchanged
///
/// Falls back to the raw content, with a warning, when it can't be converted.
pub(crate) fn restore_content(
    content: &str,
    original_format: Option<&str>,
    layout: JsonLayout,
) -> String {
    match original_format.map(str::to_ascii_lowercase).as_deref() {
        Some("json") => match serde_saphyr::from_str::<Value>(content) {
            // Serializing a serde_json::Value cannot fail.
            Ok(val) => match layout {
                JsonLayout::Pretty => serde_json::to_string_pretty(&val),
                JsonLayout::Compact => serde_json::to_string(&val),
            }
            .unwrap_or_else(|_| content.to_string()),
            Err(e) => {
                log::warn!("sidecar reassemble: could not parse content for JSON conversion ({e}); using raw content");
                content.to_string()
            }
        },
        Some("yaml") | Some("yml") => match serde_json::from_str::<Value>(content) {
            Ok(val) => serde_saphyr::to_string(&val).unwrap_or_else(|_| {
                log::warn!("sidecar reassemble: YAML serialization failed; using raw content");
                content.to_string()
            }),
            Err(_) => content.to_string(),
        },
        _ => content.to_string(),
    }
}

/// The element value reassembly injects for restored `text`. `#text` is written with full
/// escaping (quotes as entities); `#raw-text` with only the escaping XML requires.
pub(crate) fn element_value(text: String, escape: TextEscape) -> Value {
    let key = match escape {
        TextEscape::Entities => "#text",
        TextEscape::Raw => "#raw-text",
    };
    serde_json::json!({ key: text })
}

/// `text` escaped the way the writer escapes [`element_value`]'s value.
fn written_text(text: &str, escape_style: TextEscape) -> String {
    match escape_style {
        TextEscape::Entities => escape(text).into_owned(),
        TextEscape::Raw => partial_escape(text).into_owned(),
    }
}

/// The form that turns `sidecar_content` back into `source` (the element's text exactly as it
/// appears in the XML), or `None` when no form does.
pub(crate) fn find_source_form(
    sidecar_content: &str,
    original_format: Option<&str>,
    source: &str,
) -> Option<SourceForm> {
    // The layout only matters for JSON; for anything else both restore the same text, so the
    // default (pretty) is found first.
    [JsonLayout::Pretty, JsonLayout::Compact]
        .into_iter()
        .find_map(|json_layout| {
            let restored = restore_content(sidecar_content, original_format, json_layout);
            [TextEscape::Raw, TextEscape::Entities]
                .into_iter()
                .find(|&escape| written_text(&restored, escape) == source)
                .map(|escape| SourceForm {
                    json_layout,
                    escape,
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SidecarSpec {
        SidecarSpec {
            element: "schema".into(),
            extension: "yaml".into(),
            original_format: Some("json".into()),
        }
    }

    #[test]
    fn finds_compact_json_with_entity_quotes() {
        let yaml = "a: 1\nb:\n- x\n";
        let form = find_source_form(
            yaml,
            Some("json"),
            r#"{&quot;a&quot;:1,&quot;b&quot;:[&quot;x&quot;]}"#,
        );
        assert_eq!(
            form,
            Some(SourceForm {
                json_layout: JsonLayout::Compact,
                escape: TextEscape::Entities
            })
        );
    }

    #[test]
    fn finds_the_default_form_for_pretty_json_with_raw_quotes() {
        let yaml = "a: 1\n";
        let source = "{\n  \"a\": 1\n}";
        assert_eq!(
            find_source_form(yaml, Some("json"), source),
            Some(SourceForm::default())
        );
    }

    #[test]
    fn finds_pretty_json_with_entity_quotes_and_compact_with_raw_quotes() {
        let yaml = "a: 1\n";
        assert_eq!(
            find_source_form(yaml, Some("json"), "{\n  &quot;a&quot;: 1\n}"),
            Some(SourceForm {
                json_layout: JsonLayout::Pretty,
                escape: TextEscape::Entities
            })
        );
        assert_eq!(
            find_source_form(yaml, Some("json"), r#"{"a":1}"#),
            Some(SourceForm {
                json_layout: JsonLayout::Compact,
                escape: TextEscape::Raw
            })
        );
    }

    #[test]
    fn yaml_and_raw_content_only_vary_by_escaping() {
        assert_eq!(
            find_source_form("k: 'v'\n", Some("yaml"), "k: &apos;v&apos;\n"),
            Some(SourceForm {
                json_layout: JsonLayout::Pretty,
                escape: TextEscape::Entities
            })
        );
        assert_eq!(
            find_source_form("a < b", None, "a &lt; b"),
            Some(SourceForm::default())
        );
    }

    #[test]
    fn no_form_reproduces_changed_content() {
        // `1.50` comes back as `1.5`; spacing after `:` is lost.
        assert_eq!(
            find_source_form("a: 1.50\n", Some("json"), r#"{"a":1.50}"#),
            None
        );
        assert_eq!(
            find_source_form("a: 1\n", Some("json"), r#"{"a": 1}"#),
            None
        );
    }

    #[test]
    fn restores_each_format() {
        assert_eq!(
            restore_content("a: 1\n", Some("json"), JsonLayout::Compact),
            r#"{"a":1}"#
        );
        assert_eq!(
            restore_content("a: 1\n", Some("JSON"), JsonLayout::Pretty),
            "{\n  \"a\": 1\n}"
        );
        assert_eq!(
            restore_content(r#"{"a":1}"#, Some("yaml"), JsonLayout::Pretty),
            "a: 1\n"
        );
        assert_eq!(
            restore_content(r#"{"a":1}"#, Some("yml"), JsonLayout::Pretty),
            "a: 1\n"
        );
        assert_eq!(
            restore_content("a: 1\n", Some("yaml"), JsonLayout::Pretty),
            "a: 1\n"
        );
        assert_eq!(
            restore_content("{bad", Some("json"), JsonLayout::Pretty),
            "{bad"
        );
        assert_eq!(restore_content("x", None, JsonLayout::Compact), "x");
    }

    #[test]
    fn element_value_picks_the_writer_key() {
        assert_eq!(
            element_value("t".into(), TextEscape::Entities),
            serde_json::json!({ "#text": "t" })
        );
        assert_eq!(
            element_value("t".into(), TextEscape::Raw),
            serde_json::json!({ "#raw-text": "t" })
        );
    }

    #[test]
    fn recorded_sidecars_omit_default_forms_and_read_old_entries() {
        let default = RecordedSidecar::new(spec(), SourceForm::default());
        assert_eq!(
            serde_json::to_string(&default).unwrap(),
            r#"{"element":"schema","extension":"yaml","original_format":"json"}"#
        );
        let form = SourceForm {
            json_layout: JsonLayout::Compact,
            escape: TextEscape::Entities,
        };
        let json = serde_json::to_string(&RecordedSidecar::new(spec(), form)).unwrap();
        assert_eq!(
            json,
            r#"{"element":"schema","extension":"yaml","original_format":"json","json_layout":"compact","escape":"entities"}"#
        );
        let read: Vec<RecordedSidecar> = serde_json::from_str(&format!("[{json}]")).unwrap();
        assert_eq!(read[0].form(), form);
        assert_eq!(read[0].spec.element, "schema");

        // Written before the form fields existed.
        let old: Vec<RecordedSidecar> =
            serde_json::from_str(r#"[{"element":"schema","extension":"yaml"}]"#).unwrap();
        assert_eq!(old[0].form(), SourceForm::default());
        assert_eq!(old[0].spec.original_format, None);
    }

    #[tokio::test]
    async fn reads_recorded_sidecars_from_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        assert!(read_recorded_sidecars(dir).await.is_none());
        std::fs::write(tmp.path().join(SIDECARS_FILE), "not json").unwrap();
        assert!(read_recorded_sidecars(dir).await.is_none());
        std::fs::write(
            tmp.path().join(SIDECARS_FILE),
            r#"[{"element":"schema","extension":"yaml","escape":"entities"}]"#,
        )
        .unwrap();
        let read = read_recorded_sidecars(dir).await.unwrap();
        assert_eq!(read[0].escape, TextEscape::Entities);
    }
}
