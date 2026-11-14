//! A path as the files Colet keeps write it: as text when it is text, and otherwise as its UTF-16 units,
//! `{"utf16": [...]}`, as Windows allows an unpaired surrogate in a name and text would turn it into
//! U+FFFD, the path of another item. For `#[serde(with = "crate::kept_path")]`.
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap};

pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    match path.to_str() {
        Some(text) => serializer.serialize_str(text),
        None => {
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry("utf16", &path.as_os_str().encode_wide().collect::<Vec<u16>>())?;
            map.end()
        }
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Form {
        Text(String),
        Wide { utf16: Vec<u16> },
    }
    Ok(match Form::deserialize(deserializer)? {
        Form::Text(text) => text.into(),
        Form::Wide { utf16 } => OsString::from_wide(&utf16).into(),
    })
}

/// A path written as [`serialize`] writes it, for where one sits in a list or an `Option`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeptPath(#[serde(with = "crate::kept_path")] pub PathBuf);

/// `paths` as a list of what [`serialize`] writes.
pub fn serialize_each<S: Serializer>(paths: &[PathBuf], serializer: S) -> Result<S::Ok, S::Error> {
    struct One<'a>(&'a Path);
    impl Serialize for One<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serialize(self.0, serializer)
        }
    }
    serializer.collect_seq(paths.iter().map(|path| One(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_path_as_text_and_one_that_is_not_as_its_units() {
        let text = KeptPath(r"D:\作業".into());
        assert_eq!(serde_json::to_string(&text).unwrap(), r#""D:\\作業""#);
        assert_eq!(serde_json::from_str::<KeptPath>(r#""D:\\作業""#).unwrap(), text);
        let wide: Vec<u16> = r"D:\a".encode_utf16().chain([0xD800]).collect();
        let odd = KeptPath(OsString::from_wide(&wide).into());
        let json = serde_json::to_string(&odd).unwrap();
        assert_eq!(json, r#"{"utf16":[68,58,92,97,55296]}"#);
        assert_eq!(serde_json::from_str::<KeptPath>(&json).unwrap(), odd);
        // A path that is text read in units is the same path.
        assert_eq!(serde_json::from_str::<KeptPath>(r#"{"utf16":[68,58,92]}"#).unwrap(), KeptPath(r"D:\".into()));
        for broken in [r#"{"utf16":[70000]}"#, r#"{"utf16":"D:\\"}"#, r#"{"path":"D:\\"}"#, "5", "null"] {
            assert!(serde_json::from_str::<KeptPath>(broken).is_err(), "{broken}");
        }
    }
}
