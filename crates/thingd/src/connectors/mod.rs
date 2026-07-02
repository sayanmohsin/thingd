//! Database connector implementations for `Postgres` and `MySQL`.
//!
//! These connectors use `sqlx` internally with a `tokio::runtime::Runtime`
//! to provide a synchronous `PullStream` interface compatible with the
//! `Connector` trait.

#[cfg(feature = "connectors")]
mod mysql;
#[cfg(feature = "connectors")]
mod postgres;

#[cfg(feature = "connectors")]
pub use mysql::MysqlConnector;
#[cfg(feature = "connectors")]
pub use postgres::PostgresConnector;
