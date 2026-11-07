use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::Duration;

use chrono::NaiveDate;
use guru_core::{
    Baixador, BaixadorHttp, BaixadorPasta, Db, ErroFonte, Gravacao, NovoInvestimento, Origem,
    StatusFonte, atualizar, titulo_tesouro, valorizar,
};

fn data(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn pasta() -> PathBuf {
    PathBuf::from(format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR")))
}

fn abrir() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    (dir, db)
}

fn criar(db: &Db, n: NovoInvestimento) {
    match db.criar_investimento(&n, data(2026, 10, 28)).unwrap() {
        Gravacao::Ok(_) => {}
        Gravacao::Invalido(e) => panic!("inválido: {e:?}"),
    }
}

fn cdb(aplicacao: &str) -> NovoInvestimento {
    NovoInvestimento {
        tipo: "rf-cdb".into(),
        instituicao: "Banco Inter".into(),
        emissor: s("Banco Inter"),
        indexador: s("pos"),
        taxa: s("102"),
        benchmark: s("cdi"),
        data_aplicacao: s(aplicacao),
        vencimento: s("2028-03-15"),
        valor_aplicado_centavos: Some(2_000_000),
        liquidez: s("vencimento"),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn tesouro_selic() -> NovoInvestimento {
    NovoInvestimento {
        tipo: "td-selic".into(),
        instituicao: "Rico".into(),
        taxa: s("0.09"),
        data_aplicacao: s("2025-03-12"),
        vencimento: s("2029-03-01"),
        valor_aplicado_centavos: Some(1_800_000),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn itub4() -> NovoInvestimento {
    NovoInvestimento {
        tipo: "rv-acao".into(),
        instituicao: "Rico".into(),
        ticker: s("ITUB4"),
        data_aplicacao: s("2025-04-01"),
        quantidade: Some(150),
        preco_medio_centavos: Some(3142),
        ..Default::default()
    }
}

fn carteira_completa(db: &Db) {
    criar(db, cdb("2025-03-15"));
    criar(db, tesouro_selic());
    criar(db, itub4());
}

/// Serve as fixtures, conta e registra as chamadas e pode simular falhas.
struct Falso {
    pasta: BaixadorPasta,
    chamadas: RefCell<Vec<String>>,
    /// Trechos de URL que respondem o HTML de erro do SGS (status 200).
    html_erro: Vec<&'static str>,
    /// Trechos de URL que falham na rede.
    sem_rede: Vec<&'static str>,
    /// Se verdadeiro, a B3 nunca tem arquivo.
    sem_b3: bool,
    /// Trechos de URL que respondem 404 (o SGS faz isso quando o intervalo não tem dados).
    status_404: Vec<&'static str>,
}

impl Falso {
    fn novo() -> Self {
        Falso {
            pasta: BaixadorPasta { pasta: pasta() },
            chamadas: RefCell::new(Vec::new()),
            html_erro: Vec::new(),
            sem_rede: Vec::new(),
            sem_b3: false,
            status_404: Vec::new(),
        }
    }

    fn com(&self, trecho: &str) -> Vec<String> {
        self.chamadas
            .borrow()
            .iter()
            .filter(|u| u.contains(trecho))
            .cloned()
            .collect()
    }
}

impl Baixador for Falso {
    fn baixar(&self, url: &str) -> Result<Vec<u8>, ErroFonte> {
        self.chamadas.borrow_mut().push(url.to_string());
        if self.sem_rede.iter().any(|t| url.contains(t)) {
            return Err(ErroFonte::Rede("sem conexão".into()));
        }
        if self.html_erro.iter().any(|t| url.contains(t)) {
            return Ok(std::fs::read(pasta().join("sgs-erro.html")).unwrap());
        }
        if self.status_404.iter().any(|t| url.contains(t)) {
            return Err(ErroFonte::Status(404));
        }
        if self.sem_b3 && url.contains("COTAHIST") {
            return Err(ErroFonte::Status(404));
        }
        self.pasta.baixar(url)
    }
}

fn status(db: &Db, fonte: &str) -> StatusFonte {
    db.status_fontes()
        .unwrap()
        .into_iter()
        .find(|f| f.fonte == fonte)
        .unwrap()
}

struct Esperas(Cell<usize>);

impl Esperas {
    fn novo() -> Self {
        Esperas(Cell::new(0))
    }
    fn fecha(&self) -> impl Fn(Duration) + '_ {
        |d| {
            assert_eq!(d, Duration::from_secs(5));
            self.0.set(self.0.get() + 1);
        }
    }
}

const HOJE: (i32, u32, u32) = (2026, 10, 28);

fn hoje() -> NaiveDate {
    data(HOJE.0, HOJE.1, HOJE.2)
}

#[test]
fn primeira_atualizacao_grava_tudo_sem_erro() {
    let (_d, db) = abrir();
    carteira_completa(&db);
    let falso = Falso::novo();
    let esperas = Esperas::novo();
    let r = atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();

    assert!(r.novos > 0);
    assert_eq!(r.fontes.len(), 6);
    // A `cvm` só passa a ser atualizada na Task 3.
    for f in r.fontes.iter().filter(|f| f.fonte != "cvm") {
        assert_eq!(f.erro, None, "{}", f.fonte);
        assert!(f.atualizado_em.is_some(), "{}", f.fonte);
    }
    assert_eq!(
        status(&db, "cdi").ultima_data.as_deref(),
        Some("2026-10-28")
    );
    assert_eq!(
        status(&db, "selic").ultima_data.as_deref(),
        Some("2026-10-28")
    );
    assert_eq!(
        status(&db, "tesouro").ultima_data.as_deref(),
        Some("2026-10-28")
    );
    assert_eq!(status(&db, "b3").ultima_data.as_deref(), Some("2026-10-28"));
    assert!(status(&db, "ipca").ultima_data.is_some());
    assert_eq!(esperas.0.get(), 0);

    // Tesouro: só o título da carteira; B3: só os tickers da carteira (+ referências).
    assert!(
        db.preco_tesouro_em("Tesouro Selic", data(2029, 3, 1), hoje())
            .unwrap()
            .is_some()
    );
    assert!(
        db.preco_tesouro_em("Tesouro Prefixado", data(2029, 1, 1), hoje())
            .unwrap()
            .is_none()
    );
    assert!(db.cotacao_em("ITUB4", hoje()).unwrap().is_some());
    assert!(db.cotacao_em("BOVA11", hoje()).unwrap().is_some());
    assert!(db.cotacao_em("XFIX11", hoje()).unwrap().is_some());
    assert!(db.cotacao_em("B3SA3", hoje()).unwrap().is_none());
}

#[test]
fn segunda_rodada_nao_baixa_de_novo_nem_duplica() {
    let (_d, db) = abrir();
    carteira_completa(&db);
    let esperas = Esperas::novo();
    let primeiro = Falso::novo();
    atualizar(&db, &primeiro, hoje(), &esperas.fecha()).unwrap();

    let segundo = Falso::novo();
    let r = atualizar(&db, &segundo, hoje(), &esperas.fecha()).unwrap();
    assert_eq!(r.novos, 0);
    assert!(segundo.com("bcdata.sgs.12").is_empty());
    assert!(segundo.com("bcdata.sgs.11").is_empty());
    assert!(segundo.com("PrecoTaxaTesouroDireto").is_empty());
    // Alvos de fim de mês esgotados e o pregão já guardado não são pedidos de novo.
    assert!(segundo.com("COTAHIST").is_empty());
    assert!(r.fontes.iter().all(|f| f.erro.is_none()));
}

#[test]
fn falha_do_cdi_fica_no_status_e_o_resto_segue() {
    let (_d, db) = abrir();
    carteira_completa(&db);
    let mut falso = Falso::novo();
    falso.html_erro = vec!["bcdata.sgs.12"];
    let esperas = Esperas::novo();
    let r = atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();

    let cdi = status(&db, "cdi");
    assert_eq!(
        cdi.erro.as_deref(),
        Some("O Banco Central não respondeu. Tente de novo mais tarde.")
    );
    assert_eq!(cdi.ultima_data, None);
    assert_eq!(cdi.atualizado_em, None);
    for f in ["selic", "ipca", "tesouro", "b3"] {
        assert_eq!(status(&db, f).erro, None, "{f}");
    }
    assert_eq!(
        status(&db, "selic").ultima_data.as_deref(),
        Some("2026-10-28")
    );
    assert_eq!(esperas.0.get(), 1);
    assert_eq!(falso.com("bcdata.sgs.12").len(), 2);
    assert_eq!(r.fontes.len(), 6);
}

#[test]
fn falha_nao_apaga_o_que_ja_se_sabe() {
    let (_d, db) = abrir();
    carteira_completa(&db);
    let esperas = Esperas::novo();
    atualizar(&db, &Falso::novo(), hoje(), &esperas.fecha()).unwrap();
    let antes = status(&db, "cdi");
    assert!(antes.atualizado_em.is_some());

    let mut falso = Falso::novo();
    falso.html_erro = vec!["bcdata.sgs.12"];
    atualizar(&db, &falso, data(2026, 10, 29), &esperas.fecha()).unwrap();
    let depois = status(&db, "cdi");
    assert!(depois.erro.is_some());
    assert_eq!(depois.ultima_data, antes.ultima_data);
    assert_eq!(depois.atualizado_em, antes.atualizado_em);

    // E a rodada seguinte, já sem a falha, limpa o erro.
    atualizar(&db, &Falso::novo(), data(2026, 10, 29), &esperas.fecha()).unwrap();
    assert_eq!(status(&db, "cdi").erro, None);
}

#[test]
fn rede_que_volta_na_segunda_tentativa_nao_vira_erro() {
    struct UmaFalha {
        interno: Falso,
        falhou: Cell<bool>,
    }
    impl Baixador for UmaFalha {
        fn baixar(&self, url: &str) -> Result<Vec<u8>, ErroFonte> {
            if url.contains("bcdata.sgs.11") && !self.falhou.replace(true) {
                return Err(ErroFonte::Rede("caiu".into()));
            }
            self.interno.baixar(url)
        }
    }
    let (_d, db) = abrir();
    carteira_completa(&db);
    let b = UmaFalha {
        interno: Falso::novo(),
        falhou: Cell::new(false),
    };
    let esperas = Esperas::novo();
    atualizar(&db, &b, hoje(), &esperas.fecha()).unwrap();
    assert_eq!(status(&db, "selic").erro, None);
    assert_eq!(esperas.0.get(), 1);
}

#[test]
fn aplicacao_de_2014_pede_o_cdi_em_janelas_de_ate_10_anos() {
    let (_d, db) = abrir();
    criar(&db, cdb("2014-03-05"));
    let falso = Falso::novo();
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();

    let urls = falso.com("bcdata.sgs.12");
    assert_eq!(urls.len(), 2);
    let janelas: Vec<(NaiveDate, NaiveDate)> = urls
        .iter()
        .map(|u| {
            let de = u.split("dataInicial=").nth(1).unwrap()[..10].to_string();
            let ate = u.split("dataFinal=").nth(1).unwrap()[..10].to_string();
            (
                NaiveDate::parse_from_str(&de, "%d/%m/%Y").unwrap(),
                NaiveDate::parse_from_str(&ate, "%d/%m/%Y").unwrap(),
            )
        })
        .collect();
    assert_eq!(janelas[0].0, data(2014, 3, 5));
    assert_eq!(janelas[1].1, hoje());
    assert_eq!(janelas[1].0, janelas[0].1.succ_opt().unwrap());
    for (de, ate) in &janelas {
        let limite = de.checked_add_months(chrono::Months::new(120)).unwrap();
        assert!(*ate < limite, "janela passa de 10 anos: {de} a {ate}");
    }
}

#[test]
fn sem_renda_variavel_a_b3_baixa_so_as_referencias() {
    let (_d, db) = abrir();
    criar(&db, cdb("2025-03-15"));
    let falso = Falso::novo();
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();
    assert!(falso.com("PrecoTaxaTesouroDireto").is_empty());
    assert_eq!(status(&db, "tesouro").atualizado_em, None);
    // BOVA11 e XFIX11 são os benchmarks do painel: vêm mesmo sem ação nem FII.
    assert!(!falso.com("COTAHIST").is_empty());
    assert_eq!(status(&db, "b3").erro, None);
    assert!(status(&db, "b3").atualizado_em.is_some());
    assert!(db.cotacao_em("BOVA11", hoje()).unwrap().is_some());
    assert!(db.cotacao_em("XFIX11", hoje()).unwrap().is_some());
    assert!(db.cotacao_em("ITUB4", hoje()).unwrap().is_none());
}

#[test]
fn sem_nenhum_investimento_nao_baixa_tesouro_nem_b3() {
    let (_d, db) = abrir();
    let falso = Falso::novo();
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();
    assert!(falso.com("PrecoTaxaTesouroDireto").is_empty());
    assert!(falso.com("COTAHIST").is_empty());
    assert_eq!(status(&db, "b3").atualizado_em, None);
}

#[test]
fn b3_sabado_volta_para_a_sexta_sem_espera() {
    let (_d, db) = abrir();
    criar(&db, itub4());
    let falso = Falso::novo();
    let esperas = Esperas::novo();
    let r = atualizar(&db, &falso, data(2026, 10, 31), &esperas.fecha()).unwrap();

    let b3 = falso.com("COTAHIST");
    assert!(b3.len() >= 4);
    let datas: Vec<&str> = b3[..4]
        .iter()
        .map(|u| u.rsplit("COTAHIST_D").next().unwrap())
        .collect();
    assert_eq!(
        datas,
        vec![
            "31102026.ZIP",
            "30102026.ZIP",
            "29102026.ZIP",
            "28102026.ZIP"
        ]
    );
    assert_eq!(status(&db, "b3").erro, None);
    assert_eq!(status(&db, "b3").ultima_data.as_deref(), Some("2026-10-28"));
    assert!(
        db.cotacao_em("ITUB4", data(2026, 10, 28))
            .unwrap()
            .is_some()
    );
    assert_eq!(esperas.0.get(), 0, "dia sem pregão não espera");
    assert!(r.novos > 0);
}

#[test]
fn b3_sem_nenhum_pregao_em_7_dias_registra_erro() {
    let (_d, db) = abrir();
    criar(&db, itub4());
    let mut falso = Falso::novo();
    falso.sem_b3 = true;
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();

    assert_eq!(
        status(&db, "b3").erro.as_deref(),
        Some("A B3 não publicou cotações nos últimos 7 dias.")
    );
    assert_eq!(esperas.0.get(), 0);
    // 7 tentativas para hoje; cada fim de mês também tenta no máximo 7.
    let hoje_tentativas = falso
        .com("COTAHIST")
        .iter()
        .take(7)
        .map(|u| u.rsplit("COTAHIST_D").next().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(hoje_tentativas[0], "28102026.ZIP");
    assert_eq!(hoje_tentativas[6], "22102026.ZIP");
}

#[test]
fn b3_com_rede_fora_tenta_de_novo_uma_vez_e_registra_erro() {
    let (_d, db) = abrir();
    criar(&db, itub4());
    let mut falso = Falso::novo();
    falso.sem_rede = vec!["COTAHIST"];
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();

    assert!(status(&db, "b3").erro.is_some());
    assert_eq!(esperas.0.get(), 1);
    assert_eq!(falso.com("COTAHIST").len(), 2);
}

#[test]
fn tesouro_atualizado_hoje_nao_baixa_de_novo() {
    let (_d, db) = abrir();
    criar(&db, tesouro_selic());
    let esperas = Esperas::novo();
    atualizar(&db, &Falso::novo(), hoje(), &esperas.fecha()).unwrap();
    let segundo = Falso::novo();
    atualizar(&db, &segundo, hoje(), &esperas.fecha()).unwrap();
    assert!(segundo.com("PrecoTaxaTesouroDireto").is_empty());
}

#[test]
fn csv_do_tesouro_invalido_vira_erro_da_fonte() {
    struct CsvRuim(Falso);
    impl Baixador for CsvRuim {
        fn baixar(&self, url: &str) -> Result<Vec<u8>, ErroFonte> {
            if url.contains("PrecoTaxaTesouroDireto") {
                return Ok(b"<html>fora do ar</html>".to_vec());
            }
            self.0.baixar(url)
        }
    }
    let (_d, db) = abrir();
    criar(&db, tesouro_selic());
    let esperas = Esperas::novo();
    atualizar(&db, &CsvRuim(Falso::novo()), hoje(), &esperas.fecha()).unwrap();
    assert_eq!(
        status(&db, "tesouro").erro.as_deref(),
        Some("O Tesouro Transparente não respondeu. Tente de novo mais tarde.")
    );
    assert_eq!(esperas.0.get(), 1);
    assert_eq!(status(&db, "cdi").erro, None);
}

#[test]
fn baixador_http_recusa_url_fora_da_lista() {
    let r = BaixadorHttp::novo().baixar("https://exemplo.com/x");
    assert_eq!(r, Err(ErroFonte::Rede("destino não permitido".into())));
}

#[test]
fn baixador_pasta_sem_arquivo_responde_404() {
    let b = BaixadorPasta { pasta: pasta() };
    let r = b.baixar("https://bvmf.bmfbovespa.com.br/InstDados/SerHist/COTAHIST_D01012020.ZIP");
    assert_eq!(r, Err(ErroFonte::Status(404)));
}

#[test]
fn titulo_do_tesouro_usa_o_rotulo_do_catalogo() {
    let (_d, db) = abrir();
    criar(&db, tesouro_selic());
    criar(&db, cdb("2025-03-15"));
    let lista = db.listar_investimentos(data(2026, 10, 28)).unwrap();
    let td = lista.iter().find(|i| i.tipo == "td-selic").unwrap();
    assert_eq!(
        titulo_tesouro(td),
        Some(("Tesouro Selic".to_string(), data(2029, 3, 1)))
    );
    let cdb = lista.iter().find(|i| i.tipo == "rf-cdb").unwrap();
    assert_eq!(titulo_tesouro(cdb), None);
}

fn tesouro_prefixado() -> NovoInvestimento {
    NovoInvestimento {
        tipo: "td-pre".into(),
        instituicao: "Rico".into(),
        taxa: s("14.5"),
        data_aplicacao: s("2025-03-14"),
        vencimento: s("2029-01-01"),
        valor_aplicado_centavos: Some(1_000_000),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn periodo(url: &str) -> (NaiveDate, NaiveDate) {
    let p = |chave: &str| {
        let t = &url.split(chave).nth(1).unwrap()[..10];
        NaiveDate::parse_from_str(t, "%d/%m/%Y").unwrap()
    };
    (p("dataInicial="), p("dataFinal="))
}

#[test]
fn aplicacao_antiga_cadastrada_depois_preenche_o_cdi_para_tras() {
    let (_d, db) = abrir();
    criar(&db, cdb("2026-01-05"));
    let esperas = Esperas::novo();
    atualizar(&db, &Falso::novo(), hoje(), &esperas.fecha()).unwrap();
    let primeira = db.primeira_data_indice("cdi").unwrap().unwrap();

    criar(&db, cdb("2014-03-05"));
    let falso = Falso::novo();
    atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();
    let janelas: Vec<_> = falso
        .com("bcdata.sgs.12")
        .iter()
        .map(|u| periodo(u))
        .collect();
    assert_eq!(janelas.len(), 2);
    assert_eq!(janelas[0].0, data(2014, 3, 5));
    assert_eq!(janelas[1].1, primeira.pred_opt().unwrap());
    assert_eq!(janelas[1].0, janelas[0].1.succ_opt().unwrap());
    for (de, ate) in &janelas {
        assert!(*ate < de.checked_add_months(chrono::Months::new(120)).unwrap());
    }
    // O IPCA também volta no tempo.
    let ipca = falso.com("bcdata.sgs.433");
    assert!(ipca.iter().any(|u| periodo(u).0 == data(2014, 3, 1)));

    // Nada a repetir na rodada seguinte.
    let terceiro = Falso::novo();
    atualizar(&db, &terceiro, hoje(), &esperas.fecha()).unwrap();
    assert!(terceiro.com("bcdata.sgs.12").is_empty());
    assert!(terceiro.com("bcdata.sgs.11").is_empty());
}

#[test]
fn titulo_novo_do_tesouro_baixa_o_csv_no_mesmo_dia() {
    let (_d, db) = abrir();
    criar(&db, tesouro_selic());
    let esperas = Esperas::novo();
    atualizar(&db, &Falso::novo(), hoje(), &esperas.fecha()).unwrap();
    assert!(
        db.preco_tesouro_em("Tesouro Prefixado", data(2029, 1, 1), hoje())
            .unwrap()
            .is_none()
    );

    criar(&db, tesouro_prefixado());
    let falso = Falso::novo();
    atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();
    assert_eq!(falso.com("PrecoTaxaTesouroDireto").len(), 1);
    assert!(
        db.preco_tesouro_em("Tesouro Prefixado", data(2029, 1, 1), hoje())
            .unwrap()
            .is_some()
    );
}

#[test]
fn tesouro_com_vencimento_inexistente_nao_baixa_o_csv_a_cada_rodada() {
    let (_d, db) = abrir();
    criar(
        &db,
        NovoInvestimento {
            vencimento: s("2031-07-15"),
            ..tesouro_selic()
        },
    );
    let esperas = Esperas::novo();

    let primeira = Falso::novo();
    atualizar(&db, &primeira, hoje(), &esperas.fecha()).unwrap();
    assert_eq!(primeira.com("PrecoTaxaTesouroDireto").len(), 1);

    let segunda = Falso::novo();
    atualizar(&db, &segunda, hoje(), &esperas.fecha()).unwrap();
    assert!(segunda.com("PrecoTaxaTesouroDireto").is_empty());
    let inv = db.listar_investimentos(hoje()).unwrap().remove(0);
    assert_eq!(
        valorizar(&db, &inv, hoje()).unwrap().origem,
        Origem::SemDados
    );

    let amanha = hoje().succ_opt().unwrap();
    let terceira = Falso::novo();
    atualizar(&db, &terceira, amanha, &esperas.fecha()).unwrap();
    assert_eq!(terceira.com("PrecoTaxaTesouroDireto").len(), 1);
}

#[test]
fn ticker_novo_ganha_cotacao_no_mesmo_dia() {
    let (_d, db) = abrir();
    criar(&db, itub4());
    let esperas = Esperas::novo();
    atualizar(&db, &Falso::novo(), hoje(), &esperas.fecha()).unwrap();
    assert!(db.cotacao_em("B3SA3", hoje()).unwrap().is_none());

    criar(
        &db,
        NovoInvestimento {
            ticker: s("B3SA3"),
            ..itub4()
        },
    );
    atualizar(&db, &Falso::novo(), hoje(), &esperas.fecha()).unwrap();
    let c = db.cotacao_em("B3SA3", hoje()).unwrap().unwrap();
    assert_eq!(c.data, hoje());
}

#[test]
fn rede_persistente_na_b3_para_tudo_apos_uma_nova_tentativa() {
    let (_d, db) = abrir();
    criar(&db, itub4());
    let mut falso = Falso::novo();
    falso.sem_rede = vec!["COTAHIST"];
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();
    assert_eq!(falso.com("COTAHIST").len(), 2);
    assert_eq!(esperas.0.get(), 1);
    assert_eq!(
        status(&db, "b3").erro.as_deref(),
        Some("A B3 não respondeu. Tente de novo mais tarde.")
    );
}

#[test]
fn tesouro_comprado_no_sabado_guarda_o_pu_da_sexta_e_nao_baixa_de_novo() {
    let (_d, db) = abrir();
    let mut n = tesouro_selic();
    n.data_aplicacao = s("2025-03-08");
    criar(&db, n);
    let esperas = Esperas::novo();
    atualizar(&db, &Falso::novo(), hoje(), &esperas.fecha()).unwrap();
    let p = db
        .preco_tesouro_em("Tesouro Selic", data(2029, 3, 1), data(2025, 3, 8))
        .unwrap()
        .unwrap();
    assert_eq!(p.data, data(2025, 3, 7));
    let inv = db.listar_investimentos(hoje()).unwrap();
    assert_eq!(inv[0].origem_valor, Origem::Tesouro);

    let segundo = Falso::novo();
    atualizar(&db, &segundo, hoje(), &esperas.fecha()).unwrap();
    assert!(segundo.com("PrecoTaxaTesouroDireto").is_empty());
}

#[test]
fn sgs_404_e_intervalo_sem_dados_sem_erro_e_sem_espera() {
    let (_d, db) = abrir();
    criar(&db, cdb("2025-03-15"));
    let mut falso = Falso::novo();
    falso.status_404 = vec!["bcdata.sgs.12"];
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje(), &esperas.fecha()).unwrap();
    assert_eq!(status(&db, "cdi").erro, None);
    assert_eq!(falso.com("bcdata.sgs.12").len(), 1);
    assert_eq!(esperas.0.get(), 0);
}
