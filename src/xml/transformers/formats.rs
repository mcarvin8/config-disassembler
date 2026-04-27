//! Transform XmlElement to various formats.

use crate::xml::types::XmlElement;

pub async fn transform_to_yaml(parsed_xml: &XmlElement) -> String {
    serde_yaml::to_string(parsed_xml).unwrap_or_default()
}

pub async fn transform_to_json5(parsed_xml: &XmlElement) -> String {
    serde_json::to_string_pretty(parsed_xml).unwrap_or_default()
}

pub async fn transform_to_json(parsed_xml: &XmlElement) -> String {
    serde_json::to_string_pretty(parsed_xml).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn transform_to_yaml_produces_valid_yaml() {
        let el = json!({ "root": { "a": 1, "b": "two" } });
        let out = transform_to_yaml(&el).await;
        assert!(out.contains("root:"));
        assert!(out.contains("a: 1") || out.contains("a:\n") || out.contains("a:1"));
    }

    #[tokio::test]
    async fn transform_to_json_produces_valid_json() {
        let el = json!({ "root": { "a": 1 } });
        let out = transform_to_json(&el).await;
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            parsed
                .get("root")
                .and_then(|r| r.get("a"))
                .and_then(|v| v.as_i64()),
            Some(1)
        );
    }

    #[tokio::test]
    async fn transform_to_json5_produces_valid_json5() {
        let el = json!({ "root": { "a": 1 } });
        let out = transform_to_json5(&el).await;
        let parsed: serde_json::Value = json5::from_str(&out).unwrap();
        assert_eq!(
            parsed
                .get("root")
                .and_then(|r| r.get("a"))
                .and_then(|v| v.as_i64()),
            Some(1)
        );
    }
}
