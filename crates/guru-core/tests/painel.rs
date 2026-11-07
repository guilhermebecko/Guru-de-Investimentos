use chrono::NaiveDate;
use guru_core::{
    Classe, Cotacao, Db, Gravacao, NovoInvestimento, PontoDiario, Referencia, acumulado_benchmark,
    benchmark_padrao, datas_painel, historico, ler_cotahist_txt, ler_sgs, ler_tesouro, painel,
};
use rust_decimal::Decimal;

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

fn banco() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let cdi: Vec<PontoDiario> = ler_sgs(&fixture("sgs-12.json")).unwrap();
    db.gravar_indice("cdi", &cdi).unwrap();
    db.gravar_indice("selic", &ler_sgs(&fixture("sgs-11.json")).unwrap())
        .unwrap();
    let ipca: Vec<(String, Decimal)> = ler_sgs(&fixture("sgs-433.json"))
        .unwrap()
        .into_iter()
        .map(|p| (p.data.format("%Y-%m").to_string(), p.valor))
        .collect();
    db.gravar_ipca(&ipca).unwrap();
    db.gravar_tesouro(&ler_tesouro(&fixture("tesouro.csv")).unwrap())
        .unwrap();
    db.gravar_cotacoes(&ler_cotahist_txt(&fixture("COTAHIST_D28102026.TXT")).unwrap())
        .unwrap();
    (dir, db)
}

fn criar(db: &Db, n: NovoInvestimento) {
    match db.criar_investimento(&n, hoje()).unwrap() {
        Gravacao::Ok(_) => {}
        Gravacao::Invalido(e) => panic!("inválido: {e:?}"),
    }
}

fn cdb(aplicacao: &str, valor: i64) -> NovoInvestimento {
    NovoInvestimento {
        tipo: "rf-cdb".into(),
        instituicao: "Banco Inter".into(),
        emissor: s("Banco Inter"),
        indexador: s("pos"),
        taxa: s("100"),
        benchmark: s("cdi"),
        data_aplicacao: s(aplicacao),
        vencimento: s("2030-03-15"),
        valor_aplicado_centavos: Some(valor),
        liquidez: s("vencimento"),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn itub4(aplicacao: &str) -> NovoInvestimento {
    NovoInvestimento {
        tipo: "rv-acao".into(),
        instituicao: "Rico".into(),
        ticker: s("ITUB4"),
        data_aplicacao: s(aplicacao),
        quantidade: Some(150),
        preco_medio_centavos: Some(3500),
        ..Default::default()
    }
}

#[test]
fn periodo_invalido_tem_codigo() {
    let (_d, db) = banco();
    let e = historico(&db, hoje(), 7, None).unwrap_err();
    assert_eq!(e.code(), "PERIODO_INVALIDO");
}

#[test]
fn acao_aplicada_em_outubro_so_aparece_hoje() {
    let (_d, db) = banco();
    criar(&db, itub4("2026-10-01"));
    let h = historico(&db, hoje(), 12, None).unwrap();
    assert_eq!(h.pontos.len(), 13);
    for p in &h.pontos[..12] {
        assert_eq!(p.patrimonio_centavos, 0);
        assert_eq!(p.aportado_centavos, 0);
    }
    let ultimo = h.pontos.last().unwrap();
    assert_eq!(ultimo.patrimonio_centavos, 150 * 3978);
    assert_eq!(ultimo.aportado_centavos, 150 * 3500);
}

#[test]
fn carteira_vazia_nao_quebra() {
    let (_d, db) = banco();
    let h = historico(&db, hoje(), 6, None).unwrap();
    assert_eq!(h.pontos.len(), 7);
    assert!(h.por_classe.is_empty());
    assert!(h.pontos.iter().all(|p| p.patrimonio_centavos == 0));
}

#[test]
fn cdb_aplicado_no_ultimo_dia_do_mes_entra_naquele_ponto() {
    let (_d, db) = banco();
    criar(&db, cdb("2026-09-30", 1_000_000));
    let h = historico(&db, hoje(), 6, None).unwrap();
    let set = &h.pontos[5];
    assert_eq!(set.data, data(2026, 9, 30));
    assert_eq!(set.patrimonio_centavos, 1_000_000);
    assert_eq!(set.aportado_centavos, 1_000_000);
    assert_eq!(h.pontos[4].aportado_centavos, 0);
    // Em 28/10 o CDB rendeu: primeiro retorno válido, sobre o patrimônio de 30/09.
    let ultimo = h.pontos.last().unwrap();
    assert!(ultimo.patrimonio_centavos > 1_000_000);
    assert!(ultimo.retorno_centesimos.unwrap() > 0);
    assert_eq!(set.retorno_centesimos, None);
    assert_eq!(
        ultimo.acumulado_centesimos,
        ultimo.retorno_centesimos.unwrap()
    );
}

#[test]
fn aporte_no_intervalo_sai_do_retorno() {
    let (_d, db) = banco();
    criar(&db, cdb("2026-08-15", 1_000_000));
    criar(&db, cdb("2026-10-05", 500_000));
    let h = historico(&db, hoje(), 6, None).unwrap();
    let ultimo = h.pontos.last().unwrap();
    let set = &h.pontos[5];
    // O aporte de outubro não pode inflar o retorno: ele ficaria perto de 50% se entrasse.
    let r = ultimo.retorno_centesimos.unwrap();
    assert!(r > 0 && r < 200, "retorno {r}");
    assert_eq!(ultimo.aportado_centavos, 1_500_000);
    assert_eq!(set.aportado_centavos, 1_000_000);
}

#[test]
fn filtro_de_classe_ignora_o_cdb() {
    let (_d, db) = banco();
    criar(&db, cdb("2026-09-30", 1_000_000));
    criar(&db, itub4("2026-10-01"));
    let h = historico(&db, hoje(), 6, Some(Classe::Acoes)).unwrap();
    assert!(h.pontos[..6].iter().all(|p| p.patrimonio_centavos == 0));
    assert_eq!(h.pontos[6].patrimonio_centavos, 150 * 3978);
    assert_eq!(h.por_classe.len(), 1);
    assert_eq!(h.por_classe[0].0, Classe::Acoes);
}

#[test]
fn por_classe_na_ordem_do_catalogo_e_soma_o_patrimonio() {
    let (_d, db) = banco();
    criar(&db, itub4("2026-10-01"));
    criar(&db, cdb("2026-09-30", 1_000_000));
    let h = historico(&db, hoje(), 6, None).unwrap();
    let classes: Vec<Classe> = h.por_classe.iter().map(|(c, _)| *c).collect();
    assert_eq!(classes, vec![Classe::Rf, Classe::Acoes]);
    for (i, p) in h.pontos.iter().enumerate() {
        let soma: i64 = h.por_classe.iter().map(|(_, v)| v[i]).sum();
        assert_eq!(soma, p.patrimonio_centavos);
    }
}

fn centesimos(fator: Decimal) -> i64 {
    use rust_decimal::prelude::ToPrimitive;
    ((fator - Decimal::ONE) * Decimal::from(10_000) + Decimal::new(5, 1))
        .floor()
        .to_i64()
        .unwrap()
}

fn cotacao(db: &Db, ticker: &str, d: NaiveDate, centavos: i64) {
    db.gravar_cotacoes(&[Cotacao {
        ticker: ticker.into(),
        data: d,
        fechamento_centavos: centavos,
    }])
    .unwrap();
}

#[test]
fn cdi_acumulado_e_o_produto_dos_fatores_da_fixture() {
    let (_d, db) = banco();
    let mut fator = Decimal::ONE;
    for p in ler_sgs(&fixture("sgs-12.json")).unwrap() {
        if p.data > data(2026, 9, 30) && p.data <= hoje() {
            fator *= Decimal::ONE + p.valor / Decimal::from(100);
        }
    }
    let esperado = centesimos(fator);
    assert!(esperado > 0);
    let r = acumulado_benchmark(&db, Referencia::Cdi, &[data(2026, 9, 30), hoje()]).unwrap();
    assert_eq!(r, Some(vec![Some(0), Some(esperado)]));
}

#[test]
fn cdi_sem_dado_no_primeiro_ponto_fica_ausente() {
    let (_d, db) = banco();
    let datas = datas_painel(hoje(), 12).unwrap();
    assert_eq!(
        acumulado_benchmark(&db, Referencia::Cdi, &datas).unwrap(),
        None
    );
    assert_eq!(
        acumulado_benchmark(&db, Referencia::Selic, &datas).unwrap(),
        None
    );
}

#[test]
fn ipca_entra_so_com_os_meses_fechados_depois_do_primeiro_ponto() {
    let (_d, db) = banco();
    let datas = datas_painel(hoje(), 12).unwrap();
    let ipca: Vec<(String, Decimal)> = ler_sgs(&fixture("sgs-433.json"))
        .unwrap()
        .into_iter()
        .map(|p| (p.data.format("%Y-%m").to_string(), p.valor))
        .collect();
    let ate = |mes: &str| {
        let f = ipca
            .iter()
            .filter(|(m, _)| m.as_str() > "2025-10" && m.as_str() <= mes)
            .fold(Decimal::ONE, |a, (_, v)| {
                a * (Decimal::ONE + v / Decimal::from(100))
            });
        centesimos(f)
    };
    let r = acumulado_benchmark(&db, Referencia::Ipca, &datas)
        .unwrap()
        .unwrap();
    assert_eq!(r.len(), 13);
    assert_eq!(r[0], Some(0));
    assert_eq!(r[1], Some(ate("2025-11")));
    assert_eq!(r[11], Some(ate("2026-09")));
    // 28/10 ainda não fechou outubro: igual a 30/09.
    assert_eq!(r[12], Some(ate("2026-09")));
    assert!(ate("2026-09") > ate("2025-11"));
}

#[test]
fn bova11_sem_cotacao_no_primeiro_ponto_fica_ausente() {
    let (_d, db) = banco();
    let datas = datas_painel(hoje(), 12).unwrap();
    assert_eq!(
        acumulado_benchmark(&db, Referencia::Bova11, &datas).unwrap(),
        None
    );
}

#[test]
fn bova11_com_cotacao_sintetica_no_primeiro_ponto_da_a_razao() {
    let (_d, db) = banco();
    let fim = db
        .cotacao_em("BOVA11", hoje())
        .unwrap()
        .unwrap()
        .fechamento_centavos;
    let inicio = fim * 100 / 110;
    cotacao(&db, "BOVA11", data(2026, 9, 30), inicio);
    let datas = [data(2026, 9, 30), data(2026, 10, 15), hoje()];
    let r = acumulado_benchmark(&db, Referencia::Bova11, &datas)
        .unwrap()
        .unwrap();
    let esperado = centesimos(Decimal::from(fim) / Decimal::from(inicio));
    assert_eq!(r[0], Some(0));
    // 15/10 só enxerga a cotação sintética de 30/09: razão 1, retorno 0.
    assert_eq!(r[1], Some(0));
    assert_eq!(r[2], Some(esperado));
    assert!((990..=1010).contains(&esperado), "{esperado}");
}

#[test]
fn benchmark_padrao_por_classe() {
    assert_eq!(benchmark_padrao(Classe::Acoes), Referencia::Bova11);
    assert_eq!(benchmark_padrao(Classe::Rf), Referencia::Cdi);
}

#[test]
fn rendeu_desconta_o_aporte_no_meio_do_periodo() {
    let (_d, db) = banco();
    criar(&db, itub4("2026-02-01")); // 150 × 35,00
    criar(
        &db,
        NovoInvestimento {
            quantidade: Some(100),
            ..itub4("2026-08-10") // aporte no meio: 100 × 35,00
        },
    );
    let p = painel(&db, hoje(), 6, None).unwrap();
    let (v0, vn) = (
        p.historico.pontos[0].patrimonio_centavos,
        p.historico.pontos[6].patrimonio_centavos,
    );
    assert_eq!(v0, 150 * 3500);
    assert_eq!(vn, 250 * 3978);
    // V_último − V₀ − aporte do período.
    assert_eq!(p.numeros.rendeu_centavos, vn - v0 - 100 * 3500);
    assert_eq!(p.numeros.patrimonio_centavos, vn);
    assert_eq!(p.numeros.aportado_centavos, 250 * 3500);
    assert_eq!(
        p.numeros.periodo_centesimos,
        p.historico.pontos[6].acumulado_centesimos
    );
    assert_eq!(
        p.numeros.mes_centesimos,
        p.historico.pontos[6].retorno_centesimos
    );
    assert!(p.numeros.mes_cdi_centesimos.unwrap() > 0);
    // O CDI só existe na fixture a partir de setembro: sem dado em 30/04, o período some.
    assert_eq!(p.numeros.periodo_cdi_centesimos, None);
}

#[test]
fn numeros_do_cdi_quando_ha_dado_no_primeiro_ponto() {
    let (_d, db) = banco();
    criar(&db, cdb("2026-08-10", 1_000_000));
    // 6 meses a partir de 28/10 começam em 30/04: sem CDI; o mês (30/09 a 28/10) existe.
    let p = painel(&db, hoje(), 6, None).unwrap();
    let cdi_mes = acumulado_benchmark(&db, Referencia::Cdi, &[data(2026, 9, 30), hoje()])
        .unwrap()
        .unwrap()[1];
    assert_eq!(p.numeros.mes_cdi_centesimos, cdi_mes);
}

fn em(nome: &str, mut n: NovoInvestimento) -> NovoInvestimento {
    n.instituicao = nome.into();
    n
}

#[test]
fn instituicoes_somam_cem_e_vem_ordenadas() {
    let (_d, db) = banco();
    criar(&db, em("Rico", itub4("2026-10-01")));
    criar(
        &db,
        em(
            "Rico",
            NovoInvestimento {
                tipo: "rv-fii".into(),
                ticker: s("HGLG11"),
                data_aplicacao: s("2026-10-01"),
                quantidade: Some(10),
                preco_medio_centavos: Some(16_000),
                ..Default::default()
            },
        ),
    );
    criar(&db, cdb("2026-09-30", 100_000));
    let p = painel(&db, hoje(), 6, None).unwrap();
    assert_eq!(p.instituicoes.len(), 2);
    assert_eq!(p.instituicoes[0].nome, "Rico");
    assert_eq!(p.instituicoes[1].nome, "Banco Inter");
    assert!(p.instituicoes[0].atual_centavos > p.instituicoes[1].atual_centavos);
    let soma: i64 = p.instituicoes.iter().map(|i| i.percentual_decimos).sum();
    assert_eq!(soma, 1000);
}

#[test]
fn instituicoes_com_classe_so_olham_aquela_classe() {
    let (_d, db) = banco();
    criar(&db, em("Rico", itub4("2026-10-01")));
    criar(&db, cdb("2026-09-30", 100_000));
    let p = painel(&db, hoje(), 6, Some(Classe::Acoes)).unwrap();
    assert_eq!(p.instituicoes.len(), 1);
    assert_eq!(p.instituicoes[0].nome, "Rico");
    assert_eq!(p.instituicoes[0].percentual_decimos, 1000);
}

fn cdb_vence(vencimento: &str, valor: i64) -> NovoInvestimento {
    NovoInvestimento {
        vencimento: s(vencimento),
        ..cdb("2026-01-10", valor)
    }
}

#[test]
fn vencimentos_sem_o_vencido_em_ordem_e_no_maximo_cinco() {
    let (_d, db) = banco();
    criar(&db, cdb_vence("2026-10-01", 100_000)); // já venceu
    for (v, valor) in [
        ("2027-03-01", 100_000),
        ("2026-12-15", 110_000),
        ("2026-10-28", 120_000), // vence hoje: entra
        ("2028-01-01", 130_000),
        ("2027-09-09", 140_000),
        ("2026-11-20", 150_000),
        ("2029-05-05", 160_000),
    ] {
        criar(&db, cdb_vence(v, valor));
    }
    let p = painel(&db, hoje(), 6, None).unwrap();
    let datas: Vec<NaiveDate> = p.vencimentos.iter().map(|v| v.vencimento).collect();
    assert_eq!(
        datas,
        vec![
            data(2026, 10, 28),
            data(2026, 11, 20),
            data(2026, 12, 15),
            data(2027, 3, 1),
            data(2027, 9, 9)
        ]
    );
    assert!(p.vencimentos.iter().all(|v| v.atual_centavos > 0));
}

#[test]
fn variacoes_pedem_cotacao_hoje_e_no_ultimo_fim_de_mes() {
    let (_d, db) = banco();
    criar(&db, itub4("2026-08-01")); // só tem cotação em 28/10
    criar(
        &db,
        NovoInvestimento {
            tipo: "rv-fii".into(),
            instituicao: "Rico".into(),
            ticker: s("HGLG11"),
            data_aplicacao: s("2026-08-01"),
            quantidade: Some(10),
            preco_medio_centavos: Some(16_000),
            ..Default::default()
        },
    );
    criar(&db, cdb("2026-08-01", 100_000));
    let hglg = db
        .cotacao_em("HGLG11", hoje())
        .unwrap()
        .unwrap()
        .fechamento_centavos;
    cotacao(&db, "HGLG11", data(2026, 9, 30), hglg * 100 / 110);
    let p = painel(&db, hoje(), 6, None).unwrap();
    assert_eq!(p.variacoes.len(), 1);
    assert_eq!(p.variacoes[0].ticker, "HGLG11");
    assert!((99..=101).contains(&p.variacoes[0].variacao_decimos));
}

#[test]
fn variacoes_ordenadas_do_maior_para_o_menor() {
    let (_d, db) = banco();
    criar(&db, itub4("2026-08-01"));
    criar(
        &db,
        NovoInvestimento {
            tipo: "rv-fii".into(),
            instituicao: "Rico".into(),
            ticker: s("HGLG11"),
            data_aplicacao: s("2026-08-01"),
            quantidade: Some(10),
            preco_medio_centavos: Some(16_000),
            ..Default::default()
        },
    );
    let itub = db
        .cotacao_em("ITUB4", hoje())
        .unwrap()
        .unwrap()
        .fechamento_centavos;
    let hglg = db
        .cotacao_em("HGLG11", hoje())
        .unwrap()
        .unwrap()
        .fechamento_centavos;
    cotacao(&db, "ITUB4", data(2026, 9, 30), itub * 2); // caiu 50%
    cotacao(&db, "HGLG11", data(2026, 9, 30), hglg); // igual, mas em outra data
    let p = painel(&db, hoje(), 6, None).unwrap();
    let tickers: Vec<&str> = p.variacoes.iter().map(|v| v.ticker.as_str()).collect();
    assert_eq!(tickers, ["HGLG11", "ITUB4"]);
    assert_eq!(p.variacoes[0].variacao_decimos, 0);
    assert_eq!(p.variacoes[1].variacao_decimos, -500);
}

#[test]
fn painel_escolhe_os_benchmarks_pelo_filtro() {
    let (_d, db) = banco();
    criar(&db, cdb("2026-09-30", 1_000_000));
    let geral = painel(&db, hoje(), 6, None).unwrap();
    let cods: Vec<&str> = geral.benchmarks.iter().map(|(r, _)| r.codigo()).collect();
    assert_eq!(cods, ["cdi", "ipca", "bova11"]);
    assert_eq!(geral.datas.len(), 7);
    let rf = painel(&db, hoje(), 6, Some(Classe::Rf)).unwrap();
    let cods: Vec<&str> = rf.benchmarks.iter().map(|(r, _)| r.codigo()).collect();
    assert_eq!(cods, ["cdi", "selic", "ipca", "bova11", "xfix11"]);
}

#[test]
fn painel_invalido_e_vazio() {
    let (_d, db) = banco();
    assert_eq!(
        painel(&db, hoje(), 9, None).unwrap_err().code(),
        "PERIODO_INVALIDO"
    );
    let p = painel(&db, hoje(), 12, None).unwrap();
    assert_eq!(p.numeros.patrimonio_centavos, 0);
    assert_eq!(p.numeros.rendeu_centavos, 0);
    assert!(p.instituicoes.is_empty() && p.vencimentos.is_empty() && p.variacoes.is_empty());
}

#[test]
fn ipca_com_o_ultimo_mes_fechado_ainda_nao_publicado_mantem_o_ponto() {
    let (_d, db) = banco();
    // A fixture termina em set/2026: em 05/11 o IPCA de outubro ainda não saiu.
    let datas = [data(2025, 10, 31), data(2026, 9, 30), data(2026, 11, 5)];
    let r = acumulado_benchmark(&db, Referencia::Ipca, &datas)
        .unwrap()
        .unwrap();
    assert_eq!(r[0], Some(0));
    assert!(r[1].is_some_and(|v| v > 0));
    assert_eq!(r[2], r[1]);
}

// ---- Revisão final: Dietz por dias, mudança de origem e período comparável ----

/// Banco só com um CDI sintético (0,055131% a.d. nos dias úteis desde 2024), para comparar a
/// carteira com o CDI num período inteiro; a fixture do CDI só começa em setembro de 2026.
fn banco_cdi_sintetico() -> (tempfile::TempDir, Db) {
    use chrono::{Datelike, Weekday};
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let mut d = data(2024, 1, 1);
    let mut cdi = Vec::new();
    while d <= hoje() {
        if d.weekday() != Weekday::Sat && d.weekday() != Weekday::Sun {
            cdi.push(PontoDiario {
                data: d,
                valor: Decimal::new(55_131, 6),
            });
        }
        d = d.succ_opt().unwrap();
    }
    db.gravar_indice("cdi", &cdi).unwrap();
    (dir, db)
}

fn perto_de(a: i64, b: i64, tolerancia: i64, o_que: &str) {
    assert!((a - b).abs() <= tolerancia, "{o_que}: {a} contra {b}");
}

#[test]
fn aporte_grande_no_comeco_do_mes_acompanha_o_cdi() {
    let (_d, db) = banco_cdi_sintetico();
    criar(&db, cdb("2025-10-31", 100_000));
    criar(&db, cdb("2026-10-01", 1_000_000));
    let p = painel(&db, hoje(), 12, None).unwrap();
    let n = &p.numeros;
    perto_de(
        n.mes_centesimos.unwrap(),
        n.mes_cdi_centesimos.unwrap(),
        2,
        "no mês",
    );
    perto_de(
        n.periodo_centesimos,
        n.periodo_cdi_centesimos.unwrap(),
        2,
        "no período",
    );
}

#[test]
fn carteira_nova_tem_retorno_no_primeiro_mes() {
    let (_d, db) = banco_cdi_sintetico();
    criar(&db, cdb("2026-10-01", 1_000_000));
    let p = painel(&db, hoje(), 6, None).unwrap();
    let pts = &p.historico.pontos;
    assert!(pts[..6].iter().all(|x| x.retorno_centesimos.is_none()));
    let r = pts[6]
        .retorno_centesimos
        .expect("o primeiro mês tem retorno");
    perto_de(r, p.numeros.mes_cdi_centesimos.unwrap(), 2, "no mês");
    assert_eq!(pts[6].acumulado_centesimos, r);
}

#[test]
fn acao_a_custo_que_ganha_cotacao_hoje_nao_salta_no_mes() {
    let (_d, db) = banco();
    criar(&db, itub4("2026-02-01")); // sem cotação até 28/10: a custo nos fins de mês
    let p = painel(&db, hoje(), 6, None).unwrap();
    // O patrimônio do ponto continua real; só o retorno ignora a troca de origem.
    assert_eq!(p.historico.pontos[5].patrimonio_centavos, 150 * 3500);
    assert_eq!(p.historico.pontos[6].patrimonio_centavos, 150 * 3978);
    assert_eq!(p.numeros.mes_centesimos, Some(0));
    assert_eq!(p.numeros.periodo_centesimos, 0);
}

#[test]
fn pontos_passados_usam_cotacao_e_pu_guardados_ate_a_data() {
    let (_d, db) = banco();
    criar(&db, itub4("2026-02-01"));
    cotacao(&db, "ITUB4", data(2026, 8, 31), 3600);
    cotacao(&db, "ITUB4", data(2026, 9, 15), 3700);
    criar(
        &db,
        NovoInvestimento {
            tipo: "td-ipca".into(),
            instituicao: "Rico".into(),
            taxa: s("7.5"),
            data_aplicacao: s("2025-03-10"),
            vencimento: s("2035-05-15"),
            valor_aplicado_centavos: Some(1_000_000),
            ir: s("regressiva"),
            ..Default::default()
        },
    );
    let td = |d: NaiveDate| {
        db.preco_tesouro_em("Tesouro IPCA+", data(2035, 5, 15), d)
            .unwrap()
            .unwrap()
    };
    let compra = td(data(2025, 3, 10)).pu_compra;
    let valor_td = |d: NaiveDate| {
        use rust_decimal::prelude::ToPrimitive;
        (Decimal::from(1_000_000) / compra * td(d).pu_base)
            .floor()
            .to_i64()
            .unwrap()
    };
    let h = historico(&db, hoje(), 6, None).unwrap();
    let em = |d: NaiveDate| h.pontos.iter().find(|p| p.data == d).unwrap();
    // 31/07: ITUB4 ainda a custo; o Tesouro usa o último PU guardado (março de 2025).
    assert_eq!(
        em(data(2026, 7, 31)).patrimonio_centavos,
        150 * 3500 + valor_td(data(2026, 7, 31))
    );
    assert_eq!(
        em(data(2026, 8, 31)).patrimonio_centavos,
        150 * 3600 + valor_td(data(2026, 8, 31))
    );
    // 30/09 enxerga a cotação de 15/09, não a de 28/10.
    assert_eq!(
        em(data(2026, 9, 30)).patrimonio_centavos,
        150 * 3700 + valor_td(data(2026, 9, 30))
    );
    assert_eq!(
        em(hoje()).patrimonio_centavos,
        150 * 3978 + valor_td(hoje())
    );
    assert_ne!(valor_td(data(2026, 9, 30)), valor_td(hoje()));
}

#[test]
fn carteira_iniciada_em_abril_compara_o_periodo_desde_o_comeco() {
    let (_d, db) = banco_cdi_sintetico();
    criar(&db, cdb("2026-04-15", 1_000_000));
    criar(&db, cdb("2026-06-10", 500_000));
    criar(&db, cdb("2026-09-01", 300_000));
    let p = painel(&db, hoje(), 12, None).unwrap();
    perto_de(
        p.numeros.periodo_centesimos,
        p.numeros.periodo_cdi_centesimos.unwrap(),
        2,
        "no período",
    );
    // O primeiro intervalo com retorno é (31/03, 30/04]: tudo parte de 31/03.
    assert_eq!(p.datas[5], data(2026, 3, 31));
    assert_eq!(p.historico.inicio, Some(5));
    assert_eq!(p.numeros.desde, Some(data(2026, 4, 15)));
    let cdi = p.benchmarks[0].1.as_ref().unwrap();
    assert!(cdi[..5].iter().all(Option::is_none));
    assert_eq!(cdi[5], Some(0));
    assert_eq!(cdi[12], p.numeros.periodo_cdi_centesimos);
    assert!(
        p.historico.pontos[..=5]
            .iter()
            .all(|x| x.acumulado_centesimos == 0)
    );
}

#[test]
fn benchmarks_partem_do_comeco_do_historico() {
    let (_d, db) = banco();
    // Com o CDI da fixture (só de setembro em diante), 12 meses antes não teriam CDI.
    criar(&db, cdb("2026-09-30", 1_000_000));
    let p = painel(&db, hoje(), 12, None).unwrap();
    assert_eq!(p.historico.inicio, Some(11));
    assert_eq!(p.numeros.desde, Some(data(2026, 9, 30)));
    let cdi = p.benchmarks[0].1.as_ref().expect("CDI desde 30/09");
    assert_eq!(cdi.len(), 13);
    assert!(cdi[..11].iter().all(Option::is_none));
    assert_eq!(cdi[11], Some(0));
    let mes = acumulado_benchmark(&db, Referencia::Cdi, &[data(2026, 9, 30), hoje()])
        .unwrap()
        .unwrap()[1];
    assert_eq!(cdi[12], mes);
    assert_eq!(p.numeros.periodo_cdi_centesimos, mes);
    // BOVA11 não tem cotação em 30/09: ausente inteiro.
    assert_eq!(p.benchmarks[2].0, Referencia::Bova11);
    assert_eq!(p.benchmarks[2].1, None);
}

#[test]
fn sem_historico_nao_ha_inicio_nem_desde_anterior_ao_periodo() {
    let (_d, db) = banco();
    let vazio = painel(&db, hoje(), 6, None).unwrap();
    assert_eq!(vazio.historico.inicio, None);
    assert_eq!(vazio.numeros.desde, None);
    // Carteira anterior ao período: compara os 6 meses inteiros, sem "desde".
    criar(&db, itub4("2026-02-01"));
    let p = painel(&db, hoje(), 6, None).unwrap();
    assert_eq!(p.historico.inicio, Some(0));
    assert_eq!(p.numeros.desde, None);
}

/// Meta: 300 CDBs de uns 3,5 anos em menos de 250 ms em release. Rode com
/// `cargo test --release -p guru-core --test painel desempenho -- --ignored --nocapture`.
#[test]
#[ignore = "desempenho: rode à mão em release"]
fn desempenho_de_300_cdbs_em_doze_meses() {
    use chrono::{Datelike, Weekday};
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let mut d = data(2019, 1, 1);
    let mut cdi = Vec::new();
    while d <= hoje() {
        if d.weekday() != Weekday::Sat && d.weekday() != Weekday::Sun {
            cdi.push(PontoDiario {
                data: d,
                valor: Decimal::new(55_131, 6),
            });
        }
        d = d.succ_opt().unwrap();
    }
    db.gravar_indice("cdi", &cdi).unwrap();
    db.gravar_indice("selic", &cdi).unwrap();
    for i in 0..300u64 {
        let aplicacao = data(2019, 2, 1) + chrono::Days::new((i * 7) % 2700);
        let mut n = cdb(&aplicacao.to_string(), 100_000);
        n.instituicao = format!("Banco {}", i % 7);
        n.taxa = s("102");
        criar(&db, n);
    }
    let t = std::time::Instant::now();
    let p = painel(&db, hoje(), 12, None).unwrap();
    let tempo = t.elapsed();
    eprintln!("painel(12) com 300 CDBs: {tempo:?}");
    assert_eq!(p.historico.pontos.len(), 13);
    assert!(tempo.as_millis() < 250, "{tempo:?}");
}

// ---- Fundo pela cota da CVM: a troca de base (aplicado → saldo) não é rentabilidade ----

/// Fundo fictício aplicado em 31/03/2026 com R$ 10.000, CNPJ e saldo do extrato em `saldo_em`
/// (R$ 11.000). A cota fica parada até julho e sobe 1% em agosto.
fn fundo_com_saldo(saldo_em: &str) -> (tempfile::TempDir, Db) {
    use guru_core::CotaFundo;
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let cotas: Vec<CotaFundo> = [
        ((2026, 3, 31), "1.00"),
        ((2026, 4, 30), "1.00"),
        ((2026, 5, 29), "1.00"),
        ((2026, 6, 30), "1.00"),
        ((2026, 7, 15), "1.00"),
        ((2026, 7, 31), "1.00"),
        ((2026, 8, 31), "1.01"),
        ((2026, 9, 30), "1.01"),
        ((2026, 10, 27), "1.01"),
    ]
    .iter()
    .map(|&((a, m, d), c)| CotaFundo {
        cnpj: "11222333000181".into(),
        data: data(a, m, d),
        cota: c.parse().unwrap(),
    })
    .collect();
    db.gravar_cotas_fundo(&cotas).unwrap();
    criar(
        &db,
        NovoInvestimento {
            tipo: "fundo".into(),
            instituicao: "Corretora Exemplo".into(),
            emissor: s("Fundo Exemplo"),
            data_aplicacao: s("2026-03-31"),
            valor_aplicado_centavos: Some(1_000_000),
            liquidez: s("diaria"),
            ..Default::default()
        },
    );
    let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
    conn.execute(
        "UPDATE investimentos SET codigo_b3 = '11222333000181', saldo_centavos = 1100000, \
         saldo_data = ?1 WHERE tipo = 'fundo'",
        [saldo_em],
    )
    .unwrap();
    (dir, db)
}

/// Retornos dos intervalos que terminam em 31/05, 30/06, 31/07, 31/08, 30/09 e 28/10.
fn retornos_do_fundo(db: &Db) -> Vec<Option<i64>> {
    let h = historico(db, hoje(), 6, None).unwrap();
    h.pontos[1..].iter().map(|p| p.retorno_centesimos).collect()
}

#[test]
fn fundo_com_saldo_no_meio_do_mes_nao_conta_a_troca_de_base() {
    let (_d, db) = fundo_com_saldo("2026-07-15");
    // Junho a custo projetado (10.000), julho pelo saldo (11.000): o mês da troca fica neutro;
    // agosto rende o 1% da cota.
    assert_eq!(
        retornos_do_fundo(&db),
        [Some(0), Some(0), Some(0), Some(100), Some(0), Some(0)]
    );
}

#[test]
fn fundo_com_saldo_no_fim_do_mes_conta_o_mes_seguinte() {
    let (_d, db) = fundo_com_saldo("2026-07-31");
    // 31/07 vale o saldo do extrato; 31/08 é o mesmo saldo projetado pela cota: agosto conta.
    assert_eq!(
        retornos_do_fundo(&db),
        [Some(0), Some(0), Some(0), Some(100), Some(0), Some(0)]
    );
}
