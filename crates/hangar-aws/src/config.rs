//! [`HostedConfig`], the region, table and bucket that the stores use.
//!
//! The config holds no credential and no endpoint. Credentials come only
//! from the AWS provider chain, and the parser refuses a key that names one.
//! No refusal repeats a value from the config, because a value in the wrong
//! place may be a secret.

use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use std::fmt;

/// The keys a hosted config takes.
const KEYS: [&str; 3] = ["region", "table", "bucket"];

/// The keys that name a credential, which the parser refuses.
const CREDENTIAL_KEYS: [&str; 5] = [
    "access_key_id",
    "secret_access_key",
    "session_token",
    "credentials",
    "profile",
];

/// Why a hosted config, or one of its names, was refused. No message
/// repeats a value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The config names a credential.
    #[error(
        "the hosted config names a credential; hangar takes AWS credentials only from the AWS \
         provider chain (the environment, a web identity token, or a container or instance \
         role), so remove the key"
    )]
    CredentialKey,
    /// The config has a key it does not take.
    #[error("the hosted config has an unknown key; it takes region, table and bucket")]
    UnknownKey,
    /// The config lacks a key.
    #[error("the hosted config has no {0} key")]
    Missing(&'static str),
    /// The config gives a key twice.
    #[error("the hosted config gives {0} twice")]
    Repeated(&'static str),
    /// The value of a key is not text.
    #[error("the hosted config's {0} must be text")]
    NotText(&'static str),
    /// The config is not a table of keys.
    #[error("the hosted config must be a table of keys")]
    NotATable,
    /// A name breaks its rule.
    #[error("the hosted config's {key} must be {rule}")]
    Malformed {
        /// The key whose value was refused.
        key: &'static str,
        /// The rule the value breaks.
        rule: &'static str,
    },
}

/// The rule of one kind of name: its key, the rule's text, its lengths in
/// bytes, the bytes it admits and the bytes it may start and end with.
struct Rule {
    key: &'static str,
    text: &'static str,
    min: usize,
    max: usize,
    allowed: fn(u8) -> bool,
    edge: fn(u8) -> bool,
}

impl Rule {
    fn check(&self, text: &str) -> Result<(), ConfigError> {
        let bytes = text.as_bytes();
        let fits = (self.min..=self.max).contains(&bytes.len())
            && bytes.iter().copied().all(self.allowed)
            && bytes.first().copied().is_some_and(self.edge)
            && bytes.last().copied().is_some_and(self.edge);
        if fits {
            Ok(())
        } else {
            Err(ConfigError::Malformed {
                key: self.key,
                rule: self.text,
            })
        }
    }
}

/// Defines a name type that only its checking constructor builds.
macro_rules! name_type {
    ($(#[$meta:meta])* $name:ident, $rule:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// The name as text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = ConfigError;

            fn try_from(text: &str) -> Result<Self, ConfigError> {
                $rule.check(text)?;
                Ok(Self(text.to_owned()))
            }
        }
    };
}

fn lower_digit_hyphen(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
}

fn lower_or_digit(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

fn table_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

const REGION: Rule = Rule {
    key: "region",
    text: "1 to 32 lower-case letters, digits and hyphens, starting and ending with a letter or digit",
    min: 1,
    max: 32,
    allowed: lower_digit_hyphen,
    edge: lower_or_digit,
};

const TABLE: Rule = Rule {
    key: "table",
    text: "3 to 255 letters, digits, underscores, hyphens and dots",
    min: 3,
    max: 255,
    allowed: table_byte,
    edge: table_byte,
};

const BUCKET: Rule = Rule {
    key: "bucket",
    text: "3 to 63 lower-case letters, digits and hyphens, starting and ending with a letter or digit",
    min: 3,
    max: 63,
    allowed: lower_digit_hyphen,
    edge: lower_or_digit,
};

name_type!(
    /// An AWS region, such as `eu-west-1`: 1 to 32 lower-case letters,
    /// digits and hyphens, starting and ending with a letter or digit.
    ///
    /// Shortcut: the type checks the characters and the length and not that
    /// the region exists. A region that does not exist fails at the first
    /// call, and `Stores::check` makes one before a server listens.
    Region,
    REGION
);
name_type!(
    /// A DynamoDB table name: 3 to 255 letters, digits, `_`, `-` and `.`.
    TableName,
    TABLE
);
name_type!(
    /// An S3 bucket name: 3 to 63 lower-case letters, digits and hyphens,
    /// starting and ending with a letter or digit.
    ///
    /// Shortcut: the type admits no dot, so it refuses the dotted names that
    /// S3 allows, and it does not check S3's reserved prefixes and suffixes.
    /// Bucket creation refuses those.
    BucketName,
    BUCKET
);

/// Where a hosted hangar keeps its shares, grants and files: the region,
/// the DynamoDB table and the S3 bucket.
///
/// It deserializes from a table with exactly the keys `region`, `table` and
/// `bucket`, and refuses an unknown key and any key that names a
/// credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedConfig {
    region: Region,
    table: TableName,
    bucket: BucketName,
}

impl HostedConfig {
    /// A config from three parsed names.
    #[must_use]
    pub fn new(region: Region, table: TableName, bucket: BucketName) -> Self {
        Self {
            region,
            table,
            bucket,
        }
    }

    /// The region of the table and the bucket.
    #[must_use]
    pub fn region(&self) -> &Region {
        &self.region
    }

    /// The table that holds shares and grants.
    #[must_use]
    pub fn table(&self) -> &TableName {
        &self.table
    }

    /// The bucket that holds files.
    #[must_use]
    pub fn bucket(&self) -> &BucketName {
        &self.bucket
    }
}

/// One value of the config, read as text. Any other kind of value is
/// refused with a message that names the key and not the value.
struct Text(&'static str);

impl<'de> de::DeserializeSeed<'de> for Text {
    type Value = String;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        deserializer.deserialize_any(self)
    }
}

/// Defines the visitor methods that refuse a kind of value without
/// repeating it.
macro_rules! refuse {
    ($($method:ident($type:ty)),* $(,)?) => {$(
        fn $method<E: de::Error>(self, _value: $type) -> Result<String, E> {
            Err(E::custom(ConfigError::NotText(self.0)))
        }
    )*};
}

impl<'de> Visitor<'de> for Text {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("text")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
        Ok(value.to_owned())
    }

    refuse!(
        visit_bool(bool),
        visit_i64(i64),
        visit_i128(i128),
        visit_u64(u64),
        visit_u128(u128),
        visit_f64(f64),
        visit_bytes(&[u8]),
    );

    fn visit_none<E: de::Error>(self) -> Result<String, E> {
        Err(E::custom(ConfigError::NotText(self.0)))
    }

    fn visit_unit<E: de::Error>(self) -> Result<String, E> {
        Err(E::custom(ConfigError::NotText(self.0)))
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, _seq: A) -> Result<String, A::Error> {
        Err(de::Error::custom(ConfigError::NotText(self.0)))
    }

    fn visit_map<A: MapAccess<'de>>(self, _map: A) -> Result<String, A::Error> {
        Err(de::Error::custom(ConfigError::NotText(self.0)))
    }
}

/// One key of the config, matched against the known keys without keeping
/// its text.
enum Field {
    Known(usize),
    Credential,
    Unknown,
}

struct FieldVisitor;

impl Visitor<'_> for FieldVisitor {
    type Value = Field;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a key")
    }

    fn visit_str<E: de::Error>(self, key: &str) -> Result<Field, E> {
        if let Some(index) = KEYS.iter().position(|known| *known == key) {
            Ok(Field::Known(index))
        } else if CREDENTIAL_KEYS.contains(&key) {
            Ok(Field::Credential)
        } else {
            Ok(Field::Unknown)
        }
    }
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_identifier(FieldVisitor)
    }
}

struct ConfigVisitor;

impl<'de> Visitor<'de> for ConfigVisitor {
    type Value = HostedConfig;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a table with region, table and bucket")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<HostedConfig, A::Error> {
        let fail = |error: ConfigError| de::Error::custom(error);
        let mut values: [Option<String>; 3] = [None, None, None];
        while let Some(field) = map.next_key::<Field>()? {
            let index = match field {
                Field::Known(index) => index,
                Field::Credential => return Err(fail(ConfigError::CredentialKey)),
                Field::Unknown => return Err(fail(ConfigError::UnknownKey)),
            };
            let (Some(key), Some(slot)) = (KEYS.get(index), values.get_mut(index)) else {
                return Err(fail(ConfigError::UnknownKey));
            };
            if slot.is_some() {
                return Err(fail(ConfigError::Repeated(key)));
            }
            *slot = Some(map.next_value_seed(Text(key))?);
        }
        let [region, table, bucket] = values;
        let region = region.ok_or(ConfigError::Missing("region")).map_err(fail)?;
        let table = table.ok_or(ConfigError::Missing("table")).map_err(fail)?;
        let bucket = bucket.ok_or(ConfigError::Missing("bucket")).map_err(fail)?;
        Ok(HostedConfig::new(
            Region::try_from(region.as_str()).map_err(fail)?,
            TableName::try_from(table.as_str()).map_err(fail)?,
            BucketName::try_from(bucket.as_str()).map_err(fail)?,
        ))
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, _seq: A) -> Result<HostedConfig, A::Error> {
        Err(de::Error::custom(ConfigError::NotATable))
    }
}

impl<'de> Deserialize<'de> for HostedConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ConfigVisitor)
    }
}
