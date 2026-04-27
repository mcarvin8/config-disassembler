mod parse_element;
mod parse_to_xml_object;
mod parse_unique_id;
mod parse_xml;
mod parse_xml_cdata;
mod strip_whitespace;

pub use parse_element::parse_element_unified;
pub use parse_to_xml_object::parse_to_xml_object;
pub use parse_unique_id::parse_unique_id_element;
pub use parse_xml::{
    extract_xml_declaration_from_raw, extract_xmlns_from_raw, parse_xml, parse_xml_from_str,
};
pub use strip_whitespace::strip_whitespace_text_nodes;
