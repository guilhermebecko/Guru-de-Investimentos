//! Valor atual bruto (antes do IR e de taxas) de um investimento, pelas fontes oficiais
//! guardadas. Regras em "Regras de valorização" do plano da fase 3a.

use std::str::FromStr;

use chrono::{Datelike, Days, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal::prelude::ToPrimitive;

use super::{PontoDiario, titulo_tesouro};
use crate::db::Db;
use crate::error::CoreError;

use crate::investimento::{Benchmark, Indexador, Investimento, normalizar_cnpj};

/// Dias úteis por ano da convenção dos prefixados.
const DIAS_UTEIS_ANO: i64 = 252;
/// Dias depois do fim de um mês em que o IPCA dele ainda pode não ter sido publicado.
const PRAZO_IPCA_DIAS: u64 = 45;
/// Quanto depois da aplicação a série guardada pode começar e ainda cobri-la.
const FOLGA_COBERTURA_DIAS: u64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origem {
    Calculo,
    Tesouro,
    Cotacao,
    Extrato,
    Cota,
    SemFonte,
    SemDados,
}

impl Origem {
    pub fn codigo(self) -> &'static str {
        match self {
            Origem::Calculo => "calculo",
            Origem::Tesouro => "tesouro",
            Origem::Cotacao => "cotacao",
            Origem::Extrato => "extrato",
            Origem::Cota => "cota",
            Origem::SemFonte => "sem_fonte",
            Origem::SemDados => "sem_dados",
        }
    }

    pub fn rotulo(self) -> &'static str {
        match self {
            Origem::Calculo => "calculado pelo índice",
            Origem::Tesouro => "preço do Tesouro",
            Origem::Cotacao => "cotação da B3",
            Origem::Extrato => "saldo do extrato da corretora",
            Origem::Cota => "cota diária da CVM",
            Origem::SemFonte => "sem fonte de preço",
            Origem::SemDados => "sem dados ainda",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Valor {
    pub atual_centavos: i64,
    /// `(atual − aplicado) / aplicado` em décimos de ponto percentual (`+17,1%` → `171`).
    pub rentabilidade_decimos: i64,
    pub origem: Origem,
    /// Data dos dados usados; `None` quando não houve dado.
    pub data_referencia: Option<NaiveDate>,
}

/// Séries diárias e mensais até a data de valorização, carregadas uma vez para a carteira.
pub(crate) struct Series {
    cdi: Vec<PontoDiario>,
    selic: Vec<PontoDiario>,
    ipca: Vec<(String, Decimal)>,
}

impl Series {
    pub(crate) fn carregar(db: &Db, ate: NaiveDate) -> Result<Series, CoreError> {
        let desde = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or(NaiveDate::MIN);
        Ok(Series {
            cdi: db.indice_entre("cdi", desde, ate)?,
            selic: db.indice_entre("selic", desde, ate)?,
            ipca: db.ipca_meses("0000-00", &mes_de(ate))?,
        })
    }

    /// Pontos diários de `"cdi"` ou `"selic"`, em ordem; outra série não tem pontos.
    pub(crate) fn diaria(&self, serie: &str) -> &[PontoDiario] {
        match serie {
            "cdi" => &self.cdi,
            "selic" => &self.selic,
            _ => &[],
        }
    }

    /// Meses do IPCA guardados (`AAAA-MM`, valor), em ordem.
    pub(crate) fn ipca(&self) -> &[(String, Decimal)] {
        &self.ipca
    }
}

fn mes_de(d: NaiveDate) -> String {
    format!("{:04}-{:02}", d.year(), d.month())
}

/// Valor atual de `inv` em `data`, carregando as séries do banco.
pub fn valorizar(db: &Db, inv: &Investimento, data: NaiveDate) -> Result<Valor, CoreError> {
    valorizar_com(db, &Series::carregar(db, data)?, inv, data)
}

pub(crate) fn valorizar_com(
    db: &Db,
    series: &Series,
    inv: &Investimento,
    data: NaiveDate,
) -> Result<Valor, CoreError> {
    let aplicado = inv.valor_aplicado_centavos;
    if inv.tipo == "fundo" || inv.tipo == "prev" || inv.tipo == "cripto" {
        let saldo = saldo_ate(inv, data);
        let do_extrato = || match saldo {
            Some((s, ref_)) => com_atual(aplicado, s, Origem::Extrato, Some(ref_)),
            None => valor(aplicado, Origem::SemFonte, None),
        };
        if inv.tipo == "fundo"
            && let Some(cnpj) = inv.codigo_b3.as_deref().and_then(normalizar_cnpj)
        {
            let aplicacao =
                NaiveDate::parse_from_str(&inv.data_aplicacao, "%Y-%m-%d").map_err(|_| {
                    CoreError::DbInvalid("investimento com data inválida no banco".into())
                })?;
            if data < aplicacao {
                return Ok(sem_dados(aplicado));
            }
            if let Some(v) = fundo_pela_cota(db, inv, &cnpj, saldo, aplicacao, data)? {
                return Ok(v);
            }
            return Ok(match saldo {
                Some(_) => do_extrato(),
                None => sem_dados(aplicado),
            });
        }
        return Ok(do_extrato());
    }
    let aplicacao = NaiveDate::parse_from_str(&inv.data_aplicacao, "%Y-%m-%d")
        .map_err(|_| CoreError::DbInvalid("investimento com data inválida no banco".into()))?;
    if data < aplicacao {
        return Ok(sem_dados(aplicado));
    }
    let calculado = if inv.tipo.starts_with("rf-") {
        renda_fixa(series, inv, aplicacao, data)
    } else if inv.tipo.starts_with("td-") {
        tesouro(db, inv, aplicacao, data)?
    } else if inv.tipo.starts_with("rv-") {
        renda_variavel(db, inv, data)?
    } else {
        None
    };
    Ok(calculado.unwrap_or_else(|| sem_dados(aplicado)))
}

/// Saldo do extrato e a data dele, quando existe e não é posterior a `data`.
fn saldo_ate(inv: &Investimento, data: NaiveDate) -> Option<(i64, NaiveDate)> {
    let saldo = inv.saldo_centavos?;
    let ref_ = NaiveDate::parse_from_str(inv.saldo_data.as_deref()?, "%Y-%m-%d").ok()?;
    (ref_ <= data).then_some((saldo, ref_))
}

/// Valor do fundo pela cota da CVM: a base (saldo do extrato ou valor aplicado) corrigida pela
/// variação da cota entre a data da base e a última cota até `data`. `None` quando falta cota,
/// quando não há cota mais nova que a do saldo ou quando a conta estoura.
fn fundo_pela_cota(
    db: &Db,
    inv: &Investimento,
    cnpj: &str,
    saldo: Option<(i64, NaiveDate)>,
    aplicacao: NaiveDate,
    data: NaiveDate,
) -> Result<Option<Valor>, CoreError> {
    let (base, base_data) = saldo.unwrap_or((inv.valor_aplicado_centavos, aplicacao));
    let (Some((data_atual, cota_atual)), Some((data_base, cota_base))) = (
        db.cota_fundo_em(cnpj, data)?,
        db.cota_fundo_em(cnpj, base_data)?,
    ) else {
        return Ok(None);
    };
    if saldo.is_some() && data_atual <= data_base {
        return Ok(None);
    }
    let atual = Decimal::from(base)
        .checked_mul(cota_atual)
        .and_then(|v| v.checked_div(cota_base))
        .and_then(centavos);
    Ok(atual.map(|a| {
        com_atual(
            inv.valor_aplicado_centavos,
            a,
            Origem::Cota,
            Some(data_atual),
        )
    }))
}

/// Valor de `inv` em cada uma de `datas` (em ordem crescente), igual a chamar `valorizar_com`
/// data a data. O pós-fixado, que domina o custo, percorre o fator diário uma vez só.
pub(crate) fn valorizar_datas(
    db: &Db,
    series: &Series,
    inv: &Investimento,
    datas: &[NaiveDate],
) -> Result<Vec<Valor>, CoreError> {
    if datas.is_sorted()
        && let Some(v) = pos_fixado_datas(series, inv, datas)?
    {
        return Ok(v);
    }
    datas
        .iter()
        .map(|&d| valorizar_com(db, series, inv, d))
        .collect()
}

/// `renda_fixa` do pós-fixado em várias datas, acumulando o fator na mesma ordem dos dias.
/// `None` quando o investimento não é um pós-fixado com taxa e série que o cubram.
fn pos_fixado_datas(
    series: &Series,
    inv: &Investimento,
    datas: &[NaiveDate],
) -> Result<Option<Vec<Valor>>, CoreError> {
    if !inv.tipo.starts_with("rf-") || inv.indexador != Some(Indexador::Pos) {
        return Ok(None);
    }
    let (Some(benchmark), Some(taxa)) = (
        inv.benchmark,
        inv.taxa.as_deref().and_then(|t| Decimal::from_str(t).ok()),
    ) else {
        return Ok(None);
    };
    let aplicacao = NaiveDate::parse_from_str(&inv.data_aplicacao, "%Y-%m-%d")
        .map_err(|_| CoreError::DbInvalid("investimento com data inválida no banco".into()))?;
    let pontos = match benchmark {
        Benchmark::Cdi => &series.cdi,
        Benchmark::Selic => &series.selic,
    };
    if !cobre(pontos, aplicacao) {
        return Ok(None);
    }
    let cem = Decimal::from(100);
    let aplicado = inv.valor_aplicado_centavos;
    // `None` depois de um estouro: dali em diante o fator não existe mais, como em `renda_fixa`.
    let mut fator = Some(Decimal::ONE);
    let mut ref_ = None;
    let mut ate = aplicacao;
    let mut out = Vec::with_capacity(datas.len());
    for &d in datas {
        if d < aplicacao {
            out.push(sem_dados(aplicado));
            continue;
        }
        for p in dias_entre(pontos, ate, d) {
            fator = fator.and_then(|f| f.checked_mul(Decimal::ONE + p.valor / cem * taxa / cem));
            ref_ = Some(p.data);
        }
        ate = d;
        let atual = fator
            .and_then(|f| Decimal::from(aplicado).checked_mul(f))
            .and_then(centavos);
        out.push(match atual {
            Some(a) => com_atual(aplicado, a, Origem::Calculo, ref_),
            None => sem_dados(aplicado),
        });
    }
    Ok(Some(out))
}

fn sem_dados(aplicado: i64) -> Valor {
    valor(aplicado, Origem::SemDados, None)
}

fn valor(aplicado: i64, origem: Origem, ref_: Option<NaiveDate>) -> Valor {
    Valor {
        atual_centavos: aplicado,
        rentabilidade_decimos: 0,
        origem,
        data_referencia: ref_,
    }
}

fn com_atual(aplicado: i64, atual: i64, origem: Origem, ref_: Option<NaiveDate>) -> Valor {
    Valor {
        atual_centavos: atual,
        rentabilidade_decimos: rentabilidade(aplicado, atual),
        origem,
        data_referencia: ref_,
    }
}

/// `round((atual − aplicado) × 1000 / aplicado)`, meio para cima, em inteiros.
pub(crate) fn rentabilidade(aplicado: i64, atual: i64) -> i64 {
    if aplicado <= 0 {
        return 0;
    }
    let (a, v) = (i128::from(aplicado), i128::from(atual));
    let dec = ((v - a) * 2000 + a).div_euclid(2 * a);
    i64::try_from(dec).unwrap_or(0)
}

fn centavos(d: Decimal) -> Option<i64> {
    d.floor().to_i64()
}

/// Pontos da série com `de < data <= ate` (a série vem ordenada).
fn dias_entre(pontos: &[PontoDiario], de: NaiveDate, ate: NaiveDate) -> &[PontoDiario] {
    let ini = pontos.partition_point(|p| p.data <= de);
    let fim = pontos.partition_point(|p| p.data <= ate);
    &pontos[ini..fim.max(ini)]
}

/// A série cobre a aplicação se tem algum ponto e o primeiro não é mais de 7 dias depois dela.
fn cobre(pontos: &[PontoDiario], aplicacao: NaiveDate) -> bool {
    let limite = aplicacao.checked_add_days(Days::new(FOLGA_COBERTURA_DIAS));
    match (pontos.first(), limite) {
        (Some(p), Some(limite)) => p.data <= limite,
        _ => false,
    }
}

/// `(1 + taxa/100)^(du/252)` por `exp(ln(1 + taxa/100) × du/252)`, com 12 casas.
fn potencia(taxa: Decimal, du: usize) -> Option<Decimal> {
    if du == 0 {
        return Some(Decimal::ONE);
    }
    let base = Decimal::ONE + taxa / Decimal::from(100);
    let expoente =
        base.checked_ln()?.checked_mul(Decimal::from(du as i64))? / Decimal::from(DIAS_UTEIS_ANO);
    Some(expoente.checked_exp()?.round_dp(12))
}

fn renda_fixa(
    series: &Series,
    inv: &Investimento,
    aplicacao: NaiveDate,
    data: NaiveDate,
) -> Option<Valor> {
    let taxa = Decimal::from_str(inv.taxa.as_deref()?).ok()?;
    let cem = Decimal::from(100);
    let aplicado = inv.valor_aplicado_centavos;
    let (fator, ref_) = match inv.indexador? {
        Indexador::Pos => {
            let pontos = match inv.benchmark? {
                Benchmark::Cdi => &series.cdi,
                Benchmark::Selic => &series.selic,
            };
            if !cobre(pontos, aplicacao) {
                return None;
            }
            let dias = dias_entre(pontos, aplicacao, data);
            let mut fator = Decimal::ONE;
            for p in dias {
                fator = fator.checked_mul(Decimal::ONE + p.valor / cem * taxa / cem)?;
            }
            (fator, dias.last().map(|p| p.data))
        }
        Indexador::Pre => {
            if !cobre(&series.cdi, aplicacao) {
                return None;
            }
            let dias = dias_entre(&series.cdi, aplicacao, data);
            (potencia(taxa, dias.len())?, dias.last().map(|p| p.data))
        }
        Indexador::Ipca => {
            if !cobre(&series.cdi, aplicacao) {
                return None;
            }
            // Meses `m` do seguinte ao da aplicação até o último que já terminou em `data`.
            let mut mes = aplicacao
                .with_day(1)?
                .checked_add_months(chrono::Months::new(1))?;
            let ultimo = if data.succ_opt()?.month() != data.month() {
                data.with_day(1)?
            } else {
                data.with_day(1)?.pred_opt()?.with_day(1)?
            };
            let mut inflacao = Decimal::ONE;
            while mes <= ultimo {
                let chave = mes_de(mes);
                let proximo = mes.checked_add_months(chrono::Months::new(1))?;
                match series.ipca.iter().find(|(m, _)| *m == chave) {
                    Some((_, v)) => inflacao = inflacao.checked_mul(Decimal::ONE + *v / cem)?,
                    None => {
                        // Ainda dentro do prazo de publicação: o mês entra com fator 1.
                        let fim_do_mes = proximo.pred_opt()?;
                        let limite = fim_do_mes.checked_add_days(Days::new(PRAZO_IPCA_DIAS))?;
                        if limite <= data {
                            return None;
                        }
                    }
                }
                mes = proximo;
            }
            let dias = dias_entre(&series.cdi, aplicacao, data);
            let fator = inflacao.checked_mul(potencia(taxa, dias.len())?)?;
            (fator, dias.last().map(|p| p.data))
        }
        Indexador::Selic => return None,
    };
    let atual = centavos(Decimal::from(aplicado).checked_mul(fator)?)?;
    Some(com_atual(aplicado, atual, Origem::Calculo, ref_))
}

fn tesouro(
    db: &Db,
    inv: &Investimento,
    aplicacao: NaiveDate,
    data: NaiveDate,
) -> Result<Option<Valor>, CoreError> {
    let Some((titulo, vencimento)) = titulo_tesouro(inv) else {
        return Ok(None);
    };
    let (Some(compra), Some(base)) = (
        db.preco_tesouro_em(&titulo, vencimento, aplicacao)?,
        db.preco_tesouro_em(&titulo, vencimento, data)?,
    ) else {
        return Ok(None);
    };
    if compra.pu_compra <= Decimal::ZERO {
        return Ok(None);
    }
    let aplicado = inv.valor_aplicado_centavos;
    let atual = Decimal::from(aplicado)
        .checked_div(compra.pu_compra)
        .and_then(|q| q.checked_mul(base.pu_base))
        .and_then(centavos);
    Ok(atual.map(|a| com_atual(aplicado, a, Origem::Tesouro, Some(base.data))))
}

fn renda_variavel(
    db: &Db,
    inv: &Investimento,
    data: NaiveDate,
) -> Result<Option<Valor>, CoreError> {
    let (Some(ticker), Some(quantidade)) = (inv.ticker.as_deref(), inv.quantidade) else {
        return Ok(None);
    };
    let Some(c) = db.cotacao_em(ticker, data)? else {
        return Ok(None);
    };
    let atual = quantidade.checked_mul(c.fechamento_centavos);
    Ok(atual.map(|a| {
        com_atual(
            inv.valor_aplicado_centavos,
            a,
            Origem::Cotacao,
            Some(c.data),
        )
    }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn rentabilidade_em_decimos_meio_para_cima() {
        assert_eq!(rentabilidade(1_000_000, 1_171_000), 171);
        assert_eq!(rentabilidade(1_000_000, 1_000_000), 0);
        assert_eq!(rentabilidade(1_000_000, 1_000_500), 1);
        assert_eq!(rentabilidade(1_000_000, 1_000_499), 0);
        assert_eq!(rentabilidade(1_000_000, 800_000), -200);
        assert_eq!(rentabilidade(1_000_000, 999_500), 0);
        assert_eq!(rentabilidade(1_000_000, 999_499), -1);
    }

    #[test]
    fn valorizar_datas_e_igual_a_valorizar_data_a_data() {
        use crate::investimento::{Gravacao, NovoInvestimento};
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let d = |a, m, dia| NaiveDate::from_ymd_opt(a, m, dia).unwrap();
        let hoje = d(2026, 10, 28);
        let mut cdi = Vec::new();
        let mut x = d(2026, 1, 1);
        while x <= hoje {
            if x.weekday().num_days_from_monday() < 5 {
                cdi.push(PontoDiario {
                    data: x,
                    valor: Decimal::new(55_131 + i64::from(x.ordinal()), 6),
                });
            }
            x = x.succ_opt().unwrap();
        }
        db.gravar_indice("cdi", &cdi).unwrap();
        let s = |v: &str| Some(v.to_string());
        for (indexador, taxa, benchmark) in [
            ("pos", "103.5", Some("cdi")),
            ("pos", "100", Some("selic")), // sem série: sem dados em todas as datas
            ("pre", "12.4", None),
        ] {
            let n = NovoInvestimento {
                tipo: "rf-cdb".into(),
                instituicao: "Inter".into(),
                emissor: s("Inter"),
                indexador: s(indexador),
                taxa: s(taxa),
                benchmark: benchmark.and_then(s),
                data_aplicacao: s("2026-03-17"),
                vencimento: s("2030-03-15"),
                valor_aplicado_centavos: Some(1_234_567),
                liquidez: s("vencimento"),
                ir: s("regressiva"),
                ..Default::default()
            };
            assert!(matches!(
                db.criar_investimento(&n, hoje).unwrap(),
                Gravacao::Ok(_)
            ));
        }
        let series = Series::carregar(&db, hoje).unwrap();
        let datas = [
            d(2026, 2, 28),
            d(2026, 3, 17),
            d(2026, 3, 31),
            d(2026, 7, 31),
            d(2026, 9, 30),
            hoje,
        ];
        for inv in db.listar_investimentos(hoje).unwrap() {
            let juntos = valorizar_datas(&db, &series, &inv, &datas).unwrap();
            let um_a_um: Vec<Valor> = datas
                .iter()
                .map(|&x| valorizar_com(&db, &series, &inv, x).unwrap())
                .collect();
            assert_eq!(juntos, um_a_um, "{:?}", inv.indexador);
        }
    }

    #[test]
    fn potencia_de_um_ano_cheio_e_a_taxa() {
        let p = potencia(Decimal::from(12), 252).unwrap();
        assert!((p - Decimal::new(112, 2)).abs() < Decimal::new(1, 9));
        assert_eq!(potencia(Decimal::from(12), 0), Some(Decimal::ONE));
    }
}
