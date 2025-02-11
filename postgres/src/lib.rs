#![forbid(unsafe_code)]
#![feature(type_alias_impl_trait)]

//! A postgresql client on top of [rust-postgres](https://github.com/sfackler/rust-postgres/).

mod client;
mod column;
mod config;
mod connect;
mod io;
mod iter;
mod prepare;
mod query;
mod request;
mod response;
mod row;
mod util;

pub mod error;
pub mod statement;

pub use postgres_types::{ToSql, Type};

pub use self::{
    client::Client,
    config::Config,
    iter::AsyncIterator,
    row::{Row, RowSimple},
};

use self::error::Error;

#[derive(Debug)]
pub struct Postgres<C, const BATCH_LIMIT: usize> {
    cfg: C,
    backlog: usize,
}

impl<C> Postgres<C, 32>
where
    Config: TryFrom<C>,
    Error: From<<Config as TryFrom<C>>::Error>,
{
    pub fn new(cfg: C) -> Self {
        Self { cfg, backlog: 32 }
    }
}

impl<C, const BATCH_LIMIT: usize> Postgres<C, BATCH_LIMIT>
where
    Config: TryFrom<C>,
    Error: From<<Config as TryFrom<C>>::Error>,
{
    /// Set backlog of pending async queries.
    pub fn backlog(mut self, num: usize) -> Self {
        self.backlog = num;
        self
    }

    /// Set upper bound of batched queries.
    pub fn batch_limit<const BATCH_LIMIT2: usize>(self) -> Postgres<C, BATCH_LIMIT2> {
        Postgres {
            cfg: self.cfg,
            backlog: self.backlog,
        }
    }

    /// Connect to database. The returned values are [Client] and a detached async task
    /// that drives the client communication to db and it needs to spawn on an async runtime.
    ///
    /// # Examples:
    /// ```rust
    /// # use xitca_postgres::Postgres;
    /// # async fn connect() {
    /// let cfg = String::from("postgres://user:pass@localhost/db");
    /// let (cli, task) = Postgres::new(cfg).connect().await.unwrap();
    ///
    /// tokio::spawn(task);
    ///
    /// let stmt = cli.prepare("SELECT *", &[]).await.unwrap();
    /// # }
    ///
    /// ```
    pub async fn connect(self) -> Result<(Client, impl std::future::Future<Output = Result<(), Error>> + Send), Error> {
        let cfg = Config::try_from(self.cfg)?;
        let io = connect::connect(&cfg).await?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cli = Client::new(tx);
        let handle = io::buffered::BufferedIo::new(io, rx).spawn();

        let ret = cli.authenticate(cfg).await;
        // retrieve io regardless of authentication outcome.
        let io = handle.into_inner().await;
        ret?;

        Ok((cli, io.run()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn postgres() {
        let _ = Postgres::new("abc");
        let _ = Postgres::new(Config::default());
    }
}
