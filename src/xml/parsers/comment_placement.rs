//! Detect XML comments that disassembly can't keep in place.
//!
//! Comments inside the root element are kept: one written before an element travels with it
//! (`#comments-before`), one after an element's last child stays at the end of it
//! (`#comments-end`), and one in an element's text is part of its mixed content (`#comment`).
//! A comment *outside* the root element (before or after it) has no element to travel with: the
//! parser drops it. Files with such comments are left alone rather than disassembled.

use quick_xml::events::Event;
use quick_xml::Reader;

/// Whether `xml` has a comment outside its root element. Cheap for the common case: without
/// `<!--` there is nothing to scan.
pub fn has_unplaceable_comments(xml: &str) -> bool {
    if !xml.contains("<!--") {
        return false;
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Comment(_)) if depth == 0 => return true,
            Ok(Event::Eof) => return false,
            // Malformed XML is reported by the real parse; nothing to decide here.
            Err(_) => return false,
            Ok(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_comments_is_placeable() {
        assert!(!has_unplaceable_comments("<r><a>1</a><a>2</a></r>"));
    }

    #[test]
    fn comments_inside_the_root_are_placeable() {
        assert!(!has_unplaceable_comments(
            "<r><!-- first --><a>1</a><!-- between --><a><b>1</b><!-- end --></a><c>t<!-- c -->u</c><!-- last --></r>"
        ));
    }

    #[test]
    fn comment_outside_root_is_unplaceable() {
        assert!(has_unplaceable_comments(
            r#"<?xml version="1.0"?><!-- prolog --><r><a>1</a></r>"#
        ));
        assert!(has_unplaceable_comments("<r><a>1</a></r><!-- epilogue -->"));
    }

    #[test]
    fn self_closing_root_with_comment_after_is_unplaceable() {
        assert!(has_unplaceable_comments("<r/><!-- after -->"));
    }

    #[test]
    fn malformed_xml_is_left_to_the_parser() {
        assert!(!has_unplaceable_comments("<r><a><!-- c --></r>"));
    }
}
