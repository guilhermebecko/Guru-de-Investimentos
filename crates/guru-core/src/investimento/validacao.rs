use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromStr;

use super::catalogo::{Benchmark, Forma, Indexador, Ir, Liquidez, TipoInfo, tipo};
use super::cnpj::normalizar_cnpj;
use super::modelo::{ErroCampo, InvestimentoValido, NovoInvestimento};

fn erro(erros: &mut Vec<ErroCampo>, campo: &str, mensagem: &str) {
    erros.push(ErroCampo {
        campo: campo.to_string(),
        mensagem: mensagem.to_string(),
    });
}

/// `trim`; texto vazio vira `None`.
fn texto(valor: &Option<String>) -> Option<String> {
    valor
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Data `YYYY-MM-DD` estrita (com zeros).
fn data(valor: &Option<String>) -> Option<NaiveDate> {
    let t = texto(valor)?;
    if t.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(&t, "%Y-%m-%d").ok()
}

/// Maior valor em centavos aceito (2^53 − 1): o limite em que o JS ainda é exato.
pub const MAX_CENTAVOS: i64 = 9_007_199_254_740_991;

/// `^[A-Z][A-Z0-9]{3}[0-9]{1,2}$`: cobre `ITUB4`, `B3SA3` e BDRs como `M1TA34`.
fn ticker_valido(t: &str) -> bool {
    let b = t.as_bytes();
    (5..=6).contains(&b.len())
        && b[0].is_ascii_uppercase()
        && b[1..4]
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && b[4..].iter().all(u8::is_ascii_digit)
}

fn validar_instituicao(novo: &NovoInvestimento, erros: &mut Vec<ErroCampo>) -> String {
    let inst = novo.instituicao.trim();
    if inst.is_empty() || inst.chars().count() > 60 {
        erro(erros, "instituicao", "Informe a corretora ou o banco.");
    }
    inst.to_string()
}

fn validar_observacao(novo: &NovoInvestimento, erros: &mut Vec<ErroCampo>) -> Option<String> {
    let obs = texto(&novo.observacao);
    if obs.as_ref().is_some_and(|o| o.chars().count() > 200) {
        erro(erros, "observacao", "Use até 200 caracteres.");
    }
    obs
}

fn validar_cnpj(
    novo: &NovoInvestimento,
    info: &TipoInfo,
    erros: &mut Vec<ErroCampo>,
) -> Option<String> {
    if info.codigo != "fundo" {
        return None;
    }
    let informado = texto(&novo.cnpj)?;
    let cnpj = normalizar_cnpj(&informado);
    if cnpj.is_none() {
        erro(erros, "cnpj", "CNPJ inválido. Confira os 14 dígitos.");
    }
    cnpj
}

fn validar_data_aplicacao(
    novo: &NovoInvestimento,
    hoje: NaiveDate,
    erros: &mut Vec<ErroCampo>,
) -> Option<NaiveDate> {
    match data(&novo.data_aplicacao) {
        None => {
            erro(erros, "dataAplicacao", "Informe a data da aplicação.");
            None
        }
        Some(d) if d > hoje => {
            erro(
                erros,
                "dataAplicacao",
                "A data da aplicação não pode estar no futuro.",
            );
            None
        }
        Some(d) => Some(d),
    }
}

fn validar_emissor(
    novo: &NovoInvestimento,
    info: &TipoInfo,
    erros: &mut Vec<ErroCampo>,
) -> Option<String> {
    if !info.exige_emissor {
        return None;
    }
    let emissor = texto(&novo.emissor);
    if emissor.as_ref().is_none_or(|e| e.chars().count() > 80) {
        let msg = if info.forma == Forma::Valor {
            "Informe o nome."
        } else {
            "Informe o emissor."
        };
        erro(erros, "emissor", msg);
    }
    emissor
}

fn validar_indexador(
    novo: &NovoInvestimento,
    info: &TipoInfo,
    erros: &mut Vec<ErroCampo>,
) -> Option<Indexador> {
    if let Some(forcado) = info.indexador_forcado {
        return Some(forcado);
    }
    let escolhido = texto(&novo.indexador)
        .and_then(|c| Indexador::de_codigo(&c))
        .filter(|i| *i != Indexador::Selic);
    if escolhido.is_none() {
        erro(erros, "indexador", "Escolha o indexador.");
    }
    escolhido
}

fn validar_taxa(
    novo: &NovoInvestimento,
    indexador: Option<Indexador>,
    erros: &mut Vec<ErroCampo>,
) -> Option<Decimal> {
    let taxa = texto(&novo.taxa).and_then(|t| Decimal::from_str(&t).ok());
    let dentro = match (taxa, indexador) {
        (Some(t), Some(Indexador::Pos)) => t > Decimal::ZERO && t <= Decimal::from(300),
        (Some(t), Some(Indexador::Pre | Indexador::Ipca)) => {
            t > Decimal::ZERO && t <= Decimal::from(100)
        }
        (Some(t), Some(Indexador::Selic)) => t >= Decimal::ZERO && t <= Decimal::from(10),
        (Some(_), None) => true,
        (None, _) => false,
    };
    if !dentro {
        erro(erros, "taxa", "Informe uma taxa válida.");
        return None;
    }
    taxa
}

fn validar_benchmark(
    novo: &NovoInvestimento,
    indexador: Option<Indexador>,
    erros: &mut Vec<ErroCampo>,
) -> Option<Benchmark> {
    let indexador = indexador?;
    let informado = texto(&novo.benchmark);
    let benchmark = informado.as_deref().and_then(Benchmark::de_codigo);
    let ok = if indexador == Indexador::Pos {
        benchmark.is_some()
    } else {
        informado.is_none()
    };
    if !ok {
        let msg = if indexador == Indexador::Pos {
            "Escolha CDI ou Selic."
        } else {
            "O benchmark só vale para o indexador pós-fixado."
        };
        erro(erros, "benchmark", msg);
    }
    benchmark.filter(|_| indexador == Indexador::Pos)
}

fn validar_liquidez(
    novo: &NovoInvestimento,
    info: &TipoInfo,
    erros: &mut Vec<ErroCampo>,
) -> Option<Liquidez> {
    if let Some(forcada) = info.liquidez_forcada {
        return Some(forcada);
    }
    let liquidez = texto(&novo.liquidez).and_then(|c| Liquidez::de_codigo(&c));
    if liquidez.is_none() {
        erro(erros, "liquidez", "Escolha a liquidez.");
    }
    liquidez
}

fn validar_ir(novo: &NovoInvestimento, info: &TipoInfo, erros: &mut Vec<ErroCampo>) -> Option<Ir> {
    let Some(ir) = texto(&novo.ir).and_then(|c| Ir::de_codigo(&c)) else {
        erro(erros, "ir", "Escolha o IR.");
        return None;
    };
    if !info.ir_editavel && Some(ir) != info.ir_padrao {
        erro(erros, "ir", "O IR deste tipo é definido por lei.");
        return None;
    }
    Some(ir)
}

fn validar_vencimento(
    novo: &NovoInvestimento,
    exige: bool,
    aplicacao: Option<NaiveDate>,
    erros: &mut Vec<ErroCampo>,
) -> Option<NaiveDate> {
    let informado = texto(&novo.vencimento).is_some();
    let venc = data(&novo.vencimento);
    if venc.is_none() {
        if exige || informado {
            erro(erros, "vencimento", "Informe o vencimento.");
        }
        return None;
    }
    if aplicacao.is_some_and(|a| venc.is_some_and(|v| v <= a)) {
        erro(
            erros,
            "vencimento",
            "O vencimento precisa ser depois da aplicação.",
        );
    }
    venc
}

fn validar_carencia(
    novo: &NovoInvestimento,
    aplicacao: Option<NaiveDate>,
    vencimento: Option<NaiveDate>,
    erros: &mut Vec<ErroCampo>,
) -> Option<NaiveDate> {
    let carencia = data(&novo.data_carencia);
    let ok = carencia
        .is_some_and(|c| aplicacao.is_none_or(|a| c > a) && vencimento.is_none_or(|v| c <= v));
    if !ok {
        erro(
            erros,
            "dataCarencia",
            "Informe uma data de carência entre a aplicação e o vencimento.",
        );
    }
    carencia
}

fn positivo(
    valor: Option<i64>,
    maximo: i64,
    campo: &str,
    mensagem: &str,
    erros: &mut Vec<ErroCampo>,
) -> Option<i64> {
    match valor {
        Some(v) if v > 0 && v <= maximo => Some(v),
        _ => {
            erro(erros, campo, mensagem);
            None
        }
    }
}

pub fn validar(
    novo: &NovoInvestimento,
    hoje: NaiveDate,
) -> Result<InvestimentoValido, Vec<ErroCampo>> {
    let mut erros = Vec::new();
    let instituicao = validar_instituicao(novo, &mut erros);
    let observacao = validar_observacao(novo, &mut erros);
    let aplicacao = validar_data_aplicacao(novo, hoje, &mut erros);

    let Some(info) = tipo(novo.tipo.trim()) else {
        erro(&mut erros, "tipo", "Escolha um tipo da lista.");
        return Err(erros);
    };

    let emissor = validar_emissor(novo, info, &mut erros);
    let cnpj = validar_cnpj(novo, info, &mut erros);
    let mut ticker = None;
    let mut quantidade = None;
    let mut preco_medio = None;
    let mut valor_aplicado = None;
    let mut indexador = None;
    let mut taxa = None;
    let mut benchmark = None;
    let mut liquidez = None;
    let mut ir = None;
    let mut vencimento = None;
    let mut carencia = None;

    if info.forma == Forma::RendaVariavel {
        let t = texto(&novo.ticker).map(|t| t.to_uppercase());
        if t.as_deref().is_none_or(|t| !ticker_valido(t)) {
            erro(
                &mut erros,
                "ticker",
                "Use o código da B3, por exemplo ITUB4.",
            );
        }
        ticker = t;
        quantidade = positivo(
            novo.quantidade,
            i64::MAX,
            "quantidade",
            "Informe a quantidade.",
            &mut erros,
        );
        preco_medio = positivo(
            novo.preco_medio_centavos,
            MAX_CENTAVOS,
            "precoMedioCentavos",
            "Informe o preço médio.",
            &mut erros,
        );
        if let (Some(q), Some(p)) = (quantidade, preco_medio)
            && q.checked_mul(p).is_none_or(|t| t > MAX_CENTAVOS)
        {
            erro(&mut erros, "precoMedioCentavos", "Informe o preço médio.");
            preco_medio = None;
        }
    } else {
        valor_aplicado = positivo(
            novo.valor_aplicado_centavos,
            MAX_CENTAVOS,
            "valorAplicadoCentavos",
            "Informe o valor aplicado.",
            &mut erros,
        );
        liquidez = validar_liquidez(novo, info, &mut erros);
        if info.forma == Forma::RendaFixa {
            indexador = validar_indexador(novo, info, &mut erros);
            taxa = validar_taxa(novo, indexador, &mut erros);
            benchmark = validar_benchmark(novo, indexador, &mut erros);
            ir = validar_ir(novo, info, &mut erros);
        }
        let exige_vencimento = info.e_tesouro() || liquidez == Some(Liquidez::Vencimento);
        vencimento = validar_vencimento(novo, exige_vencimento, aplicacao, &mut erros);
        if liquidez == Some(Liquidez::Carencia) {
            carencia = validar_carencia(novo, aplicacao, vencimento, &mut erros);
        }
    }

    match aplicacao {
        Some(data_aplicacao) if erros.is_empty() => Ok(InvestimentoValido {
            tipo: info,
            instituicao,
            emissor,
            ticker,
            indexador,
            taxa,
            benchmark,
            data_aplicacao,
            vencimento,
            valor_aplicado_centavos: valor_aplicado,
            quantidade,
            preco_medio_centavos: preco_medio,
            liquidez,
            data_carencia: carencia,
            ir,
            observacao,
            cnpj,
        }),
        _ => Err(erros),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use chrono::NaiveDate;

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

    fn acao() -> NovoInvestimento {
        NovoInvestimento {
            tipo: "rv-acao".into(),
            instituicao: "Rico".into(),
            ticker: s("ITUB4"),
            data_aplicacao: s("2025-03-15"),
            quantidade: Some(10),
            preco_medio_centavos: Some(3_000),
            ..Default::default()
        }
    }

    fn campos(r: Result<InvestimentoValido, Vec<ErroCampo>>) -> Vec<String> {
        r.unwrap_err().into_iter().map(|e| e.campo).collect()
    }

    fn mensagem(r: Result<InvestimentoValido, Vec<ErroCampo>>, campo: &str) -> String {
        r.unwrap_err()
            .into_iter()
            .find(|e| e.campo == campo)
            .unwrap()
            .mensagem
    }

    #[test]
    fn cdb_valido_passa() {
        let v = validar(&cdb(), hoje()).unwrap();
        assert_eq!(v.tipo.codigo, "rf-cdb");
        assert_eq!(v.taxa.unwrap().to_string(), "102");
        assert_eq!(v.vencimento, NaiveDate::from_ymd_opt(2028, 3, 15));
    }

    #[test]
    fn junta_todos_os_erros() {
        let novo = NovoInvestimento {
            tipo: "rf-cdb".into(),
            ..Default::default()
        };
        let c = campos(validar(&novo, hoje()));
        for esperado in [
            "instituicao",
            "emissor",
            "indexador",
            "taxa",
            "dataAplicacao",
            "valorAplicadoCentavos",
            "liquidez",
            "ir",
        ] {
            assert!(c.iter().any(|x| x == esperado), "faltou {esperado}: {c:?}");
        }
    }

    #[test]
    fn tipo_invalido_e_recusado() {
        let mut n = cdb();
        n.tipo = "xyz".into();
        let r = validar(&n, hoje());
        assert_eq!(mensagem(r, "tipo"), "Escolha um tipo da lista.");
    }

    #[test]
    fn lci_nao_aceita_ir_regressivo() {
        let mut n = cdb();
        n.tipo = "rf-lci".into();
        assert_eq!(
            mensagem(validar(&n, hoje()), "ir"),
            "O IR deste tipo é definido por lei."
        );
    }

    #[test]
    fn debenture_aceita_isento() {
        let mut n = cdb();
        n.tipo = "rf-deb".into();
        n.ir = s("isento");
        assert!(validar(&n, hoje()).is_ok());
    }

    #[test]
    fn benchmark_so_com_pos() {
        let mut n = cdb();
        n.indexador = s("pre");
        n.taxa = s("12.5");
        assert_eq!(campos(validar(&n, hoje())), vec!["benchmark"]);
        let mut n = cdb();
        n.benchmark = None;
        assert_eq!(
            mensagem(validar(&n, hoje()), "benchmark"),
            "Escolha CDI ou Selic."
        );
    }

    #[test]
    fn data_futura_recusada() {
        let mut n = cdb();
        n.data_aplicacao = s("2026-10-29");
        assert_eq!(
            mensagem(validar(&n, hoje()), "dataAplicacao"),
            "A data da aplicação não pode estar no futuro."
        );
        n.data_aplicacao = s("2026-10-28");
        assert!(validar(&n, hoje()).is_ok());
    }

    #[test]
    fn vencimento_antes_da_aplicacao_recusado() {
        let mut n = cdb();
        n.vencimento = s("2025-03-15");
        assert_eq!(
            mensagem(validar(&n, hoje()), "vencimento"),
            "O vencimento precisa ser depois da aplicação."
        );
    }

    #[test]
    fn tesouro_sem_vencimento_recusado() {
        let mut n = cdb();
        n.tipo = "td-pre".into();
        n.emissor = None;
        n.indexador = s("pre");
        n.benchmark = None;
        n.liquidez = s("d1");
        n.vencimento = None;
        assert_eq!(
            mensagem(validar(&n, hoje()), "vencimento"),
            "Informe o vencimento."
        );
    }

    #[test]
    fn tesouro_aplica_indexador_e_liquidez_forcados() {
        let mut n = cdb();
        n.tipo = "td-selic".into();
        n.emissor = None;
        n.indexador = None;
        n.benchmark = None;
        n.taxa = s("0.0905");
        n.liquidez = None;
        let v = validar(&n, hoje()).unwrap();
        assert_eq!(v.indexador, Some(Indexador::Selic));
        assert_eq!(v.liquidez, Some(Liquidez::D1));
        assert_eq!(v.emissor, None);
    }

    #[test]
    fn carencia_exige_data_no_intervalo() {
        let mut n = cdb();
        n.liquidez = s("carencia");
        assert_eq!(
            mensagem(validar(&n, hoje()), "dataCarencia"),
            "Informe uma data de carência entre a aplicação e o vencimento."
        );
        n.data_carencia = s("2029-01-01");
        assert!(campos(validar(&n, hoje())).contains(&"dataCarencia".to_string()));
        n.data_carencia = s("2027-01-01");
        assert!(validar(&n, hoje()).is_ok());
    }

    #[test]
    fn ticker_normalizado_e_validado() {
        let mut n = acao();
        n.ticker = s(" itub4 ");
        assert_eq!(
            validar(&n, hoje()).unwrap().ticker.as_deref(),
            Some("ITUB4")
        );
        n.ticker = s("b3sa3");
        assert_eq!(
            validar(&n, hoje()).unwrap().ticker.as_deref(),
            Some("B3SA3")
        );
        n.ticker = s("M1TA34");
        assert!(validar(&n, hoje()).is_ok());
        n.ticker = s("1TUB4");
        assert_eq!(campos(validar(&n, hoje())), vec!["ticker"]);
        n.ticker = s("PETR");
        assert_eq!(
            mensagem(validar(&n, hoje()), "ticker"),
            "Use o código da B3, por exemplo ITUB4."
        );
    }

    #[test]
    fn campos_de_outra_forma_sao_descartados() {
        let mut n = cdb();
        n.ticker = s("ITUB4");
        n.quantidade = Some(10);
        n.preco_medio_centavos = Some(5);
        let v = validar(&n, hoje()).unwrap();
        assert_eq!(v.ticker, None);
        assert_eq!(v.quantidade, None);
        assert_eq!(v.preco_medio_centavos, None);

        let mut a = acao();
        a.taxa = s("10");
        a.valor_aplicado_centavos = Some(1);
        a.ir = s("isento");
        a.liquidez = s("diaria");
        let v = validar(&a, hoje()).unwrap();
        assert_eq!(v.taxa, None);
        assert_eq!(v.valor_aplicado_centavos, None);
        assert_eq!(v.ir, None);
        assert_eq!(v.liquidez, None);
    }

    #[test]
    fn estouro_de_quantidade_vezes_preco_recusado() {
        let mut n = acao();
        n.quantidade = Some(i64::MAX);
        n.preco_medio_centavos = Some(2);
        assert_eq!(campos(validar(&n, hoje())), vec!["precoMedioCentavos"]);
    }

    #[test]
    fn valores_acima_do_teto_de_centavos_recusados() {
        let mut n = cdb();
        n.valor_aplicado_centavos = Some(MAX_CENTAVOS);
        assert!(validar(&n, hoje()).is_ok());
        n.valor_aplicado_centavos = Some(MAX_CENTAVOS + 1);
        assert_eq!(
            mensagem(validar(&n, hoje()), "valorAplicadoCentavos"),
            "Informe o valor aplicado."
        );
        let mut a = acao();
        a.quantidade = Some(1);
        a.preco_medio_centavos = Some(MAX_CENTAVOS);
        assert!(validar(&a, hoje()).is_ok());
        a.quantidade = Some(2);
        assert_eq!(
            mensagem(validar(&a, hoje()), "precoMedioCentavos"),
            "Informe o preço médio."
        );
        a.quantidade = Some(1);
        a.preco_medio_centavos = Some(MAX_CENTAVOS + 1);
        assert_eq!(campos(validar(&a, hoje())), vec!["precoMedioCentavos"]);
    }

    #[test]
    fn taxa_com_virgula_recusada() {
        let mut n = cdb();
        n.taxa = s("102,5");
        assert_eq!(campos(validar(&n, hoje())), vec!["taxa"]);
    }

    #[test]
    fn limites_de_taxa_por_indexador() {
        let mut n = cdb();
        n.taxa = s("300");
        assert!(validar(&n, hoje()).is_ok());
        n.taxa = s("300.01");
        assert!(validar(&n, hoje()).is_err());
        n.taxa = s("0");
        assert!(validar(&n, hoje()).is_err());
        let mut p = cdb();
        p.indexador = s("pre");
        p.benchmark = None;
        p.taxa = s("100.5");
        assert_eq!(campos(validar(&p, hoje())), vec!["taxa"]);
    }

    #[test]
    fn observacao_e_textos_com_limite_e_trim() {
        let mut n = cdb();
        n.observacao = Some("a".repeat(201));
        assert_eq!(
            mensagem(validar(&n, hoje()), "observacao"),
            "Use até 200 caracteres."
        );
        n.observacao = Some("  ".into());
        assert_eq!(validar(&n, hoje()).unwrap().observacao, None);
        n.observacao = None;
        n.instituicao = "  Banco Inter  ".into();
        assert_eq!(validar(&n, hoje()).unwrap().instituicao, "Banco Inter");
    }

    #[test]
    fn valor_zero_e_quantidade_zero_recusados() {
        let mut n = cdb();
        n.valor_aplicado_centavos = Some(0);
        assert_eq!(campos(validar(&n, hoje())), vec!["valorAplicadoCentavos"]);
        let mut a = acao();
        a.quantidade = Some(0);
        assert_eq!(campos(validar(&a, hoje())), vec!["quantidade"]);
    }

    fn fundo(cnpj: Option<&str>) -> NovoInvestimento {
        NovoInvestimento {
            tipo: "fundo".into(),
            instituicao: "BTG Pactual".into(),
            emissor: s("Fundo Teste"),
            cnpj: cnpj.map(str::to_string),
            data_aplicacao: s("2025-03-15"),
            valor_aplicado_centavos: Some(1_000_000),
            liquidez: s("d30"),
            ..Default::default()
        }
    }

    #[test]
    fn cnpj_do_fundo_vazio_ou_ausente_e_none() {
        assert_eq!(validar(&fundo(None), hoje()).unwrap().cnpj, None);
        assert_eq!(validar(&fundo(Some("  ")), hoje()).unwrap().cnpj, None);
    }

    #[test]
    fn cnpj_do_fundo_valido_vira_14_digitos() {
        let v = validar(&fundo(Some("11.222.333/0001-81")), hoje()).unwrap();
        assert_eq!(v.cnpj.as_deref(), Some("11222333000181"));
    }

    #[test]
    fn cnpj_do_fundo_invalido_marca_o_campo() {
        let r = validar(&fundo(Some("123")), hoje());
        assert_eq!(mensagem(r, "cnpj"), "CNPJ inválido. Confira os 14 dígitos.");
    }

    #[test]
    fn cnpj_em_outro_tipo_e_ignorado() {
        let mut novo = cdb();
        novo.cnpj = s("11.222.333/0001-81");
        assert_eq!(validar(&novo, hoje()).unwrap().cnpj, None);
        novo.cnpj = s("123");
        assert!(validar(&novo, hoje()).is_ok());
    }
}
