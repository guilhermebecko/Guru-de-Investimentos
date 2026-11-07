use std::path::{Path, PathBuf};

use rusqlite::{Connection, ErrorCode};
use rusqlite_migration::{M, Migrations};

use crate::error::CoreError;

const MIGRATIONS_SQL: &[&str] = &[
    include_str!("migrations/0001_inicial.sql"),
    include_str!("migrations/0002_investimentos.sql"),
    include_str!("migrations/0003_perfil_metas.sql"),
    include_str!("migrations/0004_mercado.sql"),
    include_str!("migrations/0005_importacao.sql"),
    include_str!("migrations/0006_ia.sql"),
    include_str!("migrations/0007_ia_app.sql"),
    include_str!("migrations/0008_btg.sql"),
    include_str!("migrations/0009_cvm.sql"),
];

/// Versão do esquema: número de migrações conhecidas por este app.
pub const SCHEMA_VERSION: u32 = MIGRATIONS_SQL.len() as u32;

pub struct Db {
    pub(crate) conn: Connection,
    path: PathBuf,
}

impl Db {
    pub fn open(data_dir: &Path) -> Result<Db, CoreError> {
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join("guru.db");
        let mut conn = Connection::open(&path)?;

        // Configuração da conexão: não grava nada no arquivo.
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;

        let versao_arquivo = user_version(&conn)?;
        if versao_arquivo > SCHEMA_VERSION {
            return Err(CoreError::SchemaTooNew {
                banco: versao_arquivo,
                app: SCHEMA_VERSION,
            });
        }

        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))
            .map_err(classifica)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let migrations = Migrations::new(MIGRATIONS_SQL.iter().map(|sql| M::up(sql)).collect());
        migrations
            .to_latest(&mut conn)
            .map_err(classifica_migracao)?;

        Ok(Db { conn, path })
    }

    pub fn schema_version(&self) -> Result<u32, CoreError> {
        user_version(&self.conn)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn user_version(conn: &Connection) -> Result<u32, CoreError> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .map_err(classifica)
}

/// Arquivo que não é SQLite ou está corrompido vira `DbInvalid`; o resto fica como `Sqlite`.
pub(crate) fn classifica(e: rusqlite::Error) -> CoreError {
    match e.sqlite_error_code() {
        Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => CoreError::DbInvalid(
            "o arquivo guru.db não é um banco SQLite válido (pode estar corrompido)".to_string(),
        ),
        _ => CoreError::Sqlite(e),
    }
}

/// Erros da migração que embrulham um erro do SQLite seguem `classifica`;
/// só problemas reais de esquema são `DbInvalid`.
fn classifica_migracao(e: rusqlite_migration::Error) -> CoreError {
    match e {
        rusqlite_migration::Error::RusqliteError { err, .. } => classifica(err),
        outro => CoreError::DbInvalid(outro.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn erro_sqlite(codigo: i32) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(codigo), None)
    }

    #[test]
    fn classifica_banco_corrompido_como_db_invalid() {
        let e = classifica(erro_sqlite(rusqlite::ffi::SQLITE_CORRUPT));
        assert_eq!(e.code(), "DB_INVALID");
    }

    #[test]
    fn migracao_que_embrulha_erro_do_sqlite_mantem_a_classificacao() {
        let ocupado = rusqlite_migration::Error::with_sql(
            erro_sqlite(rusqlite::ffi::SQLITE_BUSY),
            "CREATE TABLE x (a)",
        );
        assert_eq!(classifica_migracao(ocupado).code(), "SQLITE");

        let corrompido = rusqlite_migration::Error::with_sql(
            erro_sqlite(rusqlite::ffi::SQLITE_CORRUPT),
            "CREATE TABLE x (a)",
        );
        assert_eq!(classifica_migracao(corrompido).code(), "DB_INVALID");
    }

    #[test]
    fn problema_real_de_esquema_continua_db_invalid() {
        let e = classifica_migracao(rusqlite_migration::Error::InvalidUserVersion);
        assert_eq!(e.code(), "DB_INVALID");
    }

    #[test]
    fn usa_wal_e_chaves_estrangeiras() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let modo: String = db
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        let fk: i64 = db
            .conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(modo, "wal");
        assert_eq!(fk, 1);
    }
}
