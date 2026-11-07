mod build_disassembled_file;
mod build_disassembled_files;
mod build_xml_string;
mod extract_root_attributes;
mod merge_xml_elements;

pub use build_disassembled_file::build_disassembled_file;
pub(crate) use build_disassembled_file::{render_disassembled_file, write_shards, RenderedShard};
pub use build_disassembled_files::build_disassembled_files_unified;
pub use build_xml_string::build_xml_string;
pub use extract_root_attributes::extract_root_attributes;
pub use merge_xml_elements::{
    merge_xml_elements, merge_xml_elements_owned, reorder_root_keys, reorder_root_keys_owned,
};
