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
    /// Pasta com os `inf_diario_fi_<AAAAMM>.zip` gerados no teste (a CVM sai daqui).
    cvm: Option<PathBuf>,
    /// Se verdadeiro, qualquer dia da B3 responde o COTAHIST de 28/10/2026.
    b3_qualquer_dia: bool,
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
            cvm: None,
            b3_qualquer_dia: false,
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
        if self.b3_qualquer_dia && url.contains("COTAHIST") {
            return Ok(std::fs::read(pasta().join("COTAHIST_D28102026.ZIP")).unwrap());
        }
        if let Some(cvm) = &self.cvm
            && url.contains("cvm.gov.br")
        {
            return BaixadorPasta { pasta: cvm.clone() }.baixar(url);
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
    for f in &r.fontes {
        if f.fonte == "cvm" {
            // A carteira não tem fundo com CNPJ: a CVM fica intocada, nunca atualizada.
            assert_eq!(f.erro, None);
            assert_eq!(f.ultima_data, None);
            assert_eq!(f.atualizado_em, None);
        } else {
            assert_eq!(f.erro, None, "{}", f.fonte);
            assert!(f.atualizado_em.is_some(), "{}", f.fonte);
        }
    }
    assert!(falso.com("cvm.gov.br").is_empty());
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

// ---- CVM (cotas de fundos) ----

/// CNPJs fictícios com dígitos válidos.
const CNPJ_A: &str = "11222333000181";
const CNPJ_B: &str = "22333444000181";
const MSG_CVM: &str = "A CVM não respondeu. Tente de novo mais tarde.";

fn hoje_cvm() -> NaiveDate {
    data(2026, 11, 7)
}

/// Linhas (cnpj, data, cota) de cada mês das fixtures. O CSV traz os dois CNPJs.
fn linhas_cvm(mes: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    match mes {
        "202609" => vec![
            (CNPJ_A, "2026-09-01", "1.100000000000"),
            (CNPJ_A, "2026-09-08", "1.105000000000"),
            (CNPJ_A, "2026-09-30", "1.120000000000"),
            (CNPJ_B, "2026-09-08", "2.200000000000"),
            (CNPJ_B, "2026-09-30", "2.210000000000"),
        ],
        "202610" => vec![
            (CNPJ_A, "2026-10-02", "1.121000000000"),
            (CNPJ_A, "2026-10-30", "1.130000000000"),
            (CNPJ_B, "2026-10-02", "2.211000000000"),
            (CNPJ_B, "2026-10-30", "2.220000000000"),
        ],
        "202302" => vec![
            (CNPJ_A, "2023-02-10", "1.000000000000"),
            (CNPJ_A, "2023-02-28", "1.010000000000"),
        ],
        "202405" => vec![(CNPJ_A, "2024-05-31", "1.200000000000")],
        "202406" => vec![
            (CNPJ_A, "2024-06-03", "1.201000000000"),
            (CNPJ_A, "2024-06-28", "1.210000000000"),
        ],
        "202611" => vec![
            (CNPJ_A, "2026-11-03", "1.131000000000"),
            (CNPJ_A, "2026-11-05", "1.132000000000"),
            (CNPJ_B, "2026-11-05", "2.221000000000"),
        ],
        outro => panic!("sem fixture para {outro}"),
    }
}

fn formatado(cnpj: &str) -> String {
    format!(
        "{}.{}.{}/{}-{}",
        &cnpj[0..2],
        &cnpj[2..5],
        &cnpj[5..8],
        &cnpj[8..12],
        &cnpj[12..14]
    )
}

/// Gera os zips do Informe Diário dos meses pedidos numa pasta temporária.
fn pasta_cvm(meses: &[&str]) -> tempfile::TempDir {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let dir = tempfile::tempdir().unwrap();
    for mes in meses {
        let mut csv = String::from(
            "TP_FUNDO_CLASSE;CNPJ_FUNDO_CLASSE;ID_SUBCLASSE;DT_COMPTC;VL_TOTAL;VL_QUOTA\n",
        );
        for (cnpj, dia, cota) in linhas_cvm(mes) {
            csv.push_str(&format!("FI;{};;{dia};1000.00;{cota}\n", formatado(cnpj)));
        }
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        z.start_file(
            format!("inf_diario_fi_{mes}.csv"),
            SimpleFileOptions::default(),
        )
        .unwrap();
        z.write_all(csv.as_bytes()).unwrap();
        let bytes = z.finish().unwrap().into_inner();
        std::fs::write(dir.path().join(format!("inf_diario_fi_{mes}.zip")), bytes).unwrap();
    }
    dir
}

fn falso_cvm(pasta: &tempfile::TempDir) -> Falso {
    Falso {
        cvm: Some(pasta.path().to_path_buf()),
        ..Falso::novo()
    }
}

/// Cria um fundo aplicado em `aplicacao` e grava `codigo` no `codigo_b3` (como a importação).
fn criar_fundo(dir: &tempfile::TempDir, db: &Db, aplicacao: &str, codigo: Option<&str>) -> i64 {
    let id = match db
        .criar_investimento(
            &NovoInvestimento {
                tipo: "fundo".into(),
                instituicao: "Corretora Exemplo".into(),
                emissor: s("Fundo Exemplo"),
                data_aplicacao: s(aplicacao),
                valor_aplicado_centavos: Some(1_000_000),
                liquidez: s("d30"),
                ..Default::default()
            },
            data(2026, 10, 28),
        )
        .unwrap()
    {
        Gravacao::Ok(id) => id,
        Gravacao::Invalido(e) => panic!("inválido: {e:?}"),
    };
    if let Some(c) = codigo {
        trocar_codigo(dir, id, c);
    }
    id
}

fn trocar_codigo(dir: &tempfile::TempDir, id: i64, codigo: &str) {
    let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
    conn.execute(
        "UPDATE investimentos SET codigo_b3 = ?1 WHERE id = ?2",
        rusqlite::params![codigo, id],
    )
    .unwrap();
}

/// Os meses (`AAAAMM`) pedidos à CVM, na ordem dos pedidos.
fn meses_pedidos(f: &Falso) -> Vec<String> {
    f.com("inf_diario_fi_")
        .iter()
        .map(|u| u[u.len() - 10..u.len() - 4].to_string())
        .collect()
}

fn cota(db: &Db, cnpj: &str, d: NaiveDate) -> Option<(NaiveDate, rust_decimal::Decimal)> {
    db.cota_fundo_em(cnpj, d).unwrap()
}

fn dec(v: &str) -> rust_decimal::Decimal {
    v.parse().unwrap()
}

#[test]
fn cvm_baixa_os_meses_do_fundo() {
    let (dir, db) = abrir();
    criar_fundo(&dir, &db, "2026-09-08", Some("11.222.333/0001-81"));
    let cvm = pasta_cvm(&["202609", "202610", "202611"]);
    let falso = falso_cvm(&cvm);
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje_cvm(), &esperas.fecha()).unwrap();

    let mut meses = meses_pedidos(&falso);
    meses.sort();
    assert_eq!(meses, ["202609", "202610", "202611"]);
    assert_eq!(
        cota(&db, CNPJ_A, data(2026, 9, 8)),
        Some((data(2026, 9, 8), dec("1.105")))
    );
    assert_eq!(
        cota(&db, CNPJ_A, data(2026, 10, 31)),
        Some((data(2026, 10, 30), dec("1.13")))
    );
    assert_eq!(
        cota(&db, CNPJ_A, hoje_cvm()),
        Some((data(2026, 11, 5), dec("1.132")))
    );
    // O outro CNPJ do CSV não entra.
    assert_eq!(cota(&db, CNPJ_B, hoje_cvm()), None);

    let st = status(&db, "cvm");
    assert_eq!(st.erro, None);
    assert!(st.atualizado_em.is_some());
    assert_eq!(st.ultima_data.as_deref(), Some("2026-11-05"));
}

#[test]
fn cvm_nao_roda_sem_fundo_com_cnpj() {
    let (dir, db) = abrir();
    criar_fundo(&dir, &db, "2026-09-08", None);
    // Dígito verificador errado: não é CNPJ válido.
    criar_fundo(&dir, &db, "2026-09-08", Some("11.222.333/0001-82"));
    let cvm = pasta_cvm(&["202609", "202610", "202611"]);
    let falso = falso_cvm(&cvm);
    atualizar(&db, &falso, hoje_cvm(), &Esperas::novo().fecha()).unwrap();

    assert!(falso.com("cvm.gov.br").is_empty());
    let st = status(&db, "cvm");
    assert_eq!(st.erro, None);
    assert_eq!(st.ultima_data, None);
    assert_eq!(st.atualizado_em, None);
}

#[test]
fn cvm_mes_fechado_nao_e_baixado_de_novo() {
    let (dir, db) = abrir();
    criar_fundo(&dir, &db, "2026-09-08", Some(CNPJ_A));
    let cvm = pasta_cvm(&["202609", "202610", "202611"]);
    let esperas = Esperas::novo();
    atualizar(&db, &falso_cvm(&cvm), hoje_cvm(), &esperas.fecha()).unwrap();

    // Dia seguinte: 202609 fechou (marca de 07/11 ≥ 30/09 + 10 dias). 202610 só fecha em
    // 31/10 + 10 = 10/11, então a marca de 07/11 ainda não basta; 202611 está aberto.
    let falso = falso_cvm(&cvm);
    atualizar(&db, &falso, data(2026, 11, 8), &esperas.fecha()).unwrap();
    let mut meses = meses_pedidos(&falso);
    meses.sort();
    assert_eq!(meses, ["202610", "202611"]);

    // Baixado em 11/11 (depois de 10/11), 202610 fica fechado de vez.
    atualizar(&db, &falso_cvm(&cvm), data(2026, 11, 11), &esperas.fecha()).unwrap();
    let falso = falso_cvm(&cvm);
    atualizar(&db, &falso, data(2026, 11, 12), &esperas.fecha()).unwrap();
    assert_eq!(meses_pedidos(&falso), ["202611"]);
}

#[test]
fn cvm_mesmo_dia_nao_repete() {
    let (dir, db) = abrir();
    criar_fundo(&dir, &db, "2026-09-08", Some(CNPJ_A));
    let cvm = pasta_cvm(&["202609", "202610", "202611"]);
    let falso = falso_cvm(&cvm);
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje_cvm(), &esperas.fecha()).unwrap();
    atualizar(&db, &falso, hoje_cvm(), &esperas.fecha()).unwrap();

    let mut meses = meses_pedidos(&falso);
    meses.sort();
    assert_eq!(meses, ["202609", "202610", "202611"]);
}

#[test]
fn cvm_cnpj_novo_baixa_meses_ja_marcados() {
    let (dir, db) = abrir();
    let id = criar_fundo(&dir, &db, "2026-09-08", Some(CNPJ_A));
    let cvm = pasta_cvm(&["202609", "202610", "202611"]);
    let esperas = Esperas::novo();
    atualizar(&db, &falso_cvm(&cvm), hoje_cvm(), &esperas.fecha()).unwrap();
    assert_eq!(cota(&db, CNPJ_B, data(2026, 9, 30)), None);

    trocar_codigo(&dir, id, "22.333.444/0001-81");
    let falso = falso_cvm(&cvm);
    atualizar(&db, &falso, data(2026, 11, 8), &esperas.fecha()).unwrap();

    assert!(meses_pedidos(&falso).contains(&"202609".to_string()));
    assert_eq!(
        cota(&db, CNPJ_B, data(2026, 9, 30)),
        Some((data(2026, 9, 30), dec("2.21")))
    );
    assert_eq!(status(&db, "cvm").erro, None);
}

#[test]
fn cvm_aplicacao_sem_cota_busca_mes_anterior() {
    let (dir, db) = abrir();
    criar_fundo(&dir, &db, "2026-10-01", Some(CNPJ_A));
    let cvm = pasta_cvm(&["202609", "202610", "202611"]);
    let falso = falso_cvm(&cvm);
    atualizar(&db, &falso, hoje_cvm(), &Esperas::novo().fecha()).unwrap();

    let mut meses = meses_pedidos(&falso);
    meses.sort();
    assert_eq!(meses, ["202609", "202610", "202611"]);
    assert_eq!(
        cota(&db, CNPJ_A, data(2026, 10, 1)),
        Some((data(2026, 9, 30), dec("1.12")))
    );
}

#[test]
fn cvm_404_nao_e_erro() {
    let (dir, db) = abrir();
    criar_fundo(&dir, &db, "2026-09-08", Some(CNPJ_A));
    let cvm = pasta_cvm(&["202609", "202610"]);
    let falso = falso_cvm(&cvm);
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje_cvm(), &esperas.fecha()).unwrap();

    // O 404 é resposta definitiva: pedido uma vez só, sem espera.
    assert_eq!(falso.com("inf_diario_fi_202611").len(), 1);
    assert_eq!(esperas.0.get(), 0);
    let st = status(&db, "cvm");
    assert_eq!(st.erro, None);
    assert_eq!(st.ultima_data.as_deref(), Some("2026-10-30"));
    assert!(cota(&db, CNPJ_A, data(2026, 9, 30)).is_some());
    assert_eq!(
        cota(&db, CNPJ_A, hoje_cvm()),
        Some((data(2026, 10, 30), dec("1.13")))
    );
}

#[test]
fn cvm_fora_do_ar_nao_derruba_as_outras() {
    let (dir, db) = abrir();
    criar_fundo(&dir, &db, "2026-09-08", Some(CNPJ_A));
    let cvm = pasta_cvm(&["202609", "202610", "202611"]);
    let falso = Falso {
        sem_rede: vec!["cvm.gov.br"],
        b3_qualquer_dia: true,
        ..falso_cvm(&cvm)
    };
    let esperas = Esperas::novo();
    atualizar(&db, &falso, hoje_cvm(), &esperas.fecha()).unwrap();

    // Uma nova tentativa e a fonte desiste.
    assert_eq!(falso.com("cvm.gov.br").len(), 2);
    assert_eq!(esperas.0.get(), 1);
    let st = status(&db, "cvm");
    assert_eq!(st.erro.as_deref(), Some(MSG_CVM));
    assert_eq!(st.ultima_data, None);
    assert_eq!(st.atualizado_em, None);
    for fonte in ["cdi", "selic", "b3"] {
        let st = status(&db, fonte);
        assert_eq!(st.erro, None, "{fonte}");
        assert!(st.atualizado_em.is_some(), "{fonte}");
    }
}

#[test]
fn cvm_ignora_meses_antes_de_2021() {
    let (dir, db) = abrir();
    criar_fundo(&dir, &db, "2019-05-10", Some(CNPJ_A));
    let cvm = pasta_cvm(&["202609", "202610", "202611"]);
    let falso = falso_cvm(&cvm);
    atualizar(&db, &falso, hoje_cvm(), &Esperas::novo().fecha()).unwrap();

    let mut meses = meses_pedidos(&falso);
    meses.sort();
    // Só a janela de 13 meses antes do mês de hoje até o mês de hoje: 2025-10 a 2026-11.
    let esperados: Vec<String> = (0..14u32)
        .map(|k| {
            let m = data(2025, 10, 1)
                .checked_add_months(chrono::Months::new(k))
                .unwrap();
            m.format("%Y%m").to_string()
        })
        .collect();
    assert_eq!(meses, esperados);
    assert!(meses.iter().all(|m| m.as_str() >= "202101"));
    assert_eq!(status(&db, "cvm").erro, None);
}

#[test]
fn cvm_cota_base_antiga_nao_cobre_a_aplicacao() {
    let (dir, db) = abrir();
    // Dois fundos no mesmo CNPJ, os dois antes da janela. O segundo foi aplicado num sábado
    // (01/06/2024); a cota de 28/02/2023, do primeiro, não serve de base para ele.
    criar_fundo(&dir, &db, "2023-02-10", Some(CNPJ_A));
    criar_fundo(&dir, &db, "2024-06-01", Some(CNPJ_A));
    let cvm = pasta_cvm(&["202302", "202405", "202406", "202609", "202610", "202611"]);
    let falso = falso_cvm(&cvm);
    atualizar(&db, &falso, hoje_cvm(), &Esperas::novo().fecha()).unwrap();

    assert!(meses_pedidos(&falso).contains(&"202405".to_string()));
    assert_eq!(
        cota(&db, CNPJ_A, data(2024, 6, 1)),
        Some((data(2024, 5, 31), dec("1.2")))
    );
    assert_eq!(status(&db, "cvm").erro, None);
}
