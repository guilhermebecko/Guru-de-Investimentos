//! Patrimônio e aportado nos fins de mês, mais hoje, e os retornos entre eles.
//! Regras em "Global Constraints" do plano da fase 3b.

use chrono::{Datelike, Months, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use crate::db::Db;
use crate::error::CoreError;
use crate::investimento::{Classe, Investimento};
use crate::mercado::{Series, Valor, valorizar_datas};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PontoHistorico {
    pub data: NaiveDate,
    pub patrimonio_centavos: i64,
    pub aportado_centavos: i64,
    /// Retorno do intervalo que termina neste ponto, em centésimos de ponto percentual;
    /// `None` no ponto 0 e quando não havia patrimônio no começo do intervalo.
    pub retorno_centesimos: Option<i64>,
    /// Retorno encadeado desde o ponto 0, em centésimos; o ponto 0 vale 0.
    pub acumulado_centesimos: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Historico {
    pub pontos: Vec<PontoHistorico>,
    /// Patrimônio por ponto, só das classes com algum investimento, na ordem de `Classe`.
    pub por_classe: Vec<(Classe, Vec<i64>)>,
    /// Índice do ponto onde começa o primeiro intervalo com retorno válido; `None` sem nenhum.
    /// Benchmarks e o CDI do período partem dele.
    pub inicio: Option<usize>,
    /// Data do primeiro aporte considerado, só quando é posterior ao primeiro ponto.
    pub desde: Option<NaiveDate>,
}

/// O último dia de cada um dos `meses` meses anteriores ao de `hoje`, em ordem crescente,
/// mais `hoje`. `meses` só pode ser 6 ou 12.
pub fn datas_painel(hoje: NaiveDate, meses: u32) -> Result<Vec<NaiveDate>, CoreError> {
    if meses != 6 && meses != 12 {
        return Err(CoreError::PeriodoInvalido(meses));
    }
    let primeiro = hoje.with_day(1).ok_or(CoreError::PeriodoInvalido(meses))?;
    let mut datas = Vec::with_capacity(meses as usize + 1);
    for k in (1..=meses).rev() {
        let fim = primeiro
            .checked_sub_months(Months::new(k - 1))
            .and_then(|d| d.pred_opt())
            .ok_or(CoreError::PeriodoInvalido(meses))?;
        datas.push(fim);
    }
    datas.push(hoje);
    Ok(datas)
}

fn data_aplicacao(inv: &Investimento) -> Result<NaiveDate, CoreError> {
    NaiveDate::parse_from_str(&inv.data_aplicacao, "%Y-%m-%d")
        .map_err(|_| CoreError::DbInvalid("investimento com data inválida no banco".into()))
}

fn somar(a: i64, b: i64) -> Result<i64, CoreError> {
    a.checked_add(b)
        .ok_or_else(|| CoreError::DbInvalid("soma da carteira excede o limite".into()))
}

/// `round(num / den)` meio para cima, com `den > 0`.
pub(super) fn dividir_arredondando(num: i128, den: i128) -> i128 {
    (2 * num + den).div_euclid(2 * den)
}

/// Dietz modificado, ponderado por dias corridos, no intervalo `(d0, d1]`, em centésimos:
/// `r = (V1 − V0 − ΣFᵢ) / (V0 + Σ wᵢFᵢ)` com `wᵢ = (d1 − tᵢ) / (d1 − d0)`. Ausente só com
/// denominador não positivo (sem patrimônio no começo e sem aporte que tenha ficado aplicado).
fn retorno_dietz(
    (d0, d1): (NaiveDate, NaiveDate),
    v0: i64,
    fluxos: &[(NaiveDate, i64)],
    v1: i64,
) -> Option<i64> {
    let dias = i128::from((d1 - d0).num_days());
    if dias <= 0 {
        return None;
    }
    let mut soma = 0i128;
    let mut ponderado = 0i128;
    for &(t, f) in fluxos {
        let f = i128::from(f);
        soma = soma.checked_add(f)?;
        ponderado = ponderado.checked_add(f.checked_mul(i128::from((d1 - t).num_days()))?)?;
    }
    let (v0, v1) = (i128::from(v0), i128::from(v1));
    let den = v0.checked_mul(dias)?.checked_add(ponderado)?;
    if den <= 0 {
        return None;
    }
    let num = (v1 - v0 - soma).checked_mul(dias)?.checked_mul(10_000)?;
    i64::try_from(dividir_arredondando(num, den)).ok()
}

/// Os dois valores vêm da mesma fonte: a mesma origem sobre a mesma base. Sobre o mesmo saldo
/// do extrato, o próprio saldo (`Extrato`) e a projeção dele pela cota (`Cota`) são a mesma
/// fonte; a cota sobre o aplicado e a cota sobre o saldo, não.
fn mesma_fonte(a: &Valor, b: &Valor) -> bool {
    match (a.base_saldo, b.base_saldo) {
        (Some(x), Some(y)) => x == y,
        (None, None) => a.origem == b.origem,
        _ => false,
    }
}

/// `Π(1 + r_k) − 1` em centésimos; retorno ausente conta como fator 1.
fn encadear(acumulado: Decimal, retorno: Option<i64>) -> Option<Decimal> {
    let r = Decimal::from(retorno.unwrap_or(0)) / Decimal::from(10_000);
    acumulado.checked_mul(Decimal::ONE + r)
}

pub(super) fn em_centesimos(fator: Decimal) -> i64 {
    let c = (fator - Decimal::ONE) * Decimal::from(10_000) + Decimal::new(5, 1);
    c.floor().to_i64().unwrap_or(0)
}

/// Investimentos considerados, com a data de aplicação já lida.
pub(super) fn com_aplicacao(
    investimentos: Vec<Investimento>,
    classe: Option<Classe>,
) -> Result<Vec<(Investimento, NaiveDate)>, CoreError> {
    investimentos
        .into_iter()
        .filter(|i| classe.is_none_or(|c| i.classe == c))
        .map(|i| data_aplicacao(&i).map(|d| (i, d)))
        .collect()
}

/// Patrimônio e aportado em cada data do painel, e os retornos entre elas. Com `classe`,
/// só entram os investimentos daquela classe.
pub fn historico(
    db: &Db,
    hoje: NaiveDate,
    meses: u32,
    classe: Option<Classe>,
) -> Result<Historico, CoreError> {
    let datas = datas_painel(hoje, meses)?;
    let series = Series::carregar(db, hoje)?;
    let investimentos = com_aplicacao(db.listar_investimentos(hoje)?, classe)?;
    historico_com(db, &series, &investimentos, &datas)
}

/// O mesmo que `historico`, com as séries, os investimentos e as datas já em mãos.
pub(super) fn historico_com(
    db: &Db,
    series: &Series,
    investimentos: &[(Investimento, NaiveDate)],
    datas: &[NaiveDate],
) -> Result<Historico, CoreError> {
    let valores: Vec<Vec<Valor>> = investimentos
        .iter()
        .map(|(inv, _)| valorizar_datas(db, series, inv, datas))
        .collect::<Result<_, _>>()?;

    let classes: Vec<Classe> = Classe::TODOS
        .iter()
        .copied()
        .filter(|c| investimentos.iter().any(|(i, _)| i.classe == *c))
        .collect();
    let mut por_classe: Vec<(Classe, Vec<i64>)> =
        classes.iter().map(|c| (*c, vec![0; datas.len()])).collect();

    let mut pontos: Vec<PontoHistorico> = Vec::with_capacity(datas.len());
    let mut acumulado = Decimal::ONE;
    let mut inicio = None;
    for (idx, &d) in datas.iter().enumerate() {
        let (mut patrimonio, mut aportado) = (0i64, 0i64);
        // Pontas do intervalo `(datas[idx-1], d]` para o retorno.
        let (mut v0, mut v1) = (0i64, 0i64);
        let mut fluxos: Vec<(NaiveDate, i64)> = Vec::new();
        for ((inv, aplicacao), vals) in investimentos.iter().zip(&valores) {
            if *aplicacao > d {
                continue;
            }
            let atual = vals[idx].atual_centavos;
            let aplicado = inv.valor_aplicado_centavos;
            patrimonio = somar(patrimonio, atual)?;
            aportado = somar(aportado, aplicado)?;
            if let Some((_, v)) = por_classe.iter_mut().find(|(c, _)| *c == inv.classe) {
                v[idx] = somar(v[idx], atual)?;
            }
            if idx == 0 {
                continue;
            }
            if *aplicacao > datas[idx - 1] {
                fluxos.push((*aplicacao, aplicado));
                v1 = somar(v1, atual)?;
            } else {
                let antes = &vals[idx - 1];
                // Mudou de origem (ganhou ou perdeu preço) ou de base no intervalo: a custo
                // nas duas pontas, para o ganho que não é do mês não virar rendimento dele.
                let (a, b) = if mesma_fonte(antes, &vals[idx]) {
                    (antes.atual_centavos, atual)
                } else {
                    (aplicado, aplicado)
                };
                v0 = somar(v0, a)?;
                v1 = somar(v1, b)?;
            }
        }
        let retorno = if idx == 0 {
            None
        } else {
            retorno_dietz((datas[idx - 1], d), v0, &fluxos, v1)
        };
        if retorno.is_some() && inicio.is_none() {
            inicio = Some(idx - 1);
        }
        if idx > 0 {
            acumulado = encadear(acumulado, retorno)
                .ok_or_else(|| CoreError::DbInvalid("retorno acumulado excede o limite".into()))?;
        }
        pontos.push(PontoHistorico {
            data: d,
            patrimonio_centavos: patrimonio,
            aportado_centavos: aportado,
            retorno_centesimos: retorno,
            acumulado_centesimos: em_centesimos(acumulado),
        });
    }
    let desde = investimentos
        .iter()
        .map(|(_, a)| *a)
        .filter(|a| *a <= datas[datas.len() - 1])
        .min()
        .filter(|a| datas.first().is_some_and(|d0| a > d0));
    Ok(Historico {
        pontos,
        por_classe,
        inicio,
        desde,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn data(a: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(a, m, d).unwrap()
    }

    #[test]
    fn datas_de_doze_meses() {
        let d = datas_painel(data(2026, 10, 28), 12).unwrap();
        assert_eq!(d.len(), 13);
        assert_eq!(d[0], data(2025, 10, 31));
        assert_eq!(d[1], data(2025, 11, 30));
        assert_eq!(d[11], data(2026, 9, 30));
        assert_eq!(d[12], data(2026, 10, 28));
    }

    #[test]
    fn datas_de_seis_meses_cruzando_o_ano() {
        let d = datas_painel(data(2026, 3, 15), 6).unwrap();
        assert_eq!(d.len(), 7);
        assert_eq!(d[0], data(2025, 9, 30));
        assert_eq!(d[5], data(2026, 2, 28));
        assert_eq!(d[6], data(2026, 3, 15));
    }

    #[test]
    fn periodo_invalido() {
        let e = datas_painel(data(2026, 10, 28), 7).unwrap_err();
        assert_eq!(e.code(), "PERIODO_INVALIDO");
    }

    /// Intervalo de 30 dias corridos: 31/08 a 30/09.
    const MES: (NaiveDate, NaiveDate) = (
        NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
        NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
    );

    #[test]
    fn dietz_do_caso_sintetico_com_aporte_no_meio_do_mes() {
        // Aporte em 15/09: ficou 15 de 30 dias, peso 1/2 → r = 1000 / 110000.
        let f = [(data(2026, 9, 15), 20_000)];
        assert_eq!(retorno_dietz(MES, 100_000, &f, 121_000), Some(91));
    }

    #[test]
    fn dietz_pesa_o_aporte_pelos_dias_aplicado() {
        // No primeiro dia (peso 29/30) quase todo o aporte rende; no último (peso 0), nada.
        let cedo = [(data(2026, 9, 1), 20_000)];
        assert_eq!(retorno_dietz(MES, 100_000, &cedo, 121_000), Some(84));
        let tarde = [(data(2026, 9, 30), 20_000)];
        assert_eq!(retorno_dietz(MES, 100_000, &tarde, 121_000), Some(100));
    }

    #[test]
    fn dietz_com_carteira_nova_tem_retorno() {
        // V0 = 0 e aporte em 01/09 (peso 29/30): 200 / (20000 × 29/30) = 1,03%.
        let f = [(data(2026, 9, 1), 20_000)];
        assert_eq!(retorno_dietz(MES, 0, &f, 20_200), Some(103));
    }

    #[test]
    fn dietz_ausente_so_com_denominador_nao_positivo() {
        assert_eq!(retorno_dietz(MES, 0, &[], 0), None);
        // Aporte no último dia não ficou aplicado: denominador 0.
        let f = [(data(2026, 9, 30), 20_000)];
        assert_eq!(retorno_dietz(MES, 0, &f, 21_000), None);
        assert_eq!(
            retorno_dietz(MES, 100, &[(data(2026, 9, 1), -500)], 0),
            None
        );
    }

    #[test]
    fn dietz_negativo_arredonda_meio_para_cima() {
        // -0,005 → -0,5 centésimo: meio para cima dá 0 (não -1).
        assert_eq!(retorno_dietz(MES, 20_000, &[], 19_999), Some(0));
        assert_eq!(retorno_dietz(MES, 100_000, &[], 90_000), Some(-1000));
    }

    #[test]
    fn acumulado_encadeia_dois_retornos_de_um_por_cento() {
        let a = encadear(Decimal::ONE, Some(100)).unwrap();
        let a = encadear(a, Some(100)).unwrap();
        assert_eq!(em_centesimos(a), 201);
        let a = encadear(a, None).unwrap();
        assert_eq!(em_centesimos(a), 201);
    }
}
