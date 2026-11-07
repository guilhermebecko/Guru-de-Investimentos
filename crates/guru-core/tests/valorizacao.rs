use std::str::FromStr;

use chrono::{Days, NaiveDate};
use guru_core::{
    CotaFundo, Db, Gravacao, NovaMeta, NovoInvestimento, Origem, PontoDiario, PrecoTesouro,
    ler_cotahist_txt, ler_sgs, ler_tesouro,
};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

fn data(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

fn hoje() -> NaiveDate {
    data(2026, 10, 28)
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn fixture(nome: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{nome}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Banco com todas as fixtures reais gravadas.
fn banco() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.gravar_indice("cdi", &cdi()).unwrap();
    db.gravar_indice("selic", &ler_sgs(&fixture("sgs-11.json")).unwrap())
        .unwrap();
    db.gravar_ipca(&ipca()).unwrap();
    db.gravar_tesouro(&ler_tesouro(&fixture("tesouro.csv")).unwrap())
        .unwrap();
    db.gravar_cotacoes(&ler_cotahist_txt(&fixture("COTAHIST_D28102026.TXT")).unwrap())
        .unwrap();
    (dir, db)
}

fn vazio() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    (dir, db)
}

fn cdi() -> Vec<PontoDiario> {
    ler_sgs(&fixture("sgs-12.json")).unwrap()
}

fn ipca() -> Vec<(String, Decimal)> {
    ler_sgs(&fixture("sgs-433.json"))
        .unwrap()
        .into_iter()
        .map(|p| (p.data.format("%Y-%m").to_string(), p.valor))
        .collect()
}

fn criar(db: &Db, n: NovoInvestimento) -> i64 {
    match db.criar_investimento(&n, hoje()).unwrap() {
        Gravacao::Ok(id) => id,
        Gravacao::Invalido(e) => panic!("inválido: {e:?}"),
    }
}

fn renda_fixa(indexador: &str, taxa: &str, aplicacao: &str, valor: i64) -> NovoInvestimento {
    NovoInvestimento {
        tipo: "rf-cdb".into(),
        instituicao: "Banco Inter".into(),
        emissor: s("Banco Inter"),
        indexador: s(indexador),
        taxa: s(taxa),
        benchmark: (indexador == "pos").then(|| "cdi".to_string()),
        data_aplicacao: s(aplicacao),
        vencimento: s("2030-03-15"),
        valor_aplicado_centavos: Some(valor),
        liquidez: s("vencimento"),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn acao(ticker: &str, qtd: i64, preco: i64) -> NovoInvestimento {
    NovoInvestimento {
        tipo: "rv-acao".into(),
        instituicao: "Rico".into(),
        ticker: s(ticker),
        data_aplicacao: s("2026-01-10"),
        quantidade: Some(qtd),
        preco_medio_centavos: Some(preco),
        ..Default::default()
    }
}

fn tesouro_selic(aplicacao: &str) -> NovoInvestimento {
    NovoInvestimento {
        tipo: "td-selic".into(),
        instituicao: "Rico".into(),
        taxa: s("0.09"),
        data_aplicacao: s(aplicacao),
        vencimento: s("2029-03-01"),
        valor_aplicado_centavos: Some(1_800_000),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn dec(v: &str) -> Decimal {
    Decimal::from_str(v).unwrap()
}

fn piso(d: Decimal) -> i64 {
    d.floor().to_i64().unwrap()
}

fn atual(db: &Db, id: i64) -> guru_core::Investimento {
    db.investimento(id, hoje()).unwrap()
}

fn perto(a: i64, b: i64) {
    assert!(
        (a - b).abs() <= 1,
        "{a} difere de {b} por mais de 1 centavo"
    );
}

/// `aplicado × Π (1 + cdi_d/100 × taxa/100)` dos dias da fixture em `(de, ate]`.
fn esperado_pos(pontos: &[PontoDiario], aplicado: i64, taxa: &str, de: NaiveDate) -> i64 {
    let mut f = Decimal::ONE;
    for p in pontos.iter().filter(|p| p.data > de && p.data <= hoje()) {
        f *= Decimal::ONE + p.valor / dec("100") * dec(taxa) / dec("100");
    }
    piso(Decimal::from(aplicado) * f)
}

#[test]
fn cdb_100_do_cdi_vem_do_indice_guardado() {
    let (_d, db) = banco();
    let id = criar(&db, renda_fixa("pos", "100", "2026-08-31", 1_000_000));
    let i = atual(&db, id);
    let esperado = esperado_pos(&cdi(), 1_000_000, "100", data(2026, 8, 31));
    perto(i.valor_atual_centavos, esperado);
    assert!(i.valor_atual_centavos > 1_000_000);
    assert_eq!(i.origem_valor, Origem::Calculo);
    assert_eq!(i.data_valor, Some(data(2026, 10, 28)));
    assert!(i.rentabilidade_decimos > 0);
}

#[test]
fn cdb_110_do_cdi_rende_mais_que_100() {
    let (_d, db) = banco();
    let a = criar(&db, renda_fixa("pos", "100", "2026-08-31", 1_000_000));
    let b = criar(&db, renda_fixa("pos", "110", "2026-08-31", 1_000_000));
    let (a, b) = (atual(&db, a), atual(&db, b));
    assert!(b.valor_atual_centavos > a.valor_atual_centavos);
    perto(
        b.valor_atual_centavos,
        esperado_pos(&cdi(), 1_000_000, "110", data(2026, 8, 31)),
    );
}

#[test]
fn aplicacao_de_hoje_vale_o_aplicado() {
    let (_d, db) = banco();
    let id = criar(&db, renda_fixa("pos", "100", "2026-10-28", 1_000_000));
    let i = atual(&db, id);
    assert_eq!(i.valor_atual_centavos, 1_000_000);
    assert_eq!(i.rentabilidade_decimos, 0);
    assert_eq!(i.origem_valor, Origem::Calculo);
}

#[test]
fn aplicacao_no_sabado_conta_a_partir_do_dia_util_seguinte() {
    let (_d, db) = banco();
    let sabado = criar(&db, renda_fixa("pos", "100", "2026-10-24", 1_000_000));
    let sexta = criar(&db, renda_fixa("pos", "100", "2026-10-23", 1_000_000));
    let sabado = atual(&db, sabado).valor_atual_centavos;
    assert_eq!(sabado, atual(&db, sexta).valor_atual_centavos);
    perto(
        sabado,
        esperado_pos(&cdi(), 1_000_000, "100", data(2026, 10, 24)),
    );
    assert!(sabado > 1_000_000);
}

#[test]
fn pos_com_selic_usa_a_serie_selic() {
    let (_d, db) = banco();
    let mut n = renda_fixa("pos", "100", "2026-08-31", 1_000_000);
    n.benchmark = s("selic");
    let id = criar(&db, n);
    let selic = ler_sgs(&fixture("sgs-11.json")).unwrap();
    perto(
        atual(&db, id).valor_atual_centavos,
        esperado_pos(&selic, 1_000_000, "100", data(2026, 8, 31)),
    );
}

#[test]
fn prefixado_com_252_dias_uteis_rende_a_taxa_cheia() {
    let (_d, db) = vazio();
    let inicio = data(2025, 1, 2);
    let pontos: Vec<PontoDiario> = (0..252)
        .map(|i| PontoDiario {
            data: inicio.checked_add_days(Days::new(i)).unwrap(),
            valor: dec("0.05"),
        })
        .collect();
    db.gravar_indice("cdi", &pontos).unwrap();
    let ultimo = pontos.last().unwrap().data;
    let id = criar(&db, renda_fixa("pre", "12", "2025-01-01", 1_000_000));
    let i = db.investimento(id, ultimo).unwrap();
    perto(i.valor_atual_centavos, 1_120_000);
    assert_eq!(i.rentabilidade_decimos, 120);
    assert_eq!(i.origem_valor, Origem::Calculo);
    assert_eq!(i.data_valor, Some(ultimo));
}

#[test]
fn ipca_mais_taxa_usa_so_meses_inteiros() {
    let (_d, db) = vazio();
    db.gravar_ipca(&ipca()).unwrap();
    // Um ponto de CDI por semana, a partir de 17/03/2025: só serve para contar dias úteis.
    let semanal: Vec<PontoDiario> = (0..)
        .map(|i| {
            data(2025, 3, 17)
                .checked_add_days(Days::new(7 * i))
                .unwrap()
        })
        .take_while(|d| *d <= hoje())
        .map(|d| PontoDiario {
            data: d,
            valor: dec("0.05"),
        })
        .collect();
    db.gravar_indice("cdi", &semanal).unwrap();
    let du = semanal.len() as f64;
    let id = criar(&db, renda_fixa("ipca", "6", "2025-03-15", 1_000_000));

    // Abril/2025 até setembro/2026; março (mês da aplicação) e outubro (incompleto) ficam de fora.
    let fator = |de: &str| -> Decimal {
        ipca()
            .iter()
            .filter(|(m, _)| m.as_str() >= de && m.as_str() <= "2026-09")
            .fold(Decimal::ONE, |f, (_, v)| {
                f * (Decimal::ONE + *v / dec("100"))
            })
    };
    let real = 1.06_f64.powf(du / 252.0);
    let esperado = |de: &str| (1_000_000.0 * fator(de).to_f64().unwrap() * real).floor() as i64;

    let i = atual(&db, id);
    perto(i.valor_atual_centavos, esperado("2025-04"));
    assert_ne!(esperado("2025-04"), esperado("2025-03"));
    assert_eq!(i.origem_valor, Origem::Calculo);
}

#[test]
fn tesouro_selic_usa_a_quantidade_da_compra_e_o_pu_base() {
    let (_d, db) = banco();
    let id = criar(&db, tesouro_selic("2025-03-12"));
    let precos: Vec<PrecoTesouro> = ler_tesouro(&fixture("tesouro.csv"))
        .unwrap()
        .into_iter()
        .filter(|p| p.tipo == "Tesouro Selic" && p.vencimento == data(2029, 3, 1))
        .collect();
    let compra = precos
        .iter()
        .filter(|p| p.data <= data(2025, 3, 12))
        .max_by_key(|p| p.data)
        .unwrap();
    let base = precos
        .iter()
        .filter(|p| p.data <= hoje())
        .max_by_key(|p| p.data)
        .unwrap();
    assert_eq!(base.data, hoje());
    let esperado = piso(Decimal::from(1_800_000) / compra.pu_compra * base.pu_base);
    let i = atual(&db, id);
    assert_eq!(i.valor_atual_centavos, esperado);
    assert_eq!(i.origem_valor, Origem::Tesouro);
    assert_eq!(i.data_valor, Some(base.data));
    assert!(i.valor_atual_centavos > 1_800_000);
}

#[test]
fn tesouro_sem_preco_na_data_da_compra_fica_sem_dados() {
    let (_d, db) = banco();
    let id = criar(&db, tesouro_selic("2025-01-10"));
    let i = atual(&db, id);
    assert_eq!(i.valor_atual_centavos, 1_800_000);
    assert_eq!(i.origem_valor, Origem::SemDados);
}

#[test]
fn acao_vale_quantidade_vezes_o_fechamento() {
    let (_d, db) = banco();
    let id = criar(&db, acao("ITUB4", 150, 3142));
    let i = atual(&db, id);
    assert_eq!(i.valor_atual_centavos, 150 * 3978);
    assert_eq!(i.origem_valor, Origem::Cotacao);
    assert_eq!(i.data_valor, Some(hoje()));
    // (596.700 − 471.300) / 471.300 = +26,6%
    assert_eq!(i.rentabilidade_decimos, 266);
}

#[test]
fn ticker_sem_cotacao_mostra_o_aplicado() {
    let (_d, db) = banco();
    let id = criar(&db, acao("XXXX3", 100, 1000));
    let i = atual(&db, id);
    assert_eq!(i.valor_atual_centavos, 100_000);
    assert_eq!(i.rentabilidade_decimos, 0);
    assert_eq!(i.origem_valor, Origem::SemDados);
    assert_eq!(i.data_valor, None);
}

#[test]
fn cripto_fundo_e_previdencia_sao_sem_fonte() {
    let (_d, db) = banco();
    for tipo in ["cripto", "fundo", "prev"] {
        let id = criar(
            &db,
            NovoInvestimento {
                tipo: tipo.into(),
                instituicao: "Binance".into(),
                emissor: s("Emissor"),
                data_aplicacao: s("2026-01-10"),
                valor_aplicado_centavos: Some(500_000),
                liquidez: s("diaria"),
                ..Default::default()
            },
        );
        let i = atual(&db, id);
        assert_eq!(i.origem_valor, Origem::SemFonte, "{tipo}");
        assert_eq!(i.valor_atual_centavos, 500_000);
        assert_eq!(i.rentabilidade_decimos, 0);
    }
}

#[test]
fn prejuizo_tem_rentabilidade_negativa() {
    let (_d, db) = banco();
    let id = criar(&db, acao("ITUB4", 150, 5000));
    let i = atual(&db, id);
    // (596.700 − 750.000) / 750.000 = −20,44% → −204
    assert_eq!(i.rentabilidade_decimos, -204);
}

#[test]
fn serie_que_nao_cobre_a_aplicacao_vale_o_aplicado() {
    let (_d, db) = banco();
    // O CDI guardado começa em 01/09/2026: 15/08 está a mais de 7 dias antes disso.
    let longe = criar(&db, renda_fixa("pos", "100", "2026-08-15", 1_000_000));
    let perto_ = criar(&db, renda_fixa("pos", "100", "2026-08-28", 1_000_000));
    let longe = atual(&db, longe);
    assert_eq!(longe.valor_atual_centavos, 1_000_000);
    assert_eq!(longe.origem_valor, Origem::SemDados);
    assert_eq!(longe.data_valor, None);
    assert_eq!(atual(&db, perto_).origem_valor, Origem::Calculo);
}

#[test]
fn sem_nenhum_dado_guardado_tudo_vale_o_aplicado() {
    let (_d, db) = vazio();
    let a = criar(&db, renda_fixa("pos", "100", "2026-08-31", 1_000_000));
    let b = criar(&db, renda_fixa("pre", "12", "2025-01-01", 700_000));
    let c = criar(&db, tesouro_selic("2025-03-12"));
    let d = criar(&db, acao("ITUB4", 150, 3142));
    for (id, valor) in [(a, 1_000_000), (b, 700_000), (c, 1_800_000), (d, 471_300)] {
        let i = atual(&db, id);
        assert_eq!(i.valor_atual_centavos, valor);
        assert_eq!(i.origem_valor, Origem::SemDados);
        assert_eq!(i.rentabilidade_decimos, 0);
    }
}

#[test]
fn ipca_sem_o_primeiro_mes_inteiro_guardado_vale_o_aplicado() {
    let (_d, db) = vazio();
    db.gravar_ipca(&ipca()).unwrap();
    let semanal: Vec<PontoDiario> = (0..)
        .map(|i| {
            data(2024, 11, 18)
                .checked_add_days(Days::new(7 * i))
                .unwrap()
        })
        .take_while(|d| *d <= hoje())
        .map(|d| PontoDiario {
            data: d,
            valor: dec("0.05"),
        })
        .collect();
    db.gravar_indice("cdi", &semanal).unwrap();
    // O IPCA guardado começa em 2025-01: aplicar em 20/12/2024 começa em 2025-01 (guardado);
    // aplicar em 20/11/2024 começaria em 2024-12 (ausente).
    let ok = criar(&db, renda_fixa("ipca", "6", "2024-12-20", 1_000_000));
    let falta = criar(&db, renda_fixa("ipca", "6", "2024-11-20", 1_000_000));
    assert_eq!(atual(&db, ok).origem_valor, Origem::Calculo);
    let f = atual(&db, falta);
    assert_eq!(f.origem_valor, Origem::SemDados);
    assert_eq!(f.valor_atual_centavos, 1_000_000);
}

#[test]
fn data_antes_da_aplicacao_vale_o_aplicado() {
    let (_d, db) = banco();
    let id = criar(&db, renda_fixa("pos", "100", "2026-10-20", 1_000_000));
    let i = db.investimento(id, data(2026, 10, 10)).unwrap();
    assert_eq!(i.valor_atual_centavos, 1_000_000);
    assert_eq!(i.origem_valor, Origem::SemDados);
}

#[test]
fn origem_tem_codigo_e_rotulo() {
    let t = [
        (Origem::Calculo, "calculo", "calculado pelo índice"),
        (Origem::Tesouro, "tesouro", "preço do Tesouro"),
        (Origem::Cotacao, "cotacao", "cotação da B3"),
        (Origem::Cota, "cota", "cota diária da CVM"),
        (Origem::SemFonte, "sem_fonte", "sem fonte de preço"),
        (Origem::SemDados, "sem_dados", "sem dados ainda"),
    ];
    for (o, c, r) in t {
        assert_eq!((o.codigo(), o.rotulo()), (c, r));
    }
}

#[test]
fn resumo_e_metas_usam_o_valor_atual() {
    let (_d, db) = banco();
    let Ok(meta) = db
        .criar_meta(
            &NovaMeta {
                nome: "Reserva".into(),
                valor_alvo_centavos: Some(1_000_000),
                prazo: s("2027-12"),
            },
            hoje(),
        )
        .unwrap()
    else {
        panic!("meta inválida")
    };
    let mut n = acao("ITUB4", 150, 3142);
    n.meta_id = Some(meta);
    criar(&db, n);
    criar(
        &db,
        NovoInvestimento {
            tipo: "fundo".into(),
            instituicao: "XP".into(),
            emissor: s("XP"),
            data_aplicacao: s("2026-01-10"),
            valor_aplicado_centavos: Some(596_700),
            liquidez: s("d30"),
            ..Default::default()
        },
    );
    let m = db.meta(meta, hoje()).unwrap();
    assert_eq!(m.acumulado_centavos, 596_700);

    let r = db.resumo_carteira(hoje()).unwrap();
    assert_eq!(r.total_aplicado_centavos, 471_300 + 596_700);
    assert_eq!(r.total_atual_centavos, 596_700 * 2);
    for c in &r.classes {
        assert_eq!(c.total_atual_centavos, 596_700);
        assert_eq!(c.percentual_decimos, 500);
    }
    assert_eq!(
        r.classes
            .iter()
            .map(|c| c.total_aplicado_centavos)
            .sum::<i64>(),
        r.total_aplicado_centavos
    );
}

#[test]
fn ipca_recente_sem_mes_publicado_ainda_calcula_so_com_a_taxa() {
    let (_d, db) = vazio();
    db.gravar_ipca(&ipca()).unwrap();
    // Um ponto de CDI por semana a partir de 10/09/2026: só serve para contar dias úteis.
    let semanal: Vec<PontoDiario> = (0..)
        .map(|i| {
            data(2026, 9, 10)
                .checked_add_days(Days::new(7 * i))
                .unwrap()
        })
        .take_while(|d| *d <= hoje())
        .map(|d| PontoDiario {
            data: d,
            valor: dec("0.05"),
        })
        .collect();
    db.gravar_indice("cdi", &semanal).unwrap();
    let du = semanal
        .iter()
        .filter(|p| p.data > data(2026, 9, 15))
        .count() as f64;
    // Setembro/2026 ainda não terminou na aplicação; outubro não terminou em 28/10: nenhum mês entra.
    let id = criar(&db, renda_fixa("ipca", "6", "2026-09-15", 1_000_000));
    let i = atual(&db, id);
    assert_eq!(i.origem_valor, Origem::Calculo);
    perto(
        i.valor_atual_centavos,
        (1_000_000.0 * 1.06_f64.powf(du / 252.0)).floor() as i64,
    );
}

fn fundo_com_saldo(dir: &tempfile::TempDir, db: &Db) -> i64 {
    let id = criar(
        db,
        NovoInvestimento {
            tipo: "fundo".into(),
            instituicao: "BTG Pactual".into(),
            emissor: s("Fundo X"),
            data_aplicacao: s("2026-01-10"),
            valor_aplicado_centavos: Some(100_000),
            liquidez: s("diaria"),
            ..Default::default()
        },
    );
    let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
    conn.execute(
        "UPDATE investimentos SET saldo_centavos = 105000, saldo_data = '2026-10-30' WHERE id = ?1",
        [id],
    )
    .unwrap();
    id
}

#[test]
fn fundo_com_saldo_do_extrato() {
    let (d, db) = vazio();
    let id = fundo_com_saldo(&d, &db);
    let i = db.investimento(id, data(2026, 11, 1)).unwrap();
    assert_eq!(i.valor_atual_centavos, 105_000);
    assert_eq!(i.origem_valor, Origem::Extrato);
    assert_eq!(i.origem_valor.codigo(), "extrato");
    assert_eq!(i.data_valor, Some(data(2026, 10, 30)));
    assert_eq!(i.rentabilidade_decimos, 50);
}

#[test]
fn saldo_futuro_nao_vale_para_datas_anteriores() {
    let (d, db) = vazio();
    let id = fundo_com_saldo(&d, &db);
    let i = db.investimento(id, data(2026, 10, 1)).unwrap();
    assert_eq!(i.valor_atual_centavos, 100_000);
    assert_eq!(i.origem_valor, Origem::SemFonte);
}

const CNPJ_FUNDO: &str = "11222333000181";

fn cotas(db: &Db, itens: &[((i32, u32, u32), &str)]) {
    let v: Vec<CotaFundo> = itens
        .iter()
        .map(|&((a, m, d), c)| CotaFundo {
            cnpj: CNPJ_FUNDO.into(),
            data: data(a, m, d),
            cota: Decimal::from_str(c).unwrap(),
        })
        .collect();
    db.gravar_cotas_fundo(&v).unwrap();
}

fn fundo_cnpj(
    dir: &tempfile::TempDir,
    db: &Db,
    aplicacao: &str,
    aplicado: i64,
    cnpj: Option<&str>,
) -> i64 {
    let id = criar(
        db,
        NovoInvestimento {
            tipo: "fundo".into(),
            instituicao: "BTG Pactual".into(),
            emissor: s("Fundo X"),
            data_aplicacao: s(aplicacao),
            valor_aplicado_centavos: Some(aplicado),
            liquidez: s("diaria"),
            ..Default::default()
        },
    );
    if let Some(c) = cnpj {
        let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
        conn.execute(
            "UPDATE investimentos SET codigo_b3 = ?1 WHERE id = ?2",
            rusqlite::params![c, id],
        )
        .unwrap();
    }
    id
}

fn grava_saldo(dir: &tempfile::TempDir, id: i64, saldo: i64, em: &str) {
    let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
    conn.execute(
        "UPDATE investimentos SET saldo_centavos = ?1, saldo_data = ?2 WHERE id = ?3",
        rusqlite::params![saldo, em, id],
    )
    .unwrap();
}

#[test]
fn fundo_manual_pela_cota() {
    let (d, db) = vazio();
    cotas(&db, &[((2026, 9, 8), "1.40"), ((2026, 11, 5), "1.4989")]);
    let id = fundo_cnpj(&d, &db, "2026-09-08", 3_000_000, Some(CNPJ_FUNDO));
    let i = db.investimento(id, data(2026, 11, 7)).unwrap();
    assert_eq!(i.origem_valor, Origem::Cota);
    assert_eq!(i.origem_valor.codigo(), "cota");
    assert_eq!(i.origem_valor.rotulo(), "cota diária da CVM");
    assert_eq!(i.valor_atual_centavos, 3_211_928);
    assert_eq!(i.data_valor, Some(data(2026, 11, 5)));
}

#[test]
fn fundo_aplicado_no_fim_de_semana_usa_cota_anterior() {
    let (d, db) = vazio();
    cotas(
        &db,
        &[
            ((2026, 9, 4), "1.40"),
            ((2026, 9, 8), "1.50"),
            ((2026, 11, 5), "1.4989"),
        ],
    );
    // Domingo 06/09: a cota base é a de 04/09.
    let id = fundo_cnpj(&d, &db, "2026-09-06", 1_400_000, Some(CNPJ_FUNDO));
    let i = db.investimento(id, data(2026, 11, 7)).unwrap();
    assert_eq!(i.valor_atual_centavos, 1_498_900);
}

#[test]
fn fundo_com_saldo_projeta_o_saldo() {
    let (d, db) = vazio();
    cotas(&db, &[((2026, 10, 30), "2.00"), ((2026, 11, 5), "2.02")]);
    let id = fundo_cnpj(&d, &db, "2026-01-10", 4_000_000, Some(CNPJ_FUNDO));
    grava_saldo(&d, id, 5_000_000, "2026-10-31");
    let i = db.investimento(id, data(2026, 11, 7)).unwrap();
    assert_eq!(i.valor_atual_centavos, 5_050_000);
    assert_eq!(i.origem_valor, Origem::Cota);
    let i = db.investimento(id, data(2026, 10, 31)).unwrap();
    assert_eq!(i.valor_atual_centavos, 5_000_000);
    assert_eq!(i.origem_valor, Origem::Extrato);
}

#[test]
fn fundo_com_cnpj_sem_cotas() {
    let (d, db) = vazio();
    let id = fundo_cnpj(&d, &db, "2026-01-10", 100_000, Some(CNPJ_FUNDO));
    let i = db.investimento(id, data(2026, 11, 7)).unwrap();
    assert_eq!(i.origem_valor, Origem::SemDados);
    assert_eq!(i.valor_atual_centavos, 100_000);
    grava_saldo(&d, id, 105_000, "2026-10-30");
    let i = db.investimento(id, data(2026, 11, 7)).unwrap();
    assert_eq!(i.origem_valor, Origem::Extrato);
    assert_eq!(i.valor_atual_centavos, 105_000);
}

#[test]
fn fundo_sem_cnpj_continua_sem_fonte() {
    let (d, db) = vazio();
    cotas(&db, &[((2026, 1, 10), "1.00"), ((2026, 11, 5), "2.00")]);
    let id = fundo_cnpj(&d, &db, "2026-01-10", 100_000, None);
    let i = db.investimento(id, data(2026, 11, 7)).unwrap();
    assert_eq!(i.origem_valor, Origem::SemFonte);
    assert_eq!(i.valor_atual_centavos, 100_000);
}

#[test]
fn prev_e_cripto_nao_mudam() {
    let (_d, db) = vazio();
    cotas(&db, &[((2026, 1, 10), "1.00"), ((2026, 11, 5), "2.00")]);
    for tipo in ["prev", "cripto"] {
        let id = criar(
            &db,
            NovoInvestimento {
                tipo: tipo.into(),
                instituicao: "X".into(),
                emissor: s("X"),
                data_aplicacao: s("2026-01-10"),
                valor_aplicado_centavos: Some(100_000),
                liquidez: s("diaria"),
                ..Default::default()
            },
        );
        let i = db.investimento(id, data(2026, 11, 7)).unwrap();
        assert_eq!(i.origem_valor, Origem::SemFonte, "{tipo}");
    }
}

#[test]
fn fundo_antes_da_aplicacao_sem_dados() {
    let (d, db) = vazio();
    cotas(&db, &[((2026, 9, 8), "1.40"), ((2026, 11, 5), "1.50")]);
    let id = fundo_cnpj(&d, &db, "2026-09-08", 100_000, Some(CNPJ_FUNDO));
    let i = db.investimento(id, data(2026, 9, 1)).unwrap();
    assert_eq!(i.origem_valor, Origem::SemDados);
    assert_eq!(i.valor_atual_centavos, 100_000);
}
