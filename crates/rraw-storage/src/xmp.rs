#![forbid(unsafe_code)]

//! Non-destructive XMP XML parser and editor preserving third-party namespaces.

use crate::error::StorageError;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use rraw_core::ColorLabel;
use std::io::Cursor;

/// Non-destructive XMP metadata editor preserving third-party namespaces and XML nodes.
#[derive(Debug, Clone)]
pub struct XmpEditor {
    original_xml: String,
    rating: Option<u8>,
    label: Option<ColorLabel>,
    keywords: Vec<String>,
}

impl XmpEditor {
    /// Parses an existing XMP XML document, extracting standard photographic metadata.
    pub fn parse_str(xml: &str) -> Result<Self, StorageError> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut rating = None;
        let mut label = None;
        let mut keywords = Vec::new();

        let mut in_rating = false;
        let mut in_label = false;
        let mut in_subject = false;
        let mut in_bag_item = false;

        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => match e.local_name().as_ref() {
                    b"Rating" => in_rating = true,
                    b"Label" => in_label = true,
                    b"subject" => in_subject = true,
                    b"li" if in_subject => in_bag_item = true,
                    _ => {}
                },
                Ok(Event::End(ref e)) => match e.local_name().as_ref() {
                    b"Rating" => in_rating = false,
                    b"Label" => in_label = false,
                    b"subject" => in_subject = false,
                    b"li" => in_bag_item = false,
                    _ => {}
                },
                Ok(Event::Text(ref e)) => {
                    let text = e
                        .unescape()
                        .map_err(|err| StorageError::Xmp(format!("Failed to unescape XML: {err}")))?
                        .into_owned();

                    if in_rating {
                        if let Ok(r) = text.trim().parse::<u8>() {
                            rating = Some(r.min(5));
                        }
                    } else if in_label {
                        label = match text.trim().to_lowercase().as_str() {
                            "red" => Some(ColorLabel::Red),
                            "yellow" => Some(ColorLabel::Yellow),
                            "green" => Some(ColorLabel::Green),
                            "blue" => Some(ColorLabel::Blue),
                            "purple" => Some(ColorLabel::Purple),
                            _ => Some(ColorLabel::None),
                        };
                    } else if in_bag_item && !text.trim().is_empty() {
                        keywords.push(text.trim().to_owned());
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(StorageError::Xmp(format!("XML parse error: {e}"))),
                _ => {}
            }
            buf.clear();
        }

        Ok(Self {
            original_xml: xml.to_owned(),
            rating,
            label,
            keywords,
        })
    }

    /// Extracted `xmp:Rating` (`0..=5`).
    #[must_use]
    pub const fn rating(&self) -> Option<u8> {
        self.rating
    }

    /// Sets `xmp:Rating` clamped to `[0, 5]`.
    pub fn set_rating(&mut self, rating: u8) {
        self.rating = Some(rating.min(5));
    }

    /// Extracted `xmp:Label`.
    #[must_use]
    pub const fn label(&self) -> Option<ColorLabel> {
        self.label
    }

    /// Sets `xmp:Label`.
    pub fn set_label(&mut self, label: ColorLabel) {
        self.label = Some(label);
    }

    /// Extracted `dc:subject` keywords.
    #[must_use]
    pub fn keywords(&self) -> &[String] {
        &self.keywords
    }

    /// Sets `dc:subject` keywords.
    pub fn set_keywords(&mut self, keywords: &[String]) {
        self.keywords = keywords.to_vec();
    }

    /// Serializes modified XMP metadata while strictly preserving all third-party XML elements.
    pub fn to_xml_string(&self) -> Result<String, StorageError> {
        let mut reader = Reader::from_str(&self.original_xml);
        let mut writer = Writer::new(Cursor::new(Vec::new()));

        let mut in_desc = false;
        let mut skip_depth = 0_usize;

        let mut handled_rating = false;
        let mut handled_label = false;
        let mut handled_subject = false;

        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let local = e.local_name();
                    if local.as_ref() == b"Description" {
                        in_desc = true;
                        writer
                            .write_event(Event::Start(e.clone()))
                            .map_err(|err| StorageError::Xmp(err.to_string()))?;
                    } else if in_desc && skip_depth == 0 {
                        match local.as_ref() {
                            b"Rating" => {
                                skip_depth = 1;
                                handled_rating = true;
                                if let Some(r) = self.rating {
                                    writer
                                        .write_event(Event::Start(BytesStart::new("xmp:Rating")))
                                        .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                    writer
                                        .write_event(Event::Text(BytesText::new(&r.to_string())))
                                        .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                    writer
                                        .write_event(Event::End(BytesEnd::new("xmp:Rating")))
                                        .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                }
                            }
                            b"Label" => {
                                skip_depth = 1;
                                handled_label = true;
                                if let Some(l) = self.label {
                                    let label_str = match l {
                                        ColorLabel::Red => "Red",
                                        ColorLabel::Yellow => "Yellow",
                                        ColorLabel::Green => "Green",
                                        ColorLabel::Blue => "Blue",
                                        ColorLabel::Purple => "Purple",
                                        ColorLabel::None => "None",
                                    };
                                    writer
                                        .write_event(Event::Start(BytesStart::new("xmp:Label")))
                                        .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                    writer
                                        .write_event(Event::Text(BytesText::new(label_str)))
                                        .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                    writer
                                        .write_event(Event::End(BytesEnd::new("xmp:Label")))
                                        .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                }
                            }
                            b"subject" => {
                                skip_depth = 1;
                                handled_subject = true;
                                self.write_subject(&mut writer)?;
                            }
                            _ => {
                                writer
                                    .write_event(Event::Start(e.clone()))
                                    .map_err(|err| StorageError::Xmp(err.to_string()))?;
                            }
                        }
                    } else if skip_depth > 0 {
                        skip_depth += 1;
                    } else {
                        writer
                            .write_event(Event::Start(e.clone()))
                            .map_err(|err| StorageError::Xmp(err.to_string()))?;
                    }
                }
                Ok(Event::End(ref e)) => {
                    let local = e.local_name();
                    if local.as_ref() == b"Description" {
                        if !handled_rating {
                            if let Some(r) = self.rating {
                                writer
                                    .write_event(Event::Start(BytesStart::new("xmp:Rating")))
                                    .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                writer
                                    .write_event(Event::Text(BytesText::new(&r.to_string())))
                                    .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                writer
                                    .write_event(Event::End(BytesEnd::new("xmp:Rating")))
                                    .map_err(|err| StorageError::Xmp(err.to_string()))?;
                            }
                        }
                        if !handled_label {
                            if let Some(l) = self.label {
                                let label_str = match l {
                                    ColorLabel::Red => "Red",
                                    ColorLabel::Yellow => "Yellow",
                                    ColorLabel::Green => "Green",
                                    ColorLabel::Blue => "Blue",
                                    ColorLabel::Purple => "Purple",
                                    ColorLabel::None => "None",
                                };
                                writer
                                    .write_event(Event::Start(BytesStart::new("xmp:Label")))
                                    .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                writer
                                    .write_event(Event::Text(BytesText::new(label_str)))
                                    .map_err(|err| StorageError::Xmp(err.to_string()))?;
                                writer
                                    .write_event(Event::End(BytesEnd::new("xmp:Label")))
                                    .map_err(|err| StorageError::Xmp(err.to_string()))?;
                            }
                        }
                        if !handled_subject && !self.keywords.is_empty() {
                            self.write_subject(&mut writer)?;
                        }
                        in_desc = false;
                        writer
                            .write_event(Event::End(e.clone()))
                            .map_err(|err| StorageError::Xmp(err.to_string()))?;
                    } else if skip_depth > 0 {
                        skip_depth -= 1;
                    } else {
                        writer
                            .write_event(Event::End(e.clone()))
                            .map_err(|err| StorageError::Xmp(err.to_string()))?;
                    }
                }
                Ok(Event::Eof) => break,
                Ok(event) => {
                    if skip_depth == 0 {
                        writer
                            .write_event(event)
                            .map_err(|err| StorageError::Xmp(err.to_string()))?;
                    }
                }
                Err(e) => return Err(StorageError::Xmp(format!("XML serialization error: {e}"))),
            }
            buf.clear();
        }

        let bytes = writer.into_inner().into_inner();
        String::from_utf8(bytes).map_err(|e| StorageError::Xmp(format!("UTF-8 error: {e}")))
    }

    fn write_subject(&self, writer: &mut Writer<Cursor<Vec<u8>>>) -> Result<(), StorageError> {
        writer
            .write_event(Event::Start(BytesStart::new("dc:subject")))
            .map_err(|e| StorageError::Xmp(e.to_string()))?;
        writer
            .write_event(Event::Start(BytesStart::new("rdf:Bag")))
            .map_err(|e| StorageError::Xmp(e.to_string()))?;
        for kw in &self.keywords {
            writer
                .write_event(Event::Start(BytesStart::new("rdf:li")))
                .map_err(|e| StorageError::Xmp(e.to_string()))?;
            writer
                .write_event(Event::Text(BytesText::new(kw)))
                .map_err(|e| StorageError::Xmp(e.to_string()))?;
            writer
                .write_event(Event::End(BytesEnd::new("rdf:li")))
                .map_err(|e| StorageError::Xmp(e.to_string()))?;
        }
        writer
            .write_event(Event::End(BytesEnd::new("rdf:Bag")))
            .map_err(|e| StorageError::Xmp(e.to_string()))?;
        writer
            .write_event(Event::End(BytesEnd::new("dc:subject")))
            .map_err(|e| StorageError::Xmp(e.to_string()))?;
        Ok(())
    }
}
