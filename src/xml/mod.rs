//! XML Disassembler - Disassemble large XML files into smaller files and reassemble the original XML.

pub mod builders;
pub mod cli;
pub mod constants;
pub mod handlers;
pub mod multi_level;
pub mod parsers;
pub mod transformers;
pub mod types;
pub mod utils;

pub use builders::build_xml_string;
pub use handlers::{DisassembleXmlFileHandler, ReassembleXmlFileHandler};
pub use multi_level::{
    load_multi_level_config, path_segment_from_file_pattern, save_multi_level_config,
    strip_root_and_build_xml,
};
pub use parsers::parse_xml;
pub use transformers::{transform_to_json, transform_to_json5, transform_to_yaml};
pub use types::{DecomposeRule, MultiLevelConfig, MultiLevelRule, XmlElement};
