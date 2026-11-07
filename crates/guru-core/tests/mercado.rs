use chrono::NaiveDate;
use guru_core::{
    CotaFundo, Db, PontoDiario, PrecoTesouro, SCHEMA_VERSION, StatusFonte, ler_cotahist_txt,
    ler_sgs, ler_tesouro,
};
use rust_decimal::Decimal;

fn data(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

fn banco() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    (dir, db)
}

fn fixture(nome: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{nome}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn gravar_cdi_duas_vezes_nao_duplica() {
    let (_d, db) = banco();
    let pontos = ler_sgs(&fixture("sgs-12.json")).unwrap();
    assert!(!pontos.is_empty());
    assert_eq!(db.gravar_indice("cdi", &pontos).unwrap(), pontos.len());
    assert_eq!(db.gravar_indice("cdi", &pontos).unwrap(), 0);
    let lidos = db
        .indice_entre("cdi", data(1900, 1, 1), data(2100, 1, 1))
        .unwrap();
    assert_eq!(lidos.len(), pontos.len());
    assert_eq!(lidos[0].valor, pontos[0].valor);
    let ultima = pontos.iter().map(|p| p.data).max().unwrap();
    assert_eq!(db.ultima_data_indice("cdi").unwrap(), Some(ultima));
    assert_eq!(db.ultima_data_indice("selic").unwrap(), None);
}

#[test]
fn indice_atualiza_valor_e_filtra_por_intervalo() {
    let (_d, db) = banco();
    let p = |d, v: &str| PontoDiario {
        data: d,
        valor: v.parse().unwrap(),
    };
    db.gravar_indice(
        "selic",
        &[p(data(2026, 10, 27), "0.05"), p(data(2026, 10, 28), "0.06")],
    )
    .unwrap();
    let novos = db
        .gravar_indice("selic", &[p(data(2026, 10, 28), "0.07")])
        .unwrap();
    assert_eq!(novos, 0);
    let l = db
        .indice_entre("selic", data(2026, 10, 28), data(2026, 10, 28))
        .unwrap();
    assert_eq!(l, vec![p(data(2026, 10, 28), "0.07")]);
}

#[test]
fn ipca_guarda_mes_aaaa_mm() {
    let (_d, db) = banco();
    let pontos = ler_sgs(&fixture("sgs-433.json")).unwrap();
    let meses: Vec<(String, Decimal)> = pontos
        .iter()
        .map(|p| (p.data.format("%Y-%m").to_string(), p.valor))
        .collect();
    assert_eq!(db.gravar_ipca(&meses).unwrap(), meses.len());
    assert_eq!(db.gravar_ipca(&meses).unwrap(), 0);
    let todos = db.ipca_meses("0000-01", "9999-12").unwrap();
    assert_eq!(todos.len(), meses.len());
    assert_eq!(todos[0].0.len(), 7);
    let primeiro = &meses.iter().min_by_key(|m| m.0.clone()).unwrap().0;
    assert_eq!(db.ipca_meses(primeiro, primeiro).unwrap().len(), 1);
}

#[test]
fn cotacao_em_devolve_a_do_ultimo_pregao() {
    let (_d, db) = banco();
    let c = ler_cotahist_txt(&fixture("COTAHIST_D28102026.TXT")).unwrap();
    assert_eq!(db.gravar_cotacoes(&c).unwrap(), c.len());
    assert_eq!(db.gravar_cotacoes(&c).unwrap(), 0);
    let itub = db.cotacao_em("ITUB4", data(2026, 10, 30)).unwrap().unwrap();
    assert_eq!(itub.data, data(2026, 10, 28));
    assert_eq!(itub.fechamento_centavos, 3978);
    assert!(
        db.cotacao_em("ITUB4", data(2026, 10, 27))
            .unwrap()
            .is_none()
    );
    assert!(
        db.cotacao_em("XXXX3", data(2026, 10, 30))
            .unwrap()
            .is_none()
    );
    assert_eq!(db.datas_cotacao().unwrap(), vec![data(2026, 10, 28)]);
}

#[test]
fn cotacao_nao_aceita_fechamento_zero() {
    let (_d, db) = banco();
    let c = guru_core::Cotacao {
        ticker: "ABCD3".into(),
        data: data(2026, 10, 28),
        fechamento_centavos: 0,
    };
    assert!(db.gravar_cotacoes(&[c]).is_err());
    assert!(db.datas_cotacao().unwrap().is_empty());
}

#[test]
fn preco_tesouro_sem_data_exata_devolve_o_anterior() {
    let (_d, db) = banco();
    let precos = ler_tesouro(&fixture("tesouro.csv")).unwrap();
    assert!(!precos.is_empty());
    assert_eq!(db.gravar_tesouro(&precos).unwrap(), precos.len());
    assert_eq!(db.gravar_tesouro(&precos).unwrap(), 0);

    let p = precos.iter().max_by_key(|p| p.data).unwrap();
    let exato = db
        .preco_tesouro_em(&p.tipo, p.vencimento, p.data)
        .unwrap()
        .unwrap();
    assert_eq!(&exato, p);
    let depois = db
        .preco_tesouro_em(&p.tipo, p.vencimento, p.data.succ_opt().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(depois.data, p.data);
    assert!(
        db.preco_tesouro_em(&p.tipo, p.vencimento, data(1990, 1, 1))
            .unwrap()
            .is_none()
    );
}

#[test]
fn preco_tesouro_pega_o_ultimo_anterior_entre_varios() {
    let (_d, db) = banco();
    let mk = |d: NaiveDate, base: &str| PrecoTesouro {
        tipo: "Tesouro Selic".into(),
        vencimento: data(2029, 3, 1),
        data: d,
        taxa_compra: "0.1".parse().unwrap(),
        pu_compra: "100".parse().unwrap(),
        pu_base: base.parse().unwrap(),
    };
    db.gravar_tesouro(&[mk(data(2026, 10, 23), "1.5"), mk(data(2026, 10, 27), "1.6")])
        .unwrap();
    let r = db
        .preco_tesouro_em("Tesouro Selic", data(2029, 3, 1), data(2026, 10, 26))
        .unwrap()
        .unwrap();
    assert_eq!(r.data, data(2026, 10, 23));
    assert_eq!(r.pu_base, "1.5".parse::<Decimal>().unwrap());
}

#[test]
fn status_fontes_tem_seis_na_ordem() {
    let (_d, db) = banco();
    let s = db.status_fontes().unwrap();
    let nomes: Vec<&str> = s.iter().map(|x| x.fonte.as_str()).collect();
    assert_eq!(nomes, ["cdi", "selic", "ipca", "tesouro", "b3", "cvm"]);
    assert!(
        s.iter()
            .all(|x| x.ultima_data.is_none() && x.atualizado_em.is_none() && x.erro.is_none())
    );
}

#[test]
fn registrar_status_substitui_e_mantem_a_ordem() {
    let (_d, db) = banco();
    let st = |erro: Option<&str>| StatusFonte {
        fonte: "tesouro".into(),
        ultima_data: Some("2026-10-28".into()),
        atualizado_em: Some("2026-10-29T10:00:00".into()),
        erro: erro.map(String::from),
    };
    db.registrar_status(&st(None)).unwrap();
    db.registrar_status(&st(Some("fora do ar"))).unwrap();
    let s = db.status_fontes().unwrap();
    assert_eq!(s[3], st(Some("fora do ar")));
    assert_eq!(s[0].fonte, "cdi");
    assert!(s[0].erro.is_none());
}

#[test]
fn migra_banco_da_versao_3() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
        conn.execute_batch(include_str!("../src/migrations/0001_inicial.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../src/migrations/0002_investimentos.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../src/migrations/0003_perfil_metas.sql"))
            .unwrap();
        conn.execute(
            "INSERT INTO investimentos (tipo, instituicao, data_aplicacao, \
             valor_aplicado_centavos, criado_em, atualizado_em) \
             VALUES ('rf-cdb', 'A', '2025-01-01', 100000, 'x', 'x')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO metas (nome, valor_alvo_centavos, prazo, criado_em, atualizado_em) \
             VALUES ('Casa', 5000000, '2030-01-01', 'x', 'x')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO perfis (perfil, pontos, respostas, avisos, feito_em) \
             VALUES ('moderado', 17, '[]', '[]', '2026-10-29')",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
    }
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), 9);
    assert_eq!(SCHEMA_VERSION, 9);
    let conn = rusqlite::Connection::open(db.path()).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM investimentos", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
    for tabela in ["metas", "perfis"] {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {tabela}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "{tabela}");
    }
    assert_eq!(db.status_fontes().unwrap().len(), 6);
}

#[test]
fn cotas_de_fundo_upsert_e_busca() {
    let (_d, db) = banco();
    let c = |d: u32, v: &str| CotaFundo {
        cnpj: "11222333000181".into(),
        data: data(2026, 11, d),
        cota: v.parse().unwrap(),
    };
    let cnpj = "11222333000181";
    assert_eq!(
        db.gravar_cotas_fundo(&[c(3, "1.10"), c(4, "1.20"), c(5, "1.30")])
            .unwrap(),
        3
    );
    assert_eq!(db.gravar_cotas_fundo(&[c(5, "1.35")]).unwrap(), 0);
    let (d, v) = db.cota_fundo_em(cnpj, data(2026, 11, 8)).unwrap().unwrap();
    assert_eq!(d, data(2026, 11, 5));
    assert_eq!(v, "1.35".parse::<Decimal>().unwrap());
    assert_eq!(db.cota_fundo_em(cnpj, data(2026, 11, 2)).unwrap(), None);
    assert_eq!(
        db.ultima_data_cota_fundo().unwrap(),
        Some(data(2026, 11, 5))
    );
}
