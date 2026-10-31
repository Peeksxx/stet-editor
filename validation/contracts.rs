use std::collections::BTreeMap;
use stet::{config::Settings, document::Document, input::Bindings};

#[test]
fn editing_contract() -> anyhow::Result<()> {
    let mut doc = Document::blank("\r\n");
    doc.edit(0..0, "Stet\r\n世界", false)?;
    assert!(doc.dirty());
    assert_eq!(doc.line_text(1), "世界");
    assert_eq!(doc.boundary(6, false), 4);
    doc.undo(false);
    assert_eq!(doc.end(), 0);
    assert!(!doc.dirty());
    doc.undo(true);
    assert_eq!(doc.end(), 8);
    Ok(())
}

#[test]
fn defaults_are_valid() -> anyhow::Result<()> {
    Settings::default().validate()?;
    Bindings::new(BTreeMap::new())?;
    Ok(())
}
