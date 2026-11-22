//! Strict XML 1.0 parser and content extractor built over `quick-xml` 0.42.

use std::borrow::Cow;

use quick_xml::XmlVersion;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::{BytesEnd, BytesRef, BytesStart, Event};
use quick_xml::reader::Reader;

/// Metadata parsed from `<context_summary ... />`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedContextSummary {
    pub(crate) root: String,
    pub(crate) root_path: Option<String>,
    pub(crate) included_files: usize,
    pub(crate) unselected_files: usize,
    pub(crate) discarded_files: usize,
    pub(crate) pruned_dirs: Option<String>,
}

/// Extracted file entry parsed back from a strict `<cpdir_context>` XML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedXmlFile {
    pub(crate) path: String,
    pub(crate) content: String,
}

/// Structured representation of a validated `<cpdir_context>` XML payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedCpdirXml {
    pub(crate) context_summary: Option<ParsedContextSummary>,
    pub(crate) directory_structure: Option<String>,
    pub(crate) has_git_context: bool,
    pub(crate) files: Vec<ParsedXmlFile>,
}

/// Parses and validates strict `cpdir` XML, resolving all entities without data loss.
pub(crate) fn parse_strict_xml(xml: &str) -> Result<ParsedCpdirXml, String> {
    let mut reader = Reader::from_str(xml);
    let mut state = XmlParseState::default();

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(event) => state.handle_event(&event)?,
            Err(err) => {
                let pos = reader.error_position();
                return Err(format!("xml parse error at offset {pos}: {err}"));
            }
        }
    }

    state.finish()
}

#[derive(Default)]
struct XmlParseState {
    stack: Vec<String>,
    saw_root: bool,
    context_summary: Option<ParsedContextSummary>,
    directory_structure: Option<String>,
    has_git_context: bool,
    files: Vec<ParsedXmlFile>,
    active_file: Option<(String, String)>,
    active_dir_tree: Option<String>,
}

impl XmlParseState {
    fn handle_event(&mut self, event: &Event<'_>) -> Result<(), String> {
        match event {
            Event::Start(tag) => self.on_start(tag)?,
            Event::Empty(tag) => self.on_empty(tag)?,
            Event::End(tag) => self.on_end(tag)?,
            Event::Text(text) => {
                let decoded = text.xml_content(XmlVersion::Implicit1_0);
                self.append_active_text(&decoded);
            }
            Event::CData(cdata) => {
                let decoded = cdata.xml_content(XmlVersion::Implicit1_0);
                self.append_active_text(&decoded);
            }
            Event::GeneralRef(entity) => {
                let resolved = resolve_entity_ref(entity)?;
                self.append_active_text(&resolved);
            }
            _ => {}
        }
        Ok(())
    }

    fn on_start(&mut self, tag: &BytesStart<'_>) -> Result<(), String> {
        let name = tag.name().as_ref().to_owned();
        if self.stack.is_empty() {
            if name != "cpdir_context" {
                return Err(format!("unexpected root element: <{name}>"));
            }
            self.saw_root = true;
        }
        if self.active_file.is_some() {
            return Err(format!("unexpected nested tag <{name}> inside <file>"));
        }
        match name.as_str() {
            "context_summary" => self.context_summary = Some(parse_context_summary(tag)?),
            "directory_structure" => self.active_dir_tree = Some(String::new()),
            "git_context" => self.has_git_context = true,
            "file" => {
                let path = extract_required_attr(tag, "path")?;
                self.active_file = Some((path, String::new()));
            }
            _ => {}
        }
        self.stack.push(name);
        Ok(())
    }

    fn on_empty(&mut self, tag: &BytesStart<'_>) -> Result<(), String> {
        let qname = tag.name();
        let name = qname.as_ref();
        if self.active_file.is_some() {
            return Err(format!("unexpected empty tag <{name}/> inside <file>"));
        }
        match name {
            "context_summary" => self.context_summary = Some(parse_context_summary(tag)?),
            "file" => {
                let path = extract_required_attr(tag, "path")?;
                self.files.push(ParsedXmlFile {
                    path,
                    content: String::new(),
                });
            }
            "git_context" => self.has_git_context = true,
            "directory_structure" => self.directory_structure = Some(String::new()),
            _ => {}
        }
        Ok(())
    }

    fn on_end(&mut self, tag: &BytesEnd<'_>) -> Result<(), String> {
        let qname = tag.name();
        let name = qname.as_ref();
        let popped = self
            .stack
            .pop()
            .ok_or_else(|| format!("unexpected closing tag </{name}>"))?;
        if popped != name {
            return Err(format!("mismatched closing tag </{name}> for <{popped}>"));
        }
        if name == "file"
            && let Some((path, content)) = self.active_file.take()
        {
            self.files.push(ParsedXmlFile { path, content });
        } else if name == "directory_structure" {
            self.directory_structure = self.active_dir_tree.take();
        }
        Ok(())
    }

    fn append_active_text(&mut self, chunk: &str) {
        if let Some((_, buf)) = self.active_file.as_mut() {
            buf.push_str(chunk);
        } else if let Some(tree_buf) = self.active_dir_tree.as_mut() {
            tree_buf.push_str(chunk);
        }
    }

    fn finish(self) -> Result<ParsedCpdirXml, String> {
        if !self.saw_root {
            return Err("missing <cpdir_context> root element".to_owned());
        }
        if !self.stack.is_empty() {
            return Err(format!("unclosed xml elements remaining: {:?}", self.stack));
        }
        Ok(ParsedCpdirXml {
            context_summary: self.context_summary,
            directory_structure: self.directory_structure,
            has_git_context: self.has_git_context,
            files: self.files,
        })
    }
}

fn parse_context_summary(tag: &BytesStart<'_>) -> Result<ParsedContextSummary, String> {
    Ok(ParsedContextSummary {
        root: extract_required_attr(tag, "root")?,
        root_path: extract_optional_attr(tag, "root_path")?,
        included_files: parse_usize_attr(tag, "included_files")?,
        unselected_files: parse_usize_attr(tag, "unselected_files")?,
        discarded_files: parse_usize_attr(tag, "discarded_files")?,
        pruned_dirs: extract_optional_attr(tag, "pruned_dirs")?,
    })
}

fn parse_usize_attr(tag: &BytesStart<'_>, key: &str) -> Result<usize, String> {
    let raw = extract_required_attr(tag, key)?;
    raw.parse::<usize>()
        .map_err(|err| format!("invalid integer attribute '{key}'='{raw}': {err}"))
}

fn extract_required_attr(tag: &BytesStart<'_>, key: &str) -> Result<String, String> {
    if let Some(val) = extract_optional_attr(tag, key)? {
        return Ok(val);
    }
    let qname = tag.name();
    let tag_name = qname.as_ref();
    Err(format!(
        "missing required attribute '{key}' on <{tag_name}>"
    ))
}

fn extract_optional_attr(tag: &BytesStart<'_>, key: &str) -> Result<Option<String>, String> {
    for attr_res in tag.attributes() {
        let attr = attr_res.map_err(|err| format!("invalid xml attribute: {err}"))?;
        if attr.key.as_ref() == key {
            let val = attr
                .normalized_value(XmlVersion::Implicit1_0)
                .map(Cow::into_owned)
                .map_err(|err| format!("failed to decode attribute {key}: {err}"))?;
            return Ok(Some(val));
        }
    }
    Ok(None)
}

fn resolve_entity_ref(entity: &BytesRef<'_>) -> Result<String, String> {
    let raw_str: &str = entity.as_ref();
    if let Some(hex) = raw_str.strip_prefix("#x") {
        let code = u32::from_str_radix(hex, 16)
            .map_err(|err| format!("invalid hex char ref &#x{hex};: {err}"))?;
        let ch =
            char::from_u32(code).ok_or_else(|| format!("out-of-range unicode scalar &#x{hex};"))?;
        return Ok(ch.to_string());
    }
    if let Some(dec) = raw_str.strip_prefix('#') {
        let code = dec
            .parse::<u32>()
            .map_err(|err| format!("invalid decimal char ref &#{dec};: {err}"))?;
        let ch =
            char::from_u32(code).ok_or_else(|| format!("out-of-range unicode scalar &#{dec};"))?;
        return Ok(ch.to_string());
    }
    resolve_xml_entity(raw_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("unknown xml entity &{raw_str};"))
}
