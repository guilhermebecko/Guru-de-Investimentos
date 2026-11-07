use chrono::NaiveDate;
use guru_core::{Classe, Db, ErroCampo, Perfil, SCHEMA_VERSION, alocacao_sugerida};

fn data(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

fn banco() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    (dir, db)
}

/// Respostas moderadas, sem travas: soma 17 (perguntas 2 e 6 em 1, a 12 em 0).
fn moderado() -> Vec<u8> {
    vec![1, 1, 2, 1, 2, 1, 2, 2, 1, 2, 2, 0]
}

/// Tudo zero: pergunta 2 trava em Conservador.
fn conservador() -> Vec<u8> {
    vec![0; 12]
}

fn valores(v: [i64; 5]) -> Vec<(String, i64)> {
    Classe::TODOS
        .iter()
        .zip(v)
        .map(|(c, p)| (c.codigo().to_string(), p))
        .collect()
}

#[test]
fn registra_perfil_e_perfil_atual_devolve() {
    let (_d, db) = banco();
    assert!(db.perfil_atual(data(2026, 10, 29)).unwrap().is_none());
    let g = db
        .registrar_perfil(&moderado(), data(2026, 10, 29))
        .unwrap()
        .unwrap();
    assert_eq!(g.perfil, Perfil::Moderado);
    assert_eq!(g.feito_em, "2026-10-29");
    assert!(!g.vencido);
    assert_eq!(g.alocacao_sugerida, alocacao_sugerida(Perfil::Moderado));
    let atual = db.perfil_atual(data(2026, 10, 29)).unwrap().unwrap();
    assert_eq!(atual, g);
}

#[test]
fn avisos_sao_gravados_e_lidos() {
    let (_d, db) = banco();
    // Pontos de Moderado, mas a pergunta 2 (menos de 1 ano) trava em Conservador com aviso.
    let g = db
        .registrar_perfil(&[2, 0, 2, 2, 2, 2, 2, 2, 2, 2, 1, 0], data(2026, 10, 29))
        .unwrap()
        .unwrap();
    assert_eq!(g.perfil, Perfil::Conservador);
    assert_eq!(g.avisos.len(), 1);
    let atual = db.perfil_atual(data(2026, 10, 29)).unwrap().unwrap();
    assert_eq!(atual.avisos, g.avisos);
}

#[test]
fn respostas_invalidas_nao_gravam() {
    let (_d, db) = banco();
    let erro = db
        .registrar_perfil(&[0, 1], data(2026, 10, 29))
        .unwrap()
        .unwrap_err();
    assert_eq!(erro.campo, "respostas");
    assert!(db.perfil_atual(data(2026, 10, 29)).unwrap().is_none());
    assert!(db.alocacao_alvo().unwrap().is_none());
}

#[test]
fn segundo_registro_vira_o_atual() {
    let (_d, db) = banco();
    db.registrar_perfil(&moderado(), data(2026, 1, 10))
        .unwrap()
        .unwrap();
    db.registrar_perfil(&conservador(), data(2026, 10, 29))
        .unwrap()
        .unwrap();
    let atual = db.perfil_atual(data(2026, 10, 29)).unwrap().unwrap();
    assert_eq!(atual.perfil, Perfil::Conservador);
    assert_eq!(atual.feito_em, "2026-10-29");
}

#[test]
fn primeiro_registro_cria_alocacao_e_o_segundo_nao_sobrescreve() {
    let (_d, db) = banco();
    assert!(db.alocacao_alvo().unwrap().is_none());
    db.registrar_perfil(&moderado(), data(2026, 10, 29))
        .unwrap()
        .unwrap();
    assert_eq!(
        db.alocacao_alvo().unwrap().unwrap(),
        alocacao_sugerida(Perfil::Moderado)
    );
    assert!(
        db.salvar_alocacao_alvo(&valores([10, 20, 30, 40, 0]))
            .unwrap()
            .is_empty()
    );
    db.registrar_perfil(&conservador(), data(2026, 10, 30))
        .unwrap()
        .unwrap();
    let alvo = db.alocacao_alvo().unwrap().unwrap();
    let pontos: Vec<u8> = alvo.iter().map(|(_, p)| *p).collect();
    assert_eq!(pontos, vec![10, 20, 30, 40, 0]);
}

#[test]
fn vencido_depois_de_24_meses() {
    let (_d, db) = banco();
    db.registrar_perfil(&moderado(), data(2024, 10, 28))
        .unwrap()
        .unwrap();
    assert!(
        !db.perfil_atual(data(2026, 10, 28))
            .unwrap()
            .unwrap()
            .vencido
    );
    assert!(
        db.perfil_atual(data(2026, 10, 29))
            .unwrap()
            .unwrap()
            .vencido
    );
}

#[test]
fn alocacao_aceita_soma_100() {
    let (_d, db) = banco();
    assert!(
        db.salvar_alocacao_alvo(&valores([40, 30, 10, 10, 10]))
            .unwrap()
            .is_empty()
    );
    let alvo = db.alocacao_alvo().unwrap().unwrap();
    assert_eq!(
        alvo.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        Classe::TODOS.to_vec()
    );
    assert_eq!(
        alvo.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
        vec![40, 30, 10, 10, 10]
    );
}

fn recusa(db: &Db, entrada: &[(String, i64)]) -> Vec<ErroCampo> {
    let erros = db.salvar_alocacao_alvo(entrada).unwrap();
    assert!(!erros.is_empty());
    assert!(db.alocacao_alvo().unwrap().is_none(), "não pode gravar");
    erros
}

#[test]
fn alocacao_recusa_soma_errada_com_mensagem() {
    let (_d, db) = banco();
    let e = recusa(&db, &valores([40, 30, 10, 10, 9]));
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].campo, "alocacao");
    assert_eq!(e[0].mensagem, "A soma precisa dar 100%. Hoje dá 99%.");
    let e = recusa(&db, &valores([40, 30, 10, 10, 11]));
    assert_eq!(e[0].mensagem, "A soma precisa dar 100%. Hoje dá 101%.");
}

#[test]
fn alocacao_recusa_valor_fora_de_0_a_100() {
    let (_d, db) = banco();
    let e = recusa(&db, &valores([-1, 30, 40, 20, 11]));
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].campo, "rf");
    assert_eq!(e[0].mensagem, "Use um número inteiro de 0 a 100.");
    let e = recusa(&db, &valores([0, 101, 0, 0, -1]));
    let campos: Vec<&str> = e.iter().map(|x| x.campo.as_str()).collect();
    assert_eq!(campos, vec!["acoes", "outros"]);
}

#[test]
fn alocacao_recusa_classe_faltando_ou_desconhecida() {
    let (_d, db) = banco();
    let mut v = valores([50, 30, 10, 10, 0]);
    v.pop();
    let e = recusa(&db, &v);
    assert_eq!(e[0].campo, "outros");
    assert_eq!(e[0].mensagem, "Use um número inteiro de 0 a 100.");
    let mut v = valores([50, 30, 10, 10, 0]);
    v[4].0 = "cripto".into();
    recusa(&db, &v);
}

#[test]
fn objetivo_salva_le_e_apaga() {
    let (_d, db) = banco();
    assert_eq!(db.objetivo().unwrap(), None);
    assert!(
        db.salvar_objetivo("  Comprar um apartamento  ")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.objetivo().unwrap().as_deref(),
        Some("Comprar um apartamento")
    );
    assert!(db.salvar_objetivo("Outro").unwrap().is_empty());
    assert_eq!(db.objetivo().unwrap().as_deref(), Some("Outro"));
    assert!(db.salvar_objetivo("   ").unwrap().is_empty());
    assert_eq!(db.objetivo().unwrap(), None);
}

#[test]
fn objetivo_limita_a_2000_caracteres() {
    let (_d, db) = banco();
    let ok = "ã".repeat(2000);
    assert!(db.salvar_objetivo(&ok).unwrap().is_empty());
    let e = db.salvar_objetivo(&"a".repeat(2001)).unwrap();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].campo, "objetivo");
    assert_eq!(e[0].mensagem, "Use até 2.000 caracteres.");
    assert_eq!(db.objetivo().unwrap().unwrap().chars().count(), 2000);
}

#[test]
fn migra_banco_da_versao_2() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
        conn.execute_batch(include_str!("../src/migrations/0001_inicial.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../src/migrations/0002_investimentos.sql"))
            .unwrap();
        for nome in ["A", "B"] {
            conn.execute(
                "INSERT INTO investimentos (tipo, instituicao, data_aplicacao, \
                 valor_aplicado_centavos, criado_em, atualizado_em) \
                 VALUES ('rf-cdb', ?1, '2025-01-01', 100000, 'x', 'x')",
                [nome],
            )
            .unwrap();
        }
        conn.pragma_update(None, "user_version", 2).unwrap();
    }
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), 9);
    assert_eq!(SCHEMA_VERSION, 9);
    let conn = rusqlite::Connection::open(db.path()).unwrap();
    let (n, nulos): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COUNT(*) - COUNT(meta_id) FROM investimentos",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((n, nulos), (2, 2));
}
