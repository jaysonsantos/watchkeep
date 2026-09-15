//! The Watchkeep database: migrations, rows, writes, and reads.
//!
//! Every query is a `sqlx` macro, so the SQL is checked against the schema at
//! compile time. The macros read `WATCHKEEP_DATABASE_URL` (see `sqlx.toml`) or
//! the offline data in `.sqlx/`.

/// An enum stored as `TEXT`. Each variant maps to one word. The enum encodes and
/// decodes like `&str`, so the query macros accept it as a column override
/// (`kind AS "kind: MediaKind"`), and `as_str()` gives the parameter value.
#[macro_export]
macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ::ts_rs::TS)]
        #[ts(export)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$(Self::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::model::InvalidKind;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value).ok_or_else(|| $crate::model::InvalidKind(value.to_owned()))
            }
        }

        impl sqlx::Type<sqlx::Postgres> for $name {
            fn type_info() -> sqlx::postgres::PgTypeInfo {
                <&str as sqlx::Type<sqlx::Postgres>>::type_info()
            }

            fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
                <&str as sqlx::Type<sqlx::Postgres>>::compatible(ty)
            }
        }

        impl sqlx::postgres::PgHasArrayType for $name {
            fn array_type_info() -> sqlx::postgres::PgTypeInfo {
                <&str as sqlx::postgres::PgHasArrayType>::array_type_info()
            }
        }

        impl sqlx::Encode<'_, sqlx::Postgres> for $name {
            fn encode_by_ref(
                &self,
                buf: &mut sqlx::postgres::PgArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <&str as sqlx::Encode<sqlx::Postgres>>::encode_by_ref(&self.as_str(), buf)
            }
        }

        impl<'r> sqlx::Decode<'r, sqlx::Postgres> for $name {
            fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
                let text = <&str as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
                Ok(text.parse()?)
            }
        }
    };
}

pub mod bulk;
pub mod clock;
pub mod db;
pub mod library;
pub mod lists;
pub mod model;
pub mod queries;
