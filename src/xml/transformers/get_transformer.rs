//! Get transformer for format - returns output string from XmlElement.

use crate::xml::transformers::{transform_to_json, transform_to_json5, transform_to_yaml};
use crate::xml::types::XmlElement;

/// Transform XmlElement to string in the given format.
/// Returns None if format is not supported (e.g. "xml" uses build_xml_string instead).
pub async fn transform_format(format: &str, xml_content: &XmlElement) -> Option<String> {
    let result = match format.to_lowercase().as_str() {
        "yaml" | "yml" => transform_to_yaml(xml_content).await,
        "json5" => transform_to_json5(xml_content).await,
        "json" => transform_to_json(xml_content).await,
        _ => return None,
    };
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn transform_format_yaml() {
        let el = json!({ "r": {} });
        assert!(transform_format("yaml", &el).await.is_some());
        assert!(transform_format("yml", &el).await.is_some());
    }

    #[tokio::test]
    async fn transform_format_json() {
        let el = json!({ "r": {} });
        assert!(transform_format("json", &el).await.is_some());
    }

    #[tokio::test]
    async fn transform_format_json5() {
        let el = json!({ "r": {} });
        assert!(transform_format("json5", &el).await.is_some());
    }

    #[tokio::test]
    async fn transform_format_unsupported_returns_none() {
        let el = json!({ "r": {} });
        assert!(transform_format("xml", &el).await.is_none());
        assert!(transform_format("unknown", &el).await.is_none());
    }
}
