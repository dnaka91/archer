pub mod hex {
    use std::{
        borrow::Cow,
        fmt::{Display, LowerHex},
        mem,
    };

    use num_traits::Num;
    use serde::{Deserialize, Serialize};

    pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
        T: LowerHex,
    {
        format!("{value:0width$x}", width = mem::size_of::<T>() * 2).serialize(serializer)
    }

    pub fn deserialize<'de, D, T, E>(deserializer: D) -> Result<T, D::Error>
    where
        D: serde::Deserializer<'de>,
        T: Num<FromStrRadixErr = E>,
        E: Display,
    {
        let value = <Cow<'_, str>>::deserialize(deserializer)?;
        T::from_str_radix(&value, 16).map_err(serde::de::Error::custom)
    }
}
