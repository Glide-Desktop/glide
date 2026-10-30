use std::{collections::HashMap, fmt};

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use zbus::zvariant::{OwnedValue, Signature, Type, as_value};

use crate::model::{ItemProps, WirePixmaps, admit_pixmaps, assemble};

pub(crate) type ToolTip = (String, WirePixmaps, String, String);

pub(crate) const ICON_PIXMAP: &str = "IconPixmap";
pub(crate) const ATTENTION_PIXMAP: &str = "AttentionIconPixmap";
pub(crate) const TOOLTIP: &str = "ToolTip";

#[derive(Debug, Default)]
pub(crate) struct Wire {
    pub(crate) light: HashMap<String, OwnedValue>,
    pub(crate) icon: WirePixmaps,
    pub(crate) attention: WirePixmaps,
    pub(crate) tooltip: Option<ToolTip>,
}

impl Wire {
    pub(crate) fn is_empty(&self) -> bool {
        self.light.is_empty() && self.icon.is_empty() && self.attention.is_empty() && self.tooltip.is_none()
    }

    pub(crate) fn into_props(self) -> ItemProps {
        let tooltip = self.tooltip.map(|(_, _, title, body)| (title, body)).unwrap_or_default();
        assemble(&self.light, admit_pixmaps(self.icon), admit_pixmaps(self.attention), tooltip)
    }
}

impl Type for Wire {
    const SIGNATURE: &'static Signature = <HashMap<String, OwnedValue> as Type>::SIGNATURE;
}

struct WireVisitor;

impl<'de> Visitor<'de> for WireVisitor {
    type Value = Wire;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a dictionary of item properties")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Wire, A::Error> {
        let mut wire = Wire::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                ICON_PIXMAP => wire.icon = map.next_value::<as_value::Deserialize<'de, WirePixmaps>>()?.0,
                ATTENTION_PIXMAP => wire.attention = map.next_value::<as_value::Deserialize<'de, WirePixmaps>>()?.0,
                TOOLTIP => wire.tooltip = Some(map.next_value::<as_value::Deserialize<'de, ToolTip>>()?.0),
                _ => {
                    let value = map.next_value::<OwnedValue>()?;
                    wire.light.insert(key, value);
                }
            }
        }
        Ok(wire)
    }
}

impl<'de> Deserialize<'de> for Wire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(WireVisitor)
    }
}

#[cfg(test)]
mod tests {
    use zbus::zvariant::{LE, Value, serialized::Context, to_bytes};

    use super::*;

    fn encode(entries: Vec<(&str, Value<'_>)>) -> zbus::zvariant::serialized::Data<'static, 'static> {
        let dict: HashMap<String, Value<'_>> = entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        to_bytes(Context::new_dbus(LE, 0), &dict).expect("encodes")
    }

    fn decode(data: &zbus::zvariant::serialized::Data<'_, '_>) -> Wire {
        data.deserialize::<Wire>().expect("decodes").0
    }

    #[test]
    fn pixmaps_and_tooltip_are_decoded_typed_and_the_rest_stays_loose() {
        let data = encode(vec![
            ("Id", Value::from("app")),
            ("Menu", Value::from("/not/an/object/path")),
            (ICON_PIXMAP, Value::from(vec![(1i32, 1i32, vec![255u8, 1, 2, 3])])),
            (
                TOOLTIP,
                Value::from((
                    String::new(),
                    Vec::<(i32, i32, Vec<u8>)>::new(),
                    String::from("Title"),
                    String::from("Body"),
                )),
            ),
        ]);
        let wire = decode(&data);
        assert_eq!(wire.icon, vec![(1, 1, vec![255, 1, 2, 3])]);
        assert!(wire.attention.is_empty());
        assert!(!wire.light.contains_key(ICON_PIXMAP), "heavy properties never become loose values");
        assert!(!wire.light.contains_key(TOOLTIP));
        let props = wire.into_props();
        assert_eq!(props.id, "app");
        assert_eq!(props.menu.as_deref(), Some("/not/an/object/path"), "a string-typed Menu still parses");
        assert_eq!(props.icon_pixmaps.len(), 1);
        assert_eq!((props.tooltip_title.as_str(), props.tooltip_body.as_str()), ("Title", "Body"));
    }

    #[test]
    fn a_mistyped_pixmap_is_an_error_so_the_caller_can_fall_back_to_single_gets() {
        let data = encode(vec![("Id", Value::from("app")), (ICON_PIXMAP, Value::from("not a pixmap"))]);
        assert!(data.deserialize::<Wire>().is_err());
    }

    #[test]
    fn an_empty_dictionary_is_empty() {
        let data = encode(Vec::new());
        assert!(decode(&data).is_empty());
    }
}
