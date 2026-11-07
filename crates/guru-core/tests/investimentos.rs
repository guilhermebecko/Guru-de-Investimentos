use chrono::NaiveDate;
use guru_core::{Benchmark, Classe, Db, Garantia, Gravacao, NovoInvestimento, SCHEMA_VERSION};

fn hoje() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn cdb() -> NovoInvestimento {
    NovoInvestimento {
        tipo: "rf-cdb".into(),
        instituicao: "Banco Inter".into(),
        emissor: s("Banco Inter"),
        indexador: s("pos"),
        taxa: s("102"),
        benchmark: s("cdi"),
        data_aplicacao: s("2025-03-15"),
        vencimento: s("2028-03-15"),
        valor_aplicado_centavos: Some(2_000_000),
        liquidez: s("vencimento"),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn tesouro() -> NovoInvestimento {
    NovoInvestimento {
        tipo: "td-selic".into(),
        instituicao: "Rico".into(),
        taxa: s("0.09"),
        data_aplicacao: s("2025-01-10"),
        vencimento: s("2029-03-01"),
        valor_aplicado_centavos: Some(1_800_000),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn rv(tipo: &str, ticker: &str, qtd: i64, preco: i64) -> NovoInvestimento {
    NovoInvestimento {
        tipo: tipo.into(),
        instituicao: "Rico".into(),
        ticker: s(ticker),
        data_aplicacao: s("2025-04-01"),
        quantidade: Some(qtd),
        preco_medio_centavos: Some(preco),
        ..Default::default()
    }
}

fn abrir() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    (dir, db)
}

fn id(g: Gravacao) -> i64 {
    match g {
        Gravacao::Ok(id) => id,
        Gravacao::Invalido(e) => panic!("inválido: {e:?}"),
    }
}

#[test]
fn cria_e_lista() {
    let (_d, db) = abrir();
    id(db.criar_investimento(&cdb(), hoje()).unwrap());
    id(db
        .criar_investimento(&rv("rv-acao", "ITUB4", 150, 3142), hoje())
        .unwrap());
    let lista = db.listar_investimentos(hoje()).unwrap();
    assert_eq!(lista.len(), 2);
    let c = &lista[0];
    assert_eq!(c.nome, "CDB Banco Inter");
    assert_eq!(c.rendimento.as_deref(), Some("102% do CDI"));
    assert_eq!(c.garantia, Some(Garantia::Fgc));
    assert_eq!(c.benchmark, Some(Benchmark::Cdi));
    assert_eq!(c.taxa.as_deref(), Some("102"));
    assert_eq!(c.classe, Classe::Rf);
    assert_eq!(c.tipo_rotulo, "CDB");
    assert_eq!(c.data_aplicacao, "2025-03-15");
    let a = &lista[1];
    assert_eq!(a.nome, "ITUB4");
    assert_eq!(a.valor_aplicado_centavos, 471_300);
    assert_eq!(a.quantidade, Some(150));
    assert_eq!(a.preco_medio_centavos, Some(3142));
    assert!(a.rendimento.is_none());
}

#[test]
fn invalido_nao_grava() {
    let (_d, db) = abrir();
    let mut ruim = cdb();
    ruim.taxa = None;
    match db.criar_investimento(&ruim, hoje()).unwrap() {
        Gravacao::Invalido(e) => assert!(e.iter().any(|e| e.campo == "taxa")),
        Gravacao::Ok(_) => panic!("não deveria gravar"),
    }
    assert!(db.listar_investimentos(hoje()).unwrap().is_empty());
}

#[test]
fn atualiza_mantem_id_e_muda_campos() {
    let (_d, db) = abrir();
    let i = id(db.criar_investimento(&cdb(), hoje()).unwrap());
    let mut novo = cdb();
    novo.emissor = s("Banco Master");
    novo.valor_aplicado_centavos = Some(500_000);
    novo.taxa = s("102.50");
    assert!(matches!(
        db.atualizar_investimento(i, &novo, hoje()).unwrap(),
        Gravacao::Ok(x) if x == i
    ));
    let v = db.investimento(i, hoje()).unwrap();
    assert_eq!(v.id, i);
    assert_eq!(v.nome, "CDB Banco Master");
    assert_eq!(v.valor_aplicado_centavos, 500_000);
    assert_eq!(v.taxa.as_deref(), Some("102.5"));
    assert_eq!(v.rendimento.as_deref(), Some("102,5% do CDI"));
    assert_eq!(db.listar_investimentos(hoje()).unwrap().len(), 1);
}

#[test]
fn atualiza_invalido_nao_altera() {
    let (_d, db) = abrir();
    let i = id(db.criar_investimento(&cdb(), hoje()).unwrap());
    let mut ruim = cdb();
    ruim.emissor = None;
    assert!(matches!(
        db.atualizar_investimento(i, &ruim, hoje()).unwrap(),
        Gravacao::Invalido(_)
    ));
    assert_eq!(db.investimento(i, hoje()).unwrap().nome, "CDB Banco Inter");
}

#[test]
fn atualiza_id_inexistente_da_nao_encontrado() {
    let (_d, db) = abrir();
    let e = db.atualizar_investimento(99, &cdb(), hoje()).unwrap_err();
    assert_eq!(e.code(), "NAO_ENCONTRADO");
    assert_eq!(
        e.to_string(),
        "Este investimento não existe mais. Atualize a lista."
    );
    assert_eq!(
        db.investimento(99, hoje()).unwrap_err().code(),
        "NAO_ENCONTRADO"
    );
}

#[test]
fn apaga_e_some_da_lista() {
    let (_d, db) = abrir();
    let i = id(db.criar_investimento(&cdb(), hoje()).unwrap());
    db.apagar_investimento(i).unwrap();
    assert!(db.listar_investimentos(hoje()).unwrap().is_empty());
}

#[test]
fn apaga_id_inexistente_da_nao_encontrado() {
    let (_d, db) = abrir();
    let e = db.apagar_investimento(7).unwrap_err();
    assert_eq!(e.code(), "NAO_ENCONTRADO");
}

#[test]
fn lista_ordena_por_classe_e_nome() {
    let (_d, db) = abrir();
    id(db
        .criar_investimento(&rv("rv-fii", "HGLG11", 1, 100), hoje())
        .unwrap());
    id(db
        .criar_investimento(&rv("rv-acao", "VALE3", 1, 100), hoje())
        .unwrap());
    id(db
        .criar_investimento(&rv("rv-acao", "ITUB4", 1, 100), hoje())
        .unwrap());
    id(db.criar_investimento(&cdb(), hoje()).unwrap());
    let nomes: Vec<String> = db
        .listar_investimentos(hoje())
        .unwrap()
        .into_iter()
        .map(|i| i.nome)
        .collect();
    assert_eq!(nomes, ["CDB Banco Inter", "ITUB4", "VALE3", "HGLG11"]);
}

#[test]
fn resumo_por_classe() {
    let (_d, db) = abrir();
    id(db.criar_investimento(&cdb(), hoje()).unwrap());
    id(db.criar_investimento(&tesouro(), hoje()).unwrap());
    id(db
        .criar_investimento(&rv("rv-acao", "ITUB4", 150, 3142), hoje())
        .unwrap());
    id(db
        .criar_investimento(&rv("rv-fii", "HGLG11", 10, 16_000), hoje())
        .unwrap());
    let r = db.resumo_carteira(hoje()).unwrap();
    assert_eq!(r.total_aplicado_centavos, 4_431_300);
    let v: Vec<_> = r
        .classes
        .iter()
        .map(|c| (c.classe, c.total_aplicado_centavos, c.quantidade))
        .collect();
    assert_eq!(
        v,
        [
            (Classe::Rf, 3_800_000, 2),
            (Classe::Acoes, 471_300, 1),
            (Classe::Fii, 160_000, 1),
        ]
    );
}

#[test]
fn resumo_vazio() {
    let (_d, db) = abrir();
    let r = db.resumo_carteira(hoje()).unwrap();
    assert_eq!(r.total_aplicado_centavos, 0);
    assert!(r.classes.is_empty());
}

#[test]
fn dados_sobrevivem_a_reabrir() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    id(db.criar_investimento(&cdb(), hoje()).unwrap());
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(db.listar_investimentos(hoje()).unwrap().len(), 1);
}

#[test]
fn migra_banco_da_versao_1() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE app_meta (chave TEXT PRIMARY KEY, valor TEXT NOT NULL);
             PRAGMA user_version = 1;",
        )
        .unwrap();
    }
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), 9);
    assert_eq!(SCHEMA_VERSION, 9);
    assert!(db.listar_investimentos(hoje()).unwrap().is_empty());
}
