use chrono::NaiveDate;
use guru_core::{
    AbaRv, Acao, Comparacao, Db, Decisao, Estado, FonteImportacao, Gravacao, Item, LinhaB3,
    NovoInvestimento, Posicao, SCHEMA_VERSION, aplicar_importacao, comparar, ler_posicao,
    ultima_importacao,
};

const COMPLETA: &[u8] = include_bytes!("fixtures/b3/posicao-completa.xlsx");

fn hoje() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 30).unwrap()
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn abrir() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    (dir, db)
}

fn criar(db: &Db, novo: NovoInvestimento) -> i64 {
    match db.criar_investimento(&novo, hoje()).unwrap() {
        Gravacao::Ok(id) => id,
        Gravacao::Invalido(e) => panic!("inválido: {e:?}"),
    }
}

fn rv(tipo: &str, ticker: &str, qtd: i64) -> NovoInvestimento {
    NovoInvestimento {
        tipo: tipo.into(),
        instituicao: "Corretora Fictícia".into(),
        ticker: s(ticker),
        data_aplicacao: s("2025-04-01"),
        quantidade: Some(qtd),
        preco_medio_centavos: Some(3_000),
        ..Default::default()
    }
}

fn cdb(emissor: &str, vencimento: &str) -> NovoInvestimento {
    NovoInvestimento {
        tipo: "rf-cdb".into(),
        instituicao: "Corretora Fictícia".into(),
        emissor: s(emissor),
        indexador: s("pos"),
        taxa: s("102"),
        benchmark: s("cdi"),
        data_aplicacao: s("2025-03-15"),
        vencimento: s(vencimento),
        valor_aplicado_centavos: Some(2_000_000),
        liquidez: s("vencimento"),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn selic(aplicado: i64) -> NovoInvestimento {
    NovoInvestimento {
        tipo: "td-selic".into(),
        instituicao: "Corretora Fictícia".into(),
        taxa: s("0.09"),
        data_aplicacao: s("2025-01-10"),
        vencimento: s("2029-03-01"),
        valor_aplicado_centavos: Some(aplicado),
        ir: s("regressiva"),
        ..Default::default()
    }
}

fn comparar_completa(db: &Db) -> Comparacao {
    let posicao = ler_posicao(COMPLETA).unwrap();
    comparar(db, &posicao, hoje()).unwrap()
}

fn item<'a>(c: &'a Comparacao, ativo: &str) -> &'a Item {
    c.itens
        .iter()
        .find(|i| i.ativo == ativo)
        .unwrap_or_else(|| panic!("sem item {ativo}: {:?}", c.itens))
}

fn item_cdb<'a>(c: &'a Comparacao, emissor: &str) -> &'a Item {
    c.itens
        .iter()
        .find(|i| i.ativo.starts_with("CDB") && i.ativo.contains(emissor))
        .unwrap_or_else(|| panic!("sem CDB {emissor}: {:?}", c.itens))
}

fn marcar_importado(dir: &tempfile::TempDir, id: i64, codigo: &str) {
    let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
    conn.execute(
        "UPDATE investimentos SET codigo_b3 = ?1 WHERE id = ?2",
        rusqlite::params![codigo, id],
    )
    .unwrap();
}

#[test]
fn carteira_vazia_tudo_novo() {
    let (_d, db) = abrir();
    let c = comparar_completa(&db);
    assert_eq!(c.itens.len(), 9);
    assert!(c.itens.iter().all(|i| i.estado == Estado::Novo));
    assert!(
        c.itens
            .iter()
            .all(|i| i.acao_padrao == Acao::Adicionar && i.sugestao.is_some())
    );
    assert_eq!(c.itens[0].chave.split(':').next(), Some("b3"));

    let itub = item(&c, "ITUB4");
    assert_eq!(itub.no_arquivo.as_deref(), Some("150 ações"));
    assert_eq!(itub.no_app, None);
    let sug = itub.sugestao.as_ref().unwrap();
    assert_eq!(sug.tipo, "rv-acao");
    assert_eq!(sug.quantidade, Some(150));
    let campos: Vec<&str> = itub.faltando.iter().map(|e| e.campo.as_str()).collect();
    assert!(campos.contains(&"precoMedioCentavos"), "{campos:?}");
    assert!(campos.contains(&"dataAplicacao"), "{campos:?}");

    let cdb = item_cdb(&c, "FICTICIO");
    let sug = cdb.sugestao.as_ref().unwrap();
    assert_eq!(sug.tipo, "rf-cdb");
    assert_eq!(sug.indexador.as_deref(), Some("pos"));
    assert_eq!(sug.benchmark.as_deref(), Some("cdi"));
    assert!(sug.data_aplicacao.is_some());
    let campos: Vec<&str> = cdb.faltando.iter().map(|e| e.campo.as_str()).collect();
    assert!(campos.contains(&"valorAplicadoCentavos"), "{campos:?}");
    assert!(campos.contains(&"taxa"), "{campos:?}");

    let td = item(&c, "Tesouro Selic 2029");
    let sug = td.sugestao.as_ref().unwrap();
    assert_eq!(sug.tipo, "td-selic");
    assert_eq!(sug.valor_aplicado_centavos, Some(800_000));
    assert_eq!(td.no_arquivo.as_deref(), Some("R$ 8.000,00"));

    assert_eq!(item(&c, "BOVA11").no_arquivo.as_deref(), Some("30 cotas"));
    assert_eq!(item(&c, "AAPL34").no_arquivo.as_deref(), Some("10 BDRs"));
}

#[test]
fn quantidade_diferente_e_divergente_e_importado_e_atualizar() {
    let (d, db) = abrir();
    let id = criar(&db, rv("rv-acao", "ITUB4", 120));
    let c = comparar_completa(&db);
    let i = item(&c, "ITUB4");
    assert_eq!(i.estado, Estado::Divergente);
    assert_eq!(i.no_app.as_deref(), Some("120 ações"));
    assert_eq!(i.no_arquivo.as_deref(), Some("150 ações"));
    assert_eq!(i.investimentos, vec![id]);
    assert_eq!(i.acao_padrao, Acao::Manter);
    assert!(i.sugestao.is_none());

    marcar_importado(&d, id, "ITUB4");
    let c = comparar_completa(&db);
    let i = item(&c, "ITUB4");
    assert_eq!(i.estado, Estado::Atualizar);
    assert_eq!(i.acao_padrao, Acao::UsarArquivo);
    assert!(i.acoes.contains(&Acao::UsarArquivo) && i.acoes.contains(&Acao::Manter));
}

#[test]
fn compras_somadas_sao_igual_ou_divergente_so_com_manter() {
    let (_d, db) = abrir();
    criar(&db, rv("rv-acao", "itub4", 100));
    let id2 = criar(&db, rv("rv-acao", "ITUB4", 50));
    let c = comparar_completa(&db);
    let i = item(&c, "ITUB4");
    assert_eq!(i.estado, Estado::Igual);
    assert_eq!(i.acao_padrao, Acao::Manter);
    assert_eq!(i.investimentos.len(), 2);

    db.atualizar_investimento(id2, &rv("rv-acao", "ITUB4", 40), hoje())
        .unwrap();
    let c = comparar_completa(&db);
    let i = item(&c, "ITUB4");
    assert_eq!(i.estado, Estado::Divergente);
    assert_eq!(i.no_app.as_deref(), Some("140 ações"));
    assert_eq!(i.acoes, vec![Acao::Manter]);
}

#[test]
fn compras_importadas_somadas_nao_escolhem_onde_atualizar() {
    let (d, db) = abrir();
    let a = criar(&db, rv("rv-acao", "ITUB4", 100));
    let b = criar(&db, rv("rv-acao", "ITUB4", 40));
    marcar_importado(&d, a, "ITUB4");
    marcar_importado(&d, b, "ITUB4");
    let c = comparar_completa(&db);
    let i = item(&c, "ITUB4");
    assert_eq!(i.estado, Estado::Divergente);
    assert_eq!(i.acoes, vec![Acao::Manter]);
}

#[test]
fn cdb_casa_pelo_emissor_normalizado() {
    let (_d, db) = abrir();
    let id = criar(&db, cdb("Banco Fictício SA", "2029-03-15"));
    let c = comparar_completa(&db);
    let i = item_cdb(&c, "FICTICIO");
    assert_eq!(i.estado, Estado::Igual);
    assert_eq!(i.investimentos, vec![id]);
    assert!(i.sugestao.is_none());
    // O outro CDB (vencimento 2027) continua novo.
    assert_eq!(item_cdb(&c, "EXEMPLO").estado, Estado::Novo);
}

#[test]
fn cdb_casa_primeiro_pelo_codigo_importado() {
    let (d, db) = abrir();
    let id = criar(&db, cdb("Outro Banco", "2030-01-01"));
    marcar_importado(&d, id, "CDB224ABCDE");
    let c = comparar_completa(&db);
    let i = c
        .itens
        .iter()
        .find(|i| i.investimentos == vec![id])
        .unwrap();
    assert_eq!(i.estado, Estado::Igual);
}

#[test]
fn dois_candidatos_sao_divergentes_com_vincular() {
    let (_d, db) = abrir();
    let a = criar(&db, cdb("Banco Ficticio", "2029-03-15"));
    let b = criar(&db, cdb("BANCO FICTICIO S.A.", "2029-03-15"));
    let c = comparar_completa(&db);
    let i = c
        .itens
        .iter()
        .find(|i| i.estado == Estado::Divergente)
        .unwrap();
    assert_eq!(i.acao_padrao, Acao::Manter);
    assert!(i.acoes.contains(&Acao::Vincular(a)));
    assert!(i.acoes.contains(&Acao::Vincular(b)));
    assert!(i.acoes.contains(&Acao::Adicionar));
    assert!(i.sugestao.is_some());
    assert!(!i.faltando.is_empty());
}

#[test]
fn tesouro_igual_ou_divergente_pelo_valor_aplicado() {
    let (_d, db) = abrir();
    criar(&db, selic(800_000));
    let c = comparar_completa(&db);
    assert_eq!(item(&c, "Tesouro Selic 2029").estado, Estado::Igual);

    let (d2, db2) = abrir();
    let id = criar(&db2, selic(700_000));
    let c = comparar_completa(&db2);
    let i = item(&c, "Tesouro Selic 2029");
    assert_eq!(i.estado, Estado::Divergente);
    assert_eq!(i.no_app.as_deref(), Some("R$ 7.000,00"));
    assert_eq!(i.no_arquivo.as_deref(), Some("R$ 8.000,00"));

    marcar_importado(&d2, id, "BRSTNCLF1RC8");
    let c = comparar_completa(&db2);
    assert_eq!(item(&c, "Tesouro Selic 2029").estado, Estado::Atualizar);
}

#[test]
fn investimento_que_nao_veio_na_planilha_nao_aparece() {
    let (_d, db) = abrir();
    criar(&db, cdb("Banco Sem Planilha", "2031-01-10"));
    let c = comparar_completa(&db);
    assert!(c.itens.iter().all(|i| i.ativo != "Banco Sem Planilha"));
    assert!(c.itens.iter().all(|i| i.chave.starts_with("b3:")));
    assert_eq!(c.fonte, FonteImportacao::B3);
    assert_eq!(c.data_arquivo, None);
}

#[test]
fn importacao_grava_a_fonte() {
    let (d, db) = abrir();
    let c = comparar_completa(&db);
    aplicar(&db, &c, &[]);
    let fonte: String = rusqlite::Connection::open(d.path().join("guru.db"))
        .unwrap()
        .query_row("SELECT fonte FROM importacao", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fonte, "b3");
}

#[test]
fn ordem_dos_estados_e_codigos() {
    let (_d, db) = abrir();
    criar(&db, rv("rv-acao", "ITUB4", 150));
    criar(&db, rv("rv-acao", "B3SA3", 10));
    criar(&db, rv("rv-acao", "PETR4", 10));
    let c = comparar_completa(&db);
    let estados: Vec<Estado> = c.itens.iter().map(|i| i.estado).collect();
    let mut ordenados = estados.clone();
    let peso = |e: &Estado| match e {
        Estado::Novo => 0,
        Estado::Atualizar => 1,
        Estado::Divergente => 2,
        Estado::Igual => 3,
    };
    ordenados.sort_by_key(peso);
    assert_eq!(estados, ordenados);
    assert_eq!(Acao::UsarArquivo.codigo(), "usar_arquivo");
    assert_eq!(Acao::Vincular(3).codigo(), "vincular");
    assert_eq!(c.linhas_lidas, 9);
}

#[test]
fn migra_banco_da_versao_4() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = Db::open(dir.path()).unwrap();
        criar(&db, rv("rv-acao", "ITUB4", 10));
    }
    {
        let conn = rusqlite::Connection::open(dir.path().join("guru.db")).unwrap();
        conn.execute_batch(
            "DROP TABLE cota_fundo;
             DROP TABLE ia_gerado;
             ALTER TABLE investimentos DROP COLUMN codigo_b3;
             ALTER TABLE investimentos DROP COLUMN saldo_centavos;
             ALTER TABLE investimentos DROP COLUMN saldo_data;
             DROP TABLE importacao;
             DROP TABLE ia_uso;
             DROP TABLE mensagem;
             DROP TABLE conversa;
             DROP TABLE ia_contexto_parte;
             DROP TABLE ia_config;
             PRAGMA user_version = 4;",
        )
        .unwrap();
    }
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), 9);
    assert_eq!(SCHEMA_VERSION, 9);
    let lista = db.listar_investimentos(hoje()).unwrap();
    assert_eq!(lista.len(), 1);
    assert_eq!(lista[0].codigo_b3, None);
}

fn rv_b3(ticker: &str, instituicao: &str, quantidade: i64) -> LinhaB3 {
    LinhaB3::RendaVariavel {
        aba: AbaRv::Acao,
        ticker: ticker.into(),
        produto: format!("{ticker} - EMPRESA"),
        instituicao: instituicao.into(),
        quantidade,
        preco_fechamento: None,
        valor_atualizado: None,
    }
}

fn cdb_b3(codigo: &str, prefixo: &str, vencimento: NaiveDate) -> LinhaB3 {
    LinhaB3::RendaFixa {
        produto: format!("{prefixo} - BANCO FICTICIO S/A"),
        tipo: match prefixo {
            "CDB" => Some("rf-cdb".to_string()),
            _ => None,
        },
        instituicao: "Corretora Fictícia".into(),
        emissor: "BANCO FICTICIO S/A".into(),
        codigo: codigo.into(),
        indexador: None,
        benchmark: None,
        emissao: None,
        vencimento,
        quantidade: 1.into(),
        valor_curva: None,
    }
}

fn posicao(linhas: Vec<LinhaB3>) -> Posicao {
    Posicao {
        linhas_lidas: linhas.len(),
        linhas,
        avisos: Vec::new(),
    }
}

#[test]
fn investimento_casado_nao_serve_a_uma_segunda_linha() {
    let (_d, db) = abrir();
    let id = criar(&db, cdb("Banco Fictício SA", "2029-03-15"));
    let venc = NaiveDate::from_ymd_opt(2029, 3, 15).unwrap();
    let p = posicao(vec![cdb_b3("X1", "CDB", venc), cdb_b3("X2", "CDB", venc)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    let primeiro = c.itens.iter().find(|i| i.chave == "b3:0").unwrap();
    let segundo = c.itens.iter().find(|i| i.chave == "b3:1").unwrap();
    assert_eq!(primeiro.estado, Estado::Igual);
    assert_eq!(primeiro.investimentos, vec![id]);
    assert_eq!(segundo.estado, Estado::Novo);
}

#[test]
fn prefixo_desconhecido_deixa_o_tipo_em_branco() {
    let (_d, db) = abrir();
    let venc = NaiveDate::from_ymd_opt(2029, 3, 15).unwrap();
    let p = posicao(vec![cdb_b3("X1", "XYZ", venc)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    let i = &c.itens[0];
    assert_eq!(i.estado, Estado::Novo);
    assert_eq!(i.sugestao.as_ref().unwrap().tipo, "");
    assert!(i.faltando.iter().any(|e| e.campo == "tipo"));
}

#[test]
fn tesouro_tolera_um_centavo() {
    let tesouro_b3 = |centavos: i64| LinhaB3::Tesouro {
        produto: "Tesouro Selic 2029".into(),
        tipo: "td-selic".into(),
        isin: None,
        instituicao: "Corretora Fictícia".into(),
        vencimento: NaiveDate::from_ymd_opt(2029, 3, 1).unwrap(),
        valor_aplicado: Some(rust_decimal::Decimal::new(centavos, 2)),
        valor_atualizado: None,
    };
    let (_d, db) = abrir();
    criar(&db, selic(800_000));
    let estado = |c: i64| {
        comparar(&db, &posicao(vec![tesouro_b3(c)]), hoje())
            .unwrap()
            .itens[0]
            .estado
    };
    assert_eq!(estado(800_001), Estado::Igual);
    assert_eq!(estado(799_999), Estado::Igual);
    assert_eq!(estado(800_002), Estado::Divergente);
}

fn tesouro_b3(instituicao: &str, centavos: i64) -> LinhaB3 {
    LinhaB3::Tesouro {
        produto: "Tesouro Selic 2029".into(),
        tipo: "td-selic".into(),
        isin: None,
        instituicao: instituicao.into(),
        vencimento: NaiveDate::from_ymd_opt(2029, 3, 1).unwrap(),
        valor_aplicado: Some(rust_decimal::Decimal::new(centavos, 2)),
        valor_atualizado: None,
    }
}

fn rv_em(instituicao: &str, qtd: i64) -> NovoInvestimento {
    NovoInvestimento {
        instituicao: instituicao.into(),
        ..rv("rv-acao", "PETR4", qtd)
    }
}

const XP_B3: &str = "XP INVESTIMENTOS CCTVM S/A";
const BTG_B3: &str = "BTG PACTUAL CTVM S/A";

#[test]
fn mesmo_ticker_em_duas_corretoras_vira_dois_itens() {
    let (_d, db) = abrir();
    let p = posicao(vec![rv_b3("PETR4", XP_B3, 10), rv_b3("PETR4", BTG_B3, 1)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    assert_eq!(c.itens.len(), 2);
    assert!(c.itens.iter().all(|i| i.estado == Estado::Novo));
    let xp = c.itens.iter().find(|i| i.instituicao == XP_B3).unwrap();
    let btg = c.itens.iter().find(|i| i.instituicao == BTG_B3).unwrap();
    let sug = xp.sugestao.as_ref().unwrap();
    assert_eq!(
        (sug.quantidade, sug.instituicao.as_str()),
        (Some(10), XP_B3)
    );
    let sug = btg.sugestao.as_ref().unwrap();
    assert_eq!(
        (sug.quantidade, sug.instituicao.as_str()),
        (Some(1), BTG_B3)
    );
}

#[test]
fn cada_corretora_casa_com_o_seu_investimento() {
    let (_d, db) = abrir();
    criar(&db, rv_em("XP Investimentos", 10));
    criar(&db, rv_em("BTG Pactual", 1));
    let p = posicao(vec![rv_b3("PETR4", XP_B3, 10), rv_b3("PETR4", BTG_B3, 1)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    assert_eq!(c.itens.len(), 2);
    assert!(c.itens.iter().all(|i| i.estado == Estado::Igual));
    assert!(c.itens.iter().all(|i| i.investimentos.len() == 1));
}

#[test]
fn soma_antiga_vira_atualizar_da_corretora() {
    let (d, db) = abrir();
    let id = criar(&db, rv_em("XP Investimentos", 11));
    marcar_importado(&d, id, "PETR4");
    let p = posicao(vec![rv_b3("PETR4", XP_B3, 10), rv_b3("PETR4", BTG_B3, 1)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    let xp = c.itens.iter().find(|i| i.instituicao == XP_B3).unwrap();
    let btg = c.itens.iter().find(|i| i.instituicao == BTG_B3).unwrap();
    assert_eq!(xp.estado, Estado::Atualizar);
    assert_eq!(xp.acao_padrao, Acao::UsarArquivo);
    assert_eq!(btg.estado, Estado::Novo);
    // Sem decisões, nada muda: a quantidade da XP continua 11.
    let r = aplicar(&db, &c, &[]);
    assert_eq!(r.aplicadas, 0);
    let lista = db.listar_investimentos(hoje()).unwrap();
    assert_eq!(lista.len(), 1);
    assert_eq!(lista[0].quantidade, Some(11));
}

#[test]
fn tesouro_separado_por_corretora() {
    let (_d, db) = abrir();
    let p = posicao(vec![
        tesouro_b3(XP_B3, 500_000),
        tesouro_b3(BTG_B3, 300_000),
    ]);
    let c = comparar(&db, &p, hoje()).unwrap();
    assert_eq!(c.itens.len(), 2);
    let xp = c.itens.iter().find(|i| i.instituicao == XP_B3).unwrap();
    assert_eq!(
        xp.sugestao.as_ref().unwrap().valor_aplicado_centavos,
        Some(500_000)
    );
    let btg = c.itens.iter().find(|i| i.instituicao == BTG_B3).unwrap();
    assert_eq!(
        btg.sugestao.as_ref().unwrap().valor_aplicado_centavos,
        Some(300_000)
    );
}

#[test]
fn ticker_em_outra_corretora_do_catalogo_vira_novo() {
    let (_d, db) = abrir();
    let id = criar(&db, rv_em("XP Investimentos", 10));
    let p = posicao(vec![rv_b3("PETR4", BTG_B3, 10)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    assert_eq!(c.itens.len(), 1);
    let i = &c.itens[0];
    assert_eq!(i.estado, Estado::Novo);
    assert_eq!(i.acao_padrao, Acao::Adicionar);
    assert!(!i.investimentos.contains(&id));
}

#[test]
fn corretora_digitada_a_mao_oferece_vincular_em_vez_de_duplicar() {
    let (_d, db) = abrir();
    let id = criar(&db, rv_em("Genial", 10));
    let p = posicao(vec![rv_b3("PETR4", "GENIAL INVESTIMENTOS CVM S.A.", 10)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    assert_eq!(c.itens.len(), 1);
    let i = &c.itens[0];
    assert_eq!(i.estado, Estado::Divergente);
    assert_eq!(i.acao_padrao, Acao::Ignorar);
    assert_eq!(
        i.acoes,
        vec![Acao::Vincular(id), Acao::Adicionar, Acao::Ignorar]
    );
    assert!(i.sugestao.is_some());
    assert_eq!(i.investimentos, vec![id]);
    // Sem decisões, nada muda.
    assert_eq!(aplicar(&db, &c, &[]).aplicadas, 0);
    assert_eq!(total(&db), 1);
}

#[test]
fn rf_com_codigo_em_outra_corretora_nao_vira_novo() {
    // Corretora do catálogo com o mesmo código: candidato de outra corretora, o padrão é ignorar.
    let (d, db) = abrir();
    let id = criar(
        &db,
        NovoInvestimento {
            instituicao: "XP Investimentos".into(),
            ..cdb("Banco Fictício SA", "2029-03-15")
        },
    );
    marcar_importado(&d, id, "X1");
    let venc = NaiveDate::from_ymd_opt(2029, 3, 15).unwrap();
    let c = comparar(&db, &posicao(vec![cdb_b3("X1", "CDB", venc)]), hoje()).unwrap();
    let i = &c.itens[0];
    assert_eq!(i.estado, Estado::Divergente);
    assert_eq!(i.acao_padrao, Acao::Ignorar);
    assert!(i.acoes.contains(&Acao::Vincular(id)));
    assert_eq!(aplicar(&db, &c, &[]).aplicadas, 0);
    assert_eq!(total(&db), 1);
}

#[test]
fn corretora_digitada_a_mao_com_o_codigo_casa_como_da_mesma_corretora() {
    // Depois de um `Vincular`, o investimento de corretora fora do catálogo tem o código da
    // linha: a próxima comparação o trata como par, sem mudar a corretora dele.
    let (d, db) = abrir();
    let acao = criar(&db, rv_em("Genial", 10));
    marcar_importado(&d, acao, "PETR4");
    let rf = criar(
        &db,
        NovoInvestimento {
            instituicao: "Outra Corretora".into(),
            ..cdb("Banco Fictício SA", "2029-03-15")
        },
    );
    marcar_importado(&d, rf, "X1");
    let td = criar(
        &db,
        NovoInvestimento {
            instituicao: "Genial".into(),
            ..selic(800_000)
        },
    );
    marcar_importado(&d, td, "td-selic:2029-03-01");
    let venc = NaiveDate::from_ymd_opt(2029, 3, 15).unwrap();
    let p = posicao(vec![
        rv_b3("PETR4", BTG_B3, 10),
        cdb_b3("X1", "CDB", venc),
        tesouro_b3(XP_B3, 800_000),
    ]);
    let c = comparar(&db, &p, hoje()).unwrap();
    for (chave, id) in [("b3:0", acao), ("b3:1", rf), ("b3:2", td)] {
        let i = c.itens.iter().find(|i| i.chave == chave).unwrap();
        assert_eq!(i.estado, Estado::Igual, "{chave}");
        assert_eq!(i.investimentos, vec![id], "{chave}");
    }
    aplicar(&db, &c, &[]);
    assert_eq!(db.investimento(acao, hoje()).unwrap().instituicao, "Genial");

    // Quantidade diferente: atualizar, e usar o arquivo não muda a corretora.
    let c = comparar(&db, &posicao(vec![rv_b3("PETR4", BTG_B3, 12)]), hoje()).unwrap();
    let i = &c.itens[0];
    assert_eq!(i.estado, Estado::Atualizar);
    aplicar(&db, &c, &[decisao(&i.chave, Acao::UsarArquivo)]);
    let a = db.investimento(acao, hoje()).unwrap();
    assert_eq!(a.instituicao, "Genial");
    assert_eq!(a.quantidade, Some(12));
}

#[test]
fn tesouro_em_outra_corretora_oferece_vincular() {
    let (_d, db) = abrir();
    let id = criar(&db, selic(800_000));
    let c = comparar(
        &db,
        &posicao(vec![tesouro_b3("Genial CVM", 800_000)]),
        hoje(),
    )
    .unwrap();
    let i = &c.itens[0];
    assert_eq!(i.estado, Estado::Divergente);
    assert_eq!(i.acao_padrao, Acao::Ignorar);
    assert!(i.acoes.contains(&Acao::Vincular(id)));
}

#[test]
fn mesma_corretora_soma_ticker_sem_diferenciar_maiusculas() {
    let (_d, db) = abrir();
    let p = posicao(vec![rv_b3("ITUB4", XP_B3, 100), rv_b3("itub4", XP_B3, 50)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    assert_eq!(c.itens.len(), 1);
    assert_eq!(c.itens[0].no_arquivo.as_deref(), Some("150 ações"));
    assert_eq!(c.itens[0].sugestao.as_ref().unwrap().quantidade, Some(150));
}

#[test]
fn mesma_corretora_soma_tesouro_na_sugestao() {
    let (_d, db) = abrir();
    let p = posicao(vec![tesouro_b3(XP_B3, 500_000), tesouro_b3(XP_B3, 300_000)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    assert_eq!(c.itens.len(), 1);
    let sug = c.itens[0].sugestao.as_ref().unwrap();
    assert_eq!(sug.valor_aplicado_centavos, Some(800_000));
}

#[test]
fn mesmo_ticker_em_abas_diferentes_nao_depende_da_ordem() {
    let (_d, db) = abrir();
    let etf = |q| LinhaB3::RendaVariavel {
        aba: AbaRv::Etf,
        ticker: "BOVA11".into(),
        produto: "BOVA11".into(),
        instituicao: "A".into(),
        quantidade: q,
        preco_fechamento: None,
        valor_atualizado: None,
    };
    let acao = |q| rv_b3("BOVA11", "A", q);
    let c1 = comparar(&db, &posicao(vec![etf(1), acao(2)]), hoje()).unwrap();
    assert_eq!(c1.itens.len(), 2);
    assert!(
        c1.itens
            .iter()
            .any(|i| i.no_arquivo.as_deref() == Some("1 cota"))
    );
    assert!(
        c1.itens
            .iter()
            .any(|i| i.no_arquivo.as_deref() == Some("2 ações"))
    );
}

#[test]
fn rf_ligado_por_codigo_nao_serve_a_duas_linhas() {
    let (d, db) = abrir();
    let id = criar(&db, cdb("Banco Fictício SA", "2029-03-15"));
    marcar_importado(&d, id, "X1");
    let venc = NaiveDate::from_ymd_opt(2029, 3, 15).unwrap();
    let p = posicao(vec![cdb_b3("X1", "CDB", venc), cdb_b3("X1", "CDB", venc)]);
    let c = comparar(&db, &p, hoje()).unwrap();
    let primeiro = c.itens.iter().find(|i| i.chave == "b3:0").unwrap();
    let segundo = c.itens.iter().find(|i| i.chave == "b3:1").unwrap();
    assert_eq!(primeiro.estado, Estado::Igual);
    assert_eq!(segundo.estado, Estado::Novo);
}

// ---- Aplicação ----

fn decisao(chave: &str, acao: Acao) -> Decisao {
    Decisao {
        chave: chave.to_string(),
        acao,
        dados: None,
    }
}

/// Completa a sugestão como o usuário faria na tela.
fn completar(item: &Item) -> NovoInvestimento {
    let mut n = item.sugestao.clone().expect("item sem sugestão");
    n.data_aplicacao.get_or_insert_with(|| "2025-04-01".into());
    if n.tipo.starts_with("rv-") {
        n.preco_medio_centavos = Some(3_000);
    } else {
        n.valor_aplicado_centavos.get_or_insert(2_000_000);
        let taxa = match (n.tipo.as_str(), n.indexador.as_deref()) {
            ("td-selic", _) => "0.09",
            ("td-ipca", _) => "6.5",
            (_, Some("pre")) => "12.5",
            _ => "102",
        };
        n.taxa.get_or_insert_with(|| taxa.to_string());
    }
    n
}

fn adicionar_tudo(c: &Comparacao) -> Vec<Decisao> {
    c.itens
        .iter()
        .map(|i| Decisao {
            chave: i.chave.clone(),
            acao: Acao::Adicionar,
            dados: Some(completar(i)),
        })
        .collect()
}

fn aplicar(db: &Db, c: &Comparacao, d: &[Decisao]) -> guru_core::ResultadoImportacao {
    aplicar_importacao(db, c, d, "posicao.xlsx", hoje()).unwrap()
}

fn total(db: &Db) -> usize {
    db.listar_investimentos(hoje()).unwrap().len()
}

#[test]
fn adicionar_tudo_e_comparar_de_novo_da_tudo_igual() {
    let (_d, db) = abrir();
    let c = comparar_completa(&db);
    let r = aplicar(&db, &c, &adicionar_tudo(&c));
    assert!(r.erros.is_empty(), "{:?}", r.erros);
    assert_eq!(r.aplicadas, c.itens.len());
    assert_eq!(total(&db), c.itens.len());

    let c2 = comparar_completa(&db);
    assert_eq!(c2.itens.len(), c.itens.len());
    assert!(
        c2.itens.iter().all(|i| i.estado == Estado::Igual),
        "{:?}",
        c2.itens
            .iter()
            .map(|i| (&i.ativo, i.estado))
            .collect::<Vec<_>>()
    );
    // Importar de novo não duplica.
    let r2 = aplicar(&db, &c2, &[]);
    assert_eq!(r2.aplicadas, 0);
    assert_eq!(total(&db), c.itens.len());
}

#[test]
fn codigo_b3_vem_do_ticker_do_codigo_e_do_isin() {
    let (_d, db) = abrir();
    let c = comparar_completa(&db);
    aplicar(&db, &c, &adicionar_tudo(&c));
    let conn = rusqlite::Connection::open(db.path()).unwrap();
    let codigos: Vec<(String, Option<String>)> = conn
        .prepare("SELECT tipo, codigo_b3 FROM investimentos")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(codigos.iter().all(|(_, c)| c.is_some()), "{codigos:?}");
    let de = |t: &str| -> Vec<String> {
        codigos
            .iter()
            .filter(|(tp, _)| tp.starts_with(t))
            .filter_map(|(_, c)| c.clone())
            .collect()
    };
    assert!(de("rv-").contains(&"ITUB4".to_string()));
    assert!(de("rf-").contains(&"CDB224ABCDE".to_string()));
    assert_eq!(de("td-").len(), 2);
    assert!(
        de("td-").contains(&"BRSTNCLF1RU6".to_string()),
        "{codigos:?}"
    );
}

#[test]
fn adicionar_incompleto_nao_grava_nada() {
    let (_d, db) = abrir();
    let c = comparar_completa(&db);
    let mut decisoes = adicionar_tudo(&c);
    let itub = item(&c, "ITUB4").chave.clone();
    let d = decisoes.iter_mut().find(|d| d.chave == itub).unwrap();
    d.dados.as_mut().unwrap().preco_medio_centavos = None;
    let r = aplicar(&db, &c, &decisoes);
    assert_eq!(r.aplicadas, 0);
    assert_eq!(r.erros.len(), 1);
    assert_eq!(r.erros[0].0, itub);
    assert!(
        r.erros[0].1.iter().any(|e| e.campo == "precoMedioCentavos"),
        "{:?}",
        r.erros
    );
    assert_eq!(total(&db), 0);
    assert_eq!(ultima_importacao(&db).unwrap(), None);
}

#[test]
fn usar_arquivo_na_renda_variavel_muda_so_a_quantidade() {
    let (d, db) = abrir();
    let mut novo = rv("rv-acao", "ITUB4", 120);
    novo.preco_medio_centavos = Some(3_142);
    let id = criar(&db, novo);
    marcar_importado(&d, id, "ITUB4");
    let c = comparar_completa(&db);
    let chave = item(&c, "ITUB4").chave.clone();
    let r = aplicar(&db, &c, &[decisao(&chave, Acao::UsarArquivo)]);
    assert_eq!((r.aplicadas, r.erros.len()), (1, 0));
    let inv = db.investimento(id, hoje()).unwrap();
    assert_eq!(inv.quantidade, Some(150));
    assert_eq!(inv.preco_medio_centavos, Some(3_142));
    assert_eq!(inv.valor_aplicado_centavos, 150 * 3_142);
}

#[test]
fn usar_arquivo_no_tesouro_muda_so_o_valor_aplicado() {
    let (d, db) = abrir();
    let id = criar(&db, selic(500_000));
    marcar_importado(&d, id, "BRSTNCLF1RU6");
    let c = comparar_completa(&db);
    let i = item(&c, "Tesouro Selic 2029");
    assert_eq!(i.estado, Estado::Atualizar);
    let chave = i.chave.clone();
    aplicar(&db, &c, &[decisao(&chave, Acao::UsarArquivo)]);
    let inv = db.investimento(id, hoje()).unwrap();
    assert_eq!(inv.valor_aplicado_centavos, 800_000);
    assert_eq!(inv.taxa.as_deref(), Some("0.09"));
}

#[test]
fn vincular_grava_codigo_e_a_proxima_comparacao_casa_direto() {
    let (_d, db) = abrir();
    let a = criar(&db, cdb("Banco Ficticio", "2029-03-15"));
    let _b = criar(&db, cdb("BANCO FICTICIO S.A.", "2029-03-15"));
    let c = comparar_completa(&db);
    let chave = c
        .itens
        .iter()
        .find(|i| i.estado == Estado::Divergente)
        .unwrap()
        .chave
        .clone();
    let r = aplicar(&db, &c, &[decisao(&chave, Acao::Vincular(a))]);
    assert_eq!(r.aplicadas, 1);
    assert_eq!(
        db.investimento(a, hoje()).unwrap().codigo_b3.as_deref(),
        Some("CDB224ABCDE")
    );
    let c2 = comparar_completa(&db);
    let i = c2
        .itens
        .iter()
        .find(|i| i.investimentos == vec![a])
        .unwrap();
    assert_eq!(i.estado, Estado::Igual);
}

#[test]
fn igual_na_renda_fixa_grava_o_codigo_sem_contar_como_alteracao() {
    let (_d, db) = abrir();
    let id = criar(&db, cdb("Banco Ficticio", "2029-03-15"));
    let c = comparar_completa(&db);
    let r = aplicar(&db, &c, &[]);
    assert_eq!(r.aplicadas, 0);
    assert_eq!(
        db.investimento(id, hoje()).unwrap().codigo_b3.as_deref(),
        Some("CDB224ABCDE")
    );
}

#[test]
fn divergente_mantido_nao_ganha_codigo() {
    let (_d, db) = abrir();
    let id = criar(&db, rv("rv-acao", "ITUB4", 120));
    let c = comparar_completa(&db);
    let chave = item(&c, "ITUB4").chave.clone();
    aplicar(&db, &c, &[decisao(&chave, Acao::Manter)]);
    assert_eq!(db.investimento(id, hoje()).unwrap().codigo_b3, None);
}

#[test]
fn acao_fora_da_lista_ou_chave_desconhecida_e_decisao_invalida() {
    let (_d, db) = abrir();
    let c = comparar_completa(&db);
    let chave = item(&c, "ITUB4").chave.clone();
    for d in [
        decisao(&chave, Acao::UsarArquivo),
        decisao("b3:999", Acao::Manter),
    ] {
        let e = aplicar_importacao(&db, &c, &[d], "x.xlsx", hoje()).unwrap_err();
        assert_eq!(e.code(), "DECISAO_INVALIDA");
    }
    let duas = [
        decisao(&chave, Acao::Ignorar),
        decisao(&chave, Acao::Ignorar),
    ];
    let e = aplicar_importacao(&db, &c, &duas, "x.xlsx", hoje()).unwrap_err();
    assert_eq!(e.code(), "DECISAO_INVALIDA");
    assert_eq!(total(&db), 0);
}

#[test]
fn ultima_importacao_devolve_a_data() {
    let (_d, db) = abrir();
    assert_eq!(ultima_importacao(&db).unwrap(), None);
    let c = comparar_completa(&db);
    aplicar(&db, &c, &[]);
    assert_eq!(
        ultima_importacao(&db).unwrap().as_deref(),
        Some("2026-10-30")
    );
}

// ---- Planilhas hostis ----

/// Monta um `.xlsx` mínimo com abas de células `str` (texto) ou numéricas (prefixo `#`).
fn xlsx(abas: &[(&str, &[&[&str]])]) -> Vec<u8> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let mut wb = String::from(r#"<workbook xmlns:r="x"><sheets>"#);
    let mut rels = String::from("<Relationships>");
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opcoes = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (n, (nome, linhas)) in abas.iter().enumerate() {
        let i = n + 1;
        wb += &format!(r#"<sheet name="{nome}" sheetId="{i}" r:id="rId{i}"/>"#);
        rels += &format!(
            r#"<Relationship Id="rId{i}" Type="x/worksheet" Target="worksheets/s{i}.xml"/>"#
        );
        let mut xml = String::from("<worksheet><sheetData>");
        for linha in *linhas {
            xml += "<row>";
            for c in *linha {
                match c.strip_prefix('#') {
                    Some(num) => xml += &format!("<c><v>{num}</v></c>"),
                    None => xml += &format!(r#"<c t="str"><v>{c}</v></c>"#),
                }
            }
            xml += "</row>";
        }
        xml += "</sheetData></worksheet>";
        z.start_file(format!("xl/worksheets/s{i}.xml"), opcoes)
            .unwrap();
        z.write_all(xml.as_bytes()).unwrap();
    }
    wb += "</sheets></workbook>";
    rels += "</Relationships>";
    z.start_file("xl/workbook.xml", opcoes).unwrap();
    z.write_all(wb.as_bytes()).unwrap();
    z.start_file("xl/_rels/workbook.xml.rels", opcoes).unwrap();
    z.write_all(rels.as_bytes()).unwrap();
    z.finish().unwrap().into_inner()
}

const ENORME: &str = "#79228162514264337593543950335";

#[test]
fn valor_enorme_vira_aviso_e_nao_derruba_a_comparacao() {
    let bytes = xlsx(&[
        (
            "Ações",
            &[
                &[
                    "Produto",
                    "Instituição",
                    "Código de Negociação",
                    "Quantidade",
                ],
                &["ITUB4 - ITAU UNIBANCO", "Rico", "ITUB4", "#150"],
            ],
        ),
        (
            "Tesouro Direto",
            &[
                &["Produto", "Instituição", "Vencimento", "Valor Aplicado"],
                &["Tesouro Selic 2029", "Rico", "01/03/2029", ENORME],
            ],
        ),
        (
            "Renda Fixa",
            &[
                &[
                    "Produto",
                    "Instituição",
                    "Código",
                    "Vencimento",
                    "Quantidade",
                    "Valor Atualizado CURVA",
                ],
                &[
                    "CDB - BANCO ENORME S/A",
                    "Rico",
                    "CDB9",
                    "15/03/2029",
                    "#1",
                    ENORME,
                ],
            ],
        ),
    ]);
    let (_d, db) = abrir();
    criar(&db, selic(800_000));
    let posicao = ler_posicao(&bytes).unwrap();
    let c = comparar(&db, &posicao, hoje()).unwrap();
    assert!(
        c.avisos
            .contains(&"Valor fora do limite em Tesouro Selic 2029.".to_string()),
        "{:?}",
        c.avisos
    );
    assert!(
        c.avisos
            .contains(&"Valor fora do limite em CDB - BANCO ENORME S/A.".to_string()),
        "{:?}",
        c.avisos
    );
    // As linhas fora do limite não entram: o Tesouro cadastrado nem aparece.
    let ativos: Vec<(&str, Estado)> = c
        .itens
        .iter()
        .map(|i| (i.ativo.as_str(), i.estado))
        .collect();
    assert_eq!(ativos.len(), 1, "{ativos:?}");
    assert!(ativos.contains(&("ITUB4", Estado::Novo)), "{ativos:?}");
}

// ---- Renda fixa ambígua ----

fn cdb_taxa(taxa: &str) -> NovoInvestimento {
    NovoInvestimento {
        taxa: s(taxa),
        ..cdb("Banco Fictício S/A", "2029-03-15")
    }
}

fn venc_2029() -> NaiveDate {
    NaiveDate::from_ymd_opt(2029, 3, 15).unwrap()
}

#[test]
fn rf_ambigua_oferece_os_dois_candidatos_as_duas_linhas() {
    let (_d, db) = abrir();
    let a = criar(&db, cdb_taxa("102"));
    let b = criar(&db, cdb_taxa("110"));
    let p = posicao(vec![
        cdb_b3("CDB1", "CDB", venc_2029()),
        cdb_b3("CDB2", "CDB", venc_2029()),
    ]);
    let c = comparar(&db, &p, hoje()).unwrap();
    for chave in ["b3:0", "b3:1"] {
        let i = c.itens.iter().find(|i| i.chave == chave).unwrap();
        assert_eq!(i.estado, Estado::Divergente, "{chave}");
        assert!(
            i.acoes.contains(&Acao::Vincular(a)),
            "{chave}: {:?}",
            i.acoes
        );
        assert!(
            i.acoes.contains(&Acao::Vincular(b)),
            "{chave}: {:?}",
            i.acoes
        );
    }

    // Vincular uma linha a cada um funciona e a próxima comparação casa pelo código.
    let r = aplicar(
        &db,
        &c,
        &[
            decisao("b3:0", Acao::Vincular(a)),
            decisao("b3:1", Acao::Vincular(b)),
        ],
    );
    assert_eq!((r.aplicadas, r.erros.len()), (2, 0));
    assert_eq!(
        db.investimento(a, hoje()).unwrap().codigo_b3.as_deref(),
        Some("CDB1")
    );
    assert_eq!(
        db.investimento(b, hoje()).unwrap().codigo_b3.as_deref(),
        Some("CDB2")
    );
    let c2 = comparar(&db, &p, hoje()).unwrap();
    assert!(
        c2.itens.iter().all(|i| i.estado == Estado::Igual),
        "{:?}",
        c2.itens
    );
}

#[test]
fn rf_ambigua_vincular_o_mesmo_investimento_duas_vezes_e_decisao_invalida() {
    let (_d, db) = abrir();
    let a = criar(&db, cdb_taxa("102"));
    let _b = criar(&db, cdb_taxa("110"));
    let p = posicao(vec![
        cdb_b3("CDB1", "CDB", venc_2029()),
        cdb_b3("CDB2", "CDB", venc_2029()),
    ]);
    let c = comparar(&db, &p, hoje()).unwrap();
    let duas = [
        decisao("b3:0", Acao::Vincular(a)),
        decisao("b3:1", Acao::Vincular(a)),
    ];
    let e = aplicar_importacao(&db, &c, &duas, "x.xlsx", hoje()).unwrap_err();
    assert_eq!(e.code(), "DECISAO_INVALIDA");
    assert_eq!(db.investimento(a, hoje()).unwrap().codigo_b3, None);
}

#[test]
fn vincular_o_que_outra_linha_casou_de_forma_unica_e_decisao_invalida() {
    let (_d, db) = abrir();
    let a = criar(&db, cdb_taxa("102"));
    let mut lci = cdb_taxa("95");
    lci.tipo = "rf-lci".into();
    lci.ir = s("isento");
    let b = criar(&db, lci);
    // A linha de prefixo desconhecido vê os dois; a de CDB, depois, casa só com o CDB.
    let p = posicao(vec![
        cdb_b3("X1", "XYZ", venc_2029()),
        cdb_b3("CDB1", "CDB", venc_2029()),
    ]);
    let c = comparar(&db, &p, hoje()).unwrap();
    let ambigua = c.itens.iter().find(|i| i.chave == "b3:0").unwrap();
    assert!(ambigua.acoes.contains(&Acao::Vincular(a)));
    assert!(ambigua.acoes.contains(&Acao::Vincular(b)));
    let unica = c.itens.iter().find(|i| i.chave == "b3:1").unwrap();
    assert_eq!(
        (unica.estado, unica.investimentos.clone()),
        (Estado::Igual, vec![a])
    );

    let e = aplicar_importacao(
        &db,
        &c,
        &[decisao("b3:0", Acao::Vincular(a))],
        "x.xlsx",
        hoje(),
    )
    .unwrap_err();
    assert_eq!(e.code(), "DECISAO_INVALIDA");
    let r = aplicar(&db, &c, &[decisao("b3:0", Acao::Vincular(b))]);
    assert_eq!((r.aplicadas, r.erros.len()), (1, 0));
    assert_eq!(
        db.investimento(b, hoje()).unwrap().codigo_b3.as_deref(),
        Some("X1")
    );
}

// ---- Itens menores ----

#[test]
fn tesouro_sem_valor_aplicado_nao_oferece_usar_arquivo() {
    let sem_valor = || match tesouro_b3("Corretora Fictícia", 0) {
        LinhaB3::Tesouro {
            produto,
            tipo,
            isin,
            instituicao,
            vencimento,
            ..
        } => LinhaB3::Tesouro {
            produto,
            tipo,
            isin,
            instituicao,
            vencimento,
            valor_aplicado: None,
            valor_atualizado: None,
        },
        _ => unreachable!(),
    };
    // Sem código: divergente, só Manter.
    let (d, db) = abrir();
    let id = criar(&db, selic(800_000));
    let c = comparar(&db, &posicao(vec![sem_valor()]), hoje()).unwrap();
    let i = &c.itens[0];
    assert_eq!(
        (i.acoes.clone(), i.acao_padrao),
        (vec![Acao::Manter], Acao::Manter)
    );
    // Já importado: também não vira Atualizar com UsarArquivo como padrão.
    marcar_importado(&d, id, "td-selic:2029-03-01");
    let c = comparar(&db, &posicao(vec![sem_valor()]), hoje()).unwrap();
    let i = &c.itens[0];
    assert!(!i.acoes.contains(&Acao::UsarArquivo), "{:?}", i.acoes);
    assert_eq!(i.acao_padrao, Acao::Manter);
}

#[test]
fn usar_arquivo_sem_preco_medio_ou_sem_quantidade_pede_o_campo() {
    let (d, db) = abrir();
    let id = criar(&db, rv("rv-acao", "ITUB4", 120));
    marcar_importado(&d, id, "ITUB4");
    let conn = rusqlite::Connection::open(d.path().join("guru.db")).unwrap();
    let caso = |sql: &str| {
        conn.execute(sql, rusqlite::params![id]).unwrap();
        let c = comparar_completa(&db);
        let chave = item(&c, "ITUB4").chave.clone();
        let r = aplicar(&db, &c, &[decisao(&chave, Acao::UsarArquivo)]);
        assert_eq!(r.aplicadas, 0);
        r.erros[0].1.clone()
    };
    let e = caso("UPDATE investimentos SET preco_medio_centavos = NULL WHERE id = ?1");
    assert_eq!(
        (e[0].campo.as_str(), e[0].mensagem.as_str()),
        ("precoMedioCentavos", "Informe o preço médio.")
    );
    let e = caso(
        "UPDATE investimentos SET preco_medio_centavos = 3000, quantidade = NULL WHERE id = ?1",
    );
    assert_eq!(
        (e[0].campo.as_str(), e[0].mensagem.as_str()),
        ("quantidade", "Informe a quantidade.")
    );
}

#[test]
fn aplicar_espera_a_escrita_concorrente_em_vez_de_falhar() {
    use std::sync::mpsc;
    let (d, db) = abrir();
    let id = criar(&db, rv("rv-acao", "ITUB4", 120));
    marcar_importado(&d, id, "ITUB4");
    let c = comparar_completa(&db);
    let chave = item(&c, "ITUB4").chave.clone();

    // Outra conexão (a atualização de mercado) segura a escrita e grava logo depois.
    let caminho = d.path().join("guru.db");
    let (pronto, esperar) = mpsc::channel();
    let outra = std::thread::spawn(move || {
        let conn = rusqlite::Connection::open(caminho).unwrap();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        pronto.send(()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        conn.execute_batch("UPDATE investimentos SET observacao = 'mercado'; COMMIT")
            .unwrap();
    });
    esperar.recv().unwrap();
    let r = aplicar_importacao(
        &db,
        &c,
        &[decisao(&chave, Acao::UsarArquivo)],
        "p.xlsx",
        hoje(),
    );
    outra.join().unwrap();
    let r = r.expect("a aplicação deve esperar a outra escrita terminar");
    assert_eq!((r.aplicadas, r.erros.len()), (1, 0));
    assert_eq!(db.investimento(id, hoje()).unwrap().quantidade, Some(150));
}
