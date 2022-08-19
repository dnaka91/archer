pub mod hex {
    use std::borrow::Cow;

    use serde::{Deserialize, Serialize};

    pub fn serialize<S>(value: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        hex::encode(value).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <Cow<'_, str>>::deserialize(deserializer)?;
        hex::decode(value.as_bytes()).map_err(serde::de::Error::custom)
    }
}
