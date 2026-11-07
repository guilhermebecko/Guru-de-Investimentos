use guru_core::{CoreError, Db, SCHEMA_VERSION};

#[test]
fn abre_e_cria_o_banco_na_pasta() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(db.path(), dir.path().join("guru.db"));
    assert!(db.path().exists());
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 9);
}

#[test]
fn reabrir_nao_reaplica_migracoes() {
    let dir = tempfile::tempdir().unwrap();
    drop(Db::open(dir.path()).unwrap());
    assert_eq!(
        Db::open(dir.path()).unwrap().schema_version().unwrap(),
        SCHEMA_VERSION
    );
}

#[test]
fn cria_a_pasta_se_nao_existir_e_aceita_acentos() {
    let dir = tempfile::tempdir().unwrap();
    let pasta = dir
        .path()
        .join("João da Silva")
        .join("Guru de Investimentos");
    let db = Db::open(&pasta).unwrap();
    assert!(db.path().exists());
}

#[test]
fn recusa_banco_de_versao_mais_nova_sem_alterar() {
    let dir = tempfile::tempdir().unwrap();
    drop(Db::open(dir.path()).unwrap());
    let arquivo = dir.path().join("guru.db");
    {
        let conn = rusqlite::Connection::open(&arquivo).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
    }
    let antes = std::fs::read(&arquivo).unwrap();

    let erro = Db::open(dir.path()).err().expect("deveria recusar");
    assert!(matches!(
        erro,
        CoreError::SchemaTooNew { banco, app }
            if banco == SCHEMA_VERSION + 1 && app == SCHEMA_VERSION
    ));
    assert_eq!(erro.code(), "SCHEMA_TOO_NEW");
    assert_eq!(std::fs::read(&arquivo).unwrap(), antes);
}

#[test]
fn recusa_arquivo_que_nao_e_sqlite_sem_apagar() {
    let dir = tempfile::tempdir().unwrap();
    let arquivo = dir.path().join("guru.db");
    std::fs::write(&arquivo, "isto não é um banco").unwrap();

    let erro = Db::open(dir.path()).err().expect("deveria recusar");
    assert_eq!(erro.code(), "DB_INVALID");
    assert_eq!(
        std::fs::read_to_string(&arquivo).unwrap(),
        "isto não é um banco"
    );
}

#[test]
fn banco_com_cabecalho_valido_e_paginas_corrompidas_e_db_invalid() {
    let dir = tempfile::tempdir().unwrap();
    drop(Db::open(dir.path()).unwrap());
    let arquivo = dir.path().join("guru.db");
    let mut bytes = std::fs::read(&arquivo).unwrap();
    // Mantém o cabeçalho de 100 bytes e estraga o resto da primeira página.
    for b in bytes[100..4096].iter_mut() {
        *b = 0xFF;
    }
    std::fs::write(&arquivo, &bytes).unwrap();
    let _ = std::fs::remove_file(dir.path().join("guru.db-wal"));
    let _ = std::fs::remove_file(dir.path().join("guru.db-shm"));

    let erro = Db::open(dir.path()).err().expect("deveria recusar");
    assert_eq!(erro.code(), "DB_INVALID", "erro: {erro}");
}
