//! Detect XML comments that disassembly can't keep in place.
//!
//! The parsed tree stores an element's comments as one `#comment` string, written after the
//! element's children and text. That keeps comments that already sit there --
//! `<a><b>1</b><!-- c --></a>` or `<a>text<!-- c --></a>` (several there are merged into one, but
//! their text is kept) -- but not:
//! - a comment followed by a child element: it would move after the children;
//! - a comment directly in the root element: the root's `#comment` goes through the leaf path
//!   and is written as JSON text, corrupting the file;
//! - a comment outside the root element: the parser drops it.
//!
//! Files with such comments are left alone rather than disassembled.

use quick_xml::events::Event;
use quick_xml::Reader;

/// Whether `xml` has a comment disassembly can't keep in place (see the module docs). Cheap for
/// the common case: without `<!--` there is nothing to scan.
pub fn has_unplaceable_comments(xml: &str) -> bool {
    if !xml.contains("<!--") {
        return false;
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    // One entry per open element: whether a comment has been seen in it.
    let mut open: Vec<bool> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) | Ok(Event::Empty(_))
                // A child element after a comment in the same parent.
                if open.last() == Some(&true) =>
            {
                return true;
            }
            Ok(Event::Start(_)) => open.push(false),
            Ok(Event::Comment(_)) => match open.len() {
                // Outside the root element, or directly in it.
                0 | 1 => return true,
                _ => {
                    if let Some(seen) = open.last_mut() {
                        *seen = true;
                    }
                }
            },
            Ok(Event::End(_)) => {
                open.pop();
            }
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
    fn comment_after_children_or_in_text_is_placeable() {
        assert!(!has_unplaceable_comments(
            "<r><a><b>1</b><!-- after --></a><c>text<!-- c -->more</c><d><!-- only --></d></r>"
        ));
    }

    #[test]
    fn comment_before_a_later_child_is_unplaceable() {
        assert!(has_unplaceable_comments(
            "<r><labels><fullName>x</fullName><!-- c --><value>v</value></labels></r>"
        ));
        assert!(has_unplaceable_comments("<r><a><!-- c --><b>1</b></a></r>"));
        assert!(has_unplaceable_comments("<r><a><!-- c --><b/></a></r>"));
    }

    #[test]
    fn several_comments_after_the_children_are_placeable() {
        // Merged into one comment on write, but their text is kept.
        assert!(!has_unplaceable_comments(
            "<r><a><b>1</b><!-- one --><!-- two --></a></r>"
        ));
    }

    #[test]
    fn comment_directly_in_root_is_unplaceable() {
        assert!(has_unplaceable_comments(
            "<r><a>1</a><!-- c --><a>2</a></r>"
        ));
        assert!(has_unplaceable_comments("<r><a>1</a><!-- after --></r>"));
    }

    #[test]
    fn comment_outside_root_is_unplaceable() {
        assert!(has_unplaceable_comments(
            r#"<?xml version="1.0"?><!-- prolog --><r><a>1</a></r>"#
        ));
        assert!(has_unplaceable_comments("<r><a>1</a></r><!-- epilogue -->"));
    }

    #[test]
    fn malformed_xml_is_left_to_the_parser() {
        assert!(!has_unplaceable_comments("<r><a><!-- c --></r>"));
    }
}
