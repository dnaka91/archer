pub mod hex {
    use std::{
        borrow::Cow,
        fmt::{Display, LowerHex},
        mem,
    };

    use num_traits::Num;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: LowerHex,
    {
        format!("{value:0width$x}", width = mem::size_of::<T>() * 2).serialize(serializer)
    }

    pub fn deserialize<'de, D, T, E>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: Num<FromStrRadixErr = E>,
        E: Display,
    {
        let value = <Cow<'_, str>>::deserialize(deserializer)?;
        T::from_str_radix(&value, 16).map_err(de::Error::custom)
    }

    pub mod nonzero {
        use std::{
            fmt::{Display, LowerHex},
            num::{NonZeroU128, NonZeroU64},
        };

        use num_traits::Num;
        use serde::{de, Deserializer, Serializer};

        pub trait NonZeroNum<T>: Sized {
            fn new(num: T) -> Option<Self>;
            fn get(self) -> T;
        }

        impl NonZeroNum<u128> for NonZeroU128 {
            fn new(num: u128) -> Option<Self> {
                Self::new(num)
            }

            fn get(self) -> u128 {
                self.get()
            }
        }

        impl NonZeroNum<u64> for NonZeroU64 {
            fn new(num: u64) -> Option<Self> {
                Self::new(num)
            }

            fn get(self) -> u64 {
                self.get()
            }
        }

        pub fn serialize<S, T, N>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
            T: NonZeroNum<N> + Copy,
            N: LowerHex,
        {
            super::serialize(&value.get(), serializer)
        }

        pub fn deserialize<'de, D, T, N, E>(deserializer: D) -> Result<T, D::Error>
        where
            D: Deserializer<'de>,
            T: NonZeroNum<N>,
            N: Num<FromStrRadixErr = E>,
            E: Display,
        {
            let value = super::deserialize(deserializer)?;
            T::new(value).ok_or_else(|| de::Error::custom("value must not be zero"))
        }
    }
}
