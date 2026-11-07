use chrono::{NaiveDate, SecondsFormat, Utc};
use rusqlite::{Connection, Row, named_params};

use super::catalogo::{Benchmark, Classe, Garantia, Indexador, Ir, Liquidez, TipoInfo, tipo};
use super::modelo::{ErroCampo, InvestimentoValido, NovoInvestimento};
use super::texto::{nome, rendimento};
use super::validacao::validar;
use crate::db::{Db, classifica};
use crate::error::CoreError;
use crate::mercado::{Origem, Series, valorizar_com};

/// Resultado de criar ou atualizar: gravou (com o id) ou devolveu os erros de validação.
#[derive(Debug)]
pub enum Gravacao {
    Ok(i64),
    Invalido(Vec<ErroCampo>),
}

#[derive(Debug, Clone)]
pub struct Investimento {
    pub id: i64,
    pub tipo: String,
    pub tipo_rotulo: String,
    pub classe: Classe,
    pub nome: String,
    pub instituicao: String,
    pub emissor: Option<String>,
    pub ticker: Option<String>,
    pub indexador: Option<Indexador>,
    pub taxa: Option<String>,
    pub benchmark: Option<Benchmark>,
    pub rendimento: Option<String>,
    pub data_aplicacao: String,
    pub vencimento: Option<String>,
    pub valor_aplicado_centavos: i64,
    pub quantidade: Option<i64>,
    pub preco_medio_centavos: Option<i64>,
    pub liquidez: Option<Liquidez>,
    pub data_carencia: Option<String>,
    pub ir: Option<Ir>,
    pub garantia: Option<Garantia>,
    pub observacao: Option<String>,
    /// Código do ativo na B3 quando o investimento veio de uma importação.
    pub codigo_b3: Option<String>,
    pub meta_id: Option<i64>,
    pub meta_nome: Option<String>,
    /// Saldo bruto informado pelo extrato da corretora (fundos, cripto, previdência).
    pub saldo_centavos: Option<i64>,
    /// Data do saldo (`AAAA-MM-DD`).
    pub saldo_data: Option<String>,
    /// Valor bruto (antes do IR e de taxas) na data da consulta.
    pub valor_atual_centavos: i64,
    /// Décimos de ponto percentual sobre o valor aplicado.
    pub rentabilidade_decimos: i64,
    pub origem_valor: Origem,
    /// Data dos dados usados no valor atual.
    pub data_valor: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoClasse {
    pub classe: Classe,
    pub total_aplicado_centavos: i64,
    pub total_atual_centavos: i64,
    pub quantidade: i64,
    /// Parte do total em décimos de ponto percentual (`round(parte × 1000 / total)`).
    pub percentual_decimos: i64,
    /// Alvo da alocação em pontos percentuais; `None` se nunca foi definida.
    pub alvo_percentual: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resumo {
    pub total_aplicado_centavos: i64,
    pub total_atual_centavos: i64,
    pub classes: Vec<ResumoClasse>,
}

const SELECT: &str = "SELECT i.id, i.tipo, i.instituicao, i.emissor, i.ticker, i.indexador, \
    i.taxa, i.benchmark, i.data_aplicacao, i.vencimento, i.valor_aplicado_centavos, \
    i.quantidade, i.preco_medio_centavos, i.liquidez, i.data_carencia, i.ir, i.observacao, \
    i.codigo_b3, i.meta_id, m.nome AS meta_nome, i.saldo_centavos, i.saldo_data \
    FROM investimentos i LEFT JOIN metas m ON m.id = i.meta_id";

fn agora() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn corrompido(detalhe: &str) -> CoreError {
    CoreError::DbInvalid(format!(
        "investimento com dado inválido no banco ({detalhe})"
    ))
}

fn codigo<T>(
    valor: Option<String>,
    campo: &str,
    de_codigo: fn(&str) -> Option<T>,
) -> Result<Option<T>, CoreError> {
    valor
        .map(|c| de_codigo(&c).ok_or_else(|| corrompido(campo)))
        .transpose()
}

fn data_valida(valor: &Option<String>) -> Option<NaiveDate> {
    valor
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

/// Linha crua da tabela, convertida em `Investimento` fora do callback do rusqlite.
struct Linha {
    id: i64,
    tipo: String,
    instituicao: String,
    emissor: Option<String>,
    ticker: Option<String>,
    indexador: Option<String>,
    taxa: Option<String>,
    benchmark: Option<String>,
    data_aplicacao: String,
    vencimento: Option<String>,
    valor_aplicado: i64,
    quantidade: Option<i64>,
    preco_medio: Option<i64>,
    liquidez: Option<String>,
    data_carencia: Option<String>,
    ir: Option<String>,
    observacao: Option<String>,
    codigo_b3: Option<String>,
    meta_id: Option<i64>,
    meta_nome: Option<String>,
    saldo_centavos: Option<i64>,
    saldo_data: Option<String>,
}

fn linha(r: &Row) -> rusqlite::Result<Linha> {
    Ok(Linha {
        id: r.get("id")?,
        tipo: r.get("tipo")?,
        instituicao: r.get("instituicao")?,
        emissor: r.get("emissor")?,
        ticker: r.get("ticker")?,
        indexador: r.get("indexador")?,
        taxa: r.get("taxa")?,
        benchmark: r.get("benchmark")?,
        data_aplicacao: r.get("data_aplicacao")?,
        vencimento: r.get("vencimento")?,
        valor_aplicado: r.get("valor_aplicado_centavos")?,
        quantidade: r.get("quantidade")?,
        preco_medio: r.get("preco_medio_centavos")?,
        liquidez: r.get("liquidez")?,
        data_carencia: r.get("data_carencia")?,
        ir: r.get("ir")?,
        observacao: r.get("observacao")?,
        codigo_b3: r.get("codigo_b3")?,
        meta_id: r.get("meta_id")?,
        meta_nome: r.get("meta_nome")?,
        saldo_centavos: r.get("saldo_centavos")?,
        saldo_data: r.get("saldo_data")?,
    })
}

fn montar(l: Linha) -> Result<Investimento, CoreError> {
    let info: &'static TipoInfo = tipo(&l.tipo).ok_or_else(|| corrompido("tipo"))?;
    let indexador = codigo(l.indexador, "indexador", Indexador::de_codigo)?;
    let benchmark = codigo(l.benchmark, "benchmark", Benchmark::de_codigo)?;
    let liquidez = codigo(l.liquidez, "liquidez", Liquidez::de_codigo)?;
    let ir = codigo(l.ir, "ir", Ir::de_codigo)?;
    let taxa = l
        .taxa
        .map(|t| t.parse().map_err(|_| corrompido("taxa")))
        .transpose()?;
    let aplicacao =
        data_valida(&Some(l.data_aplicacao.clone())).ok_or_else(|| corrompido("data"))?;

    let valido = InvestimentoValido {
        tipo: info,
        instituicao: l.instituicao.clone(),
        emissor: l.emissor.clone(),
        ticker: l.ticker.clone(),
        indexador,
        taxa,
        benchmark,
        data_aplicacao: aplicacao,
        vencimento: data_valida(&l.vencimento),
        valor_aplicado_centavos: Some(l.valor_aplicado),
        quantidade: l.quantidade,
        preco_medio_centavos: l.preco_medio,
        liquidez,
        data_carencia: data_valida(&l.data_carencia),
        ir,
        observacao: l.observacao.clone(),
        cnpj: None,
    };

    Ok(Investimento {
        id: l.id,
        tipo: info.codigo.to_string(),
        tipo_rotulo: info.rotulo.to_string(),
        classe: info.classe,
        nome: nome(&valido),
        instituicao: l.instituicao,
        emissor: l.emissor,
        ticker: l.ticker,
        indexador,
        taxa: taxa.map(|t: rust_decimal::Decimal| t.normalize().to_string()),
        benchmark,
        rendimento: rendimento(&valido),
        data_aplicacao: l.data_aplicacao,
        vencimento: l.vencimento,
        valor_aplicado_centavos: l.valor_aplicado,
        quantidade: l.quantidade,
        preco_medio_centavos: l.preco_medio,
        liquidez,
        data_carencia: l.data_carencia,
        ir,
        garantia: info.garantia,
        observacao: l.observacao,
        codigo_b3: l.codigo_b3,
        meta_id: l.meta_id,
        meta_nome: l.meta_nome,
        saldo_centavos: l.saldo_centavos,
        saldo_data: l.saldo_data,
        valor_atual_centavos: l.valor_aplicado,
        rentabilidade_decimos: 0,
        origem_valor: Origem::SemDados,
        data_valor: None,
    })
}

fn iso(d: Option<NaiveDate>) -> Option<String> {
    d.map(|d| d.to_string())
}

/// Valores de um `InvestimentoValido` já no formato de coluna.
struct Colunas {
    tipo: &'static str,
    instituicao: String,
    emissor: Option<String>,
    ticker: Option<String>,
    indexador: Option<&'static str>,
    taxa: Option<String>,
    benchmark: Option<&'static str>,
    data_aplicacao: String,
    vencimento: Option<String>,
    valor: i64,
    quantidade: Option<i64>,
    preco_medio: Option<i64>,
    liquidez: Option<&'static str>,
    data_carencia: Option<String>,
    ir: Option<&'static str>,
    observacao: Option<String>,
    /// Vira `codigo_b3` quando o tipo é `fundo`.
    cnpj: Option<String>,
    e_fundo: bool,
    meta_id: Option<i64>,
}

impl Colunas {
    fn de(v: InvestimentoValido, meta_id: Option<i64>) -> Colunas {
        // Em renda variável o valor aplicado é quantidade × preço médio (sem estouro: a
        // validação já recusou); nas demais formas é o valor informado.
        let valor = match (v.quantidade, v.preco_medio_centavos) {
            (Some(q), Some(p)) => q.saturating_mul(p),
            _ => v.valor_aplicado_centavos.unwrap_or_default(),
        };
        Colunas {
            tipo: v.tipo.codigo,
            instituicao: v.instituicao,
            emissor: v.emissor,
            ticker: v.ticker,
            indexador: v.indexador.map(|i| i.codigo()),
            taxa: v.taxa.map(|t| t.normalize().to_string()),
            benchmark: v.benchmark.map(|b| b.codigo()),
            data_aplicacao: iso(Some(v.data_aplicacao)).unwrap_or_default(),
            vencimento: iso(v.vencimento),
            valor,
            quantidade: v.quantidade,
            preco_medio: v.preco_medio_centavos,
            liquidez: v.liquidez.map(|l| l.codigo()),
            data_carencia: iso(v.data_carencia),
            ir: v.ir.map(|i| i.codigo()),
            observacao: v.observacao,
            cnpj: v.cnpj,
            e_fundo: v.tipo.codigo == "fundo",
            meta_id,
        }
    }
}

/// Parâmetros nomeados de INSERT/UPDATE; `extra` acrescenta pares (ex.: `":id": id`).
macro_rules! parametros {
    ($c:expr, $agora:expr $(, $extra:tt : $valor:expr)*) => {
        named_params! {
            ":tipo": $c.tipo,
            ":instituicao": $c.instituicao,
            ":emissor": $c.emissor,
            ":ticker": $c.ticker,
            ":indexador": $c.indexador,
            ":taxa": $c.taxa,
            ":benchmark": $c.benchmark,
            ":data_aplicacao": $c.data_aplicacao,
            ":vencimento": $c.vencimento,
            ":valor": $c.valor,
            ":quantidade": $c.quantidade,
            ":preco_medio": $c.preco_medio,
            ":liquidez": $c.liquidez,
            ":data_carencia": $c.data_carencia,
            ":ir": $c.ir,
            ":observacao": $c.observacao,
            ":meta_id": $c.meta_id,
            ":agora": $agora,
            $($extra: $valor),*
        }
    };
}

/// `validar` mais a checagem de `meta_id`, que precisa do banco; os erros vêm juntos.
pub(crate) fn validar_com_meta(
    conn: &Connection,
    novo: &NovoInvestimento,
    hoje: NaiveDate,
) -> Result<Result<InvestimentoValido, Vec<ErroCampo>>, CoreError> {
    let validado = validar(novo, hoje);
    let meta_existe = match novo.meta_id {
        None => true,
        Some(id) => conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM metas WHERE id = :id)",
                named_params! {":id": id},
                |r| r.get(0),
            )
            .map_err(classifica)?,
    };
    if meta_existe {
        return Ok(validado);
    }
    let mut erros = validado.err().unwrap_or_default();
    erros.push(ErroCampo {
        campo: "metaId".to_string(),
        mensagem: "Escolha uma meta da lista.".to_string(),
    });
    Ok(Err(erros))
}

/// Grava um investimento já validado; funciona dentro de uma transação aberta pelo chamador.
pub(crate) fn inserir_validado(
    conn: &Connection,
    v: InvestimentoValido,
    meta_id: Option<i64>,
) -> Result<i64, CoreError> {
    let agora = agora();
    let c = Colunas::de(v, meta_id);
    conn.execute(
        "INSERT INTO investimentos (tipo, instituicao, emissor, ticker, indexador, taxa, \
         benchmark, data_aplicacao, vencimento, valor_aplicado_centavos, quantidade, \
         preco_medio_centavos, liquidez, data_carencia, ir, observacao, codigo_b3, meta_id, \
         criado_em, atualizado_em) VALUES (:tipo, :instituicao, :emissor, :ticker, :indexador, :taxa, \
         :benchmark, :data_aplicacao, :vencimento, :valor, :quantidade, :preco_medio, \
         :liquidez, :data_carencia, :ir, :observacao, :cnpj, :meta_id, :agora, :agora)",
        parametros!(c, agora, ":cnpj": c.cnpj),
    )
    .map_err(classifica)?;
    Ok(conn.last_insert_rowid())
}

pub(crate) fn apagar_em(conn: &Connection, id: i64) -> Result<(), CoreError> {
    let n = conn
        .execute(
            "DELETE FROM investimentos WHERE id = :id",
            named_params! {":id": id},
        )
        .map_err(classifica)?;
    if n == 0 {
        return Err(CoreError::NaoEncontrado(id));
    }
    Ok(())
}

impl Db {
    pub fn criar_investimento(
        &self,
        novo: &NovoInvestimento,
        hoje: NaiveDate,
    ) -> Result<Gravacao, CoreError> {
        let v = match validar_com_meta(&self.conn, novo, hoje)? {
            Ok(v) => v,
            Err(erros) => return Ok(Gravacao::Invalido(erros)),
        };
        Ok(Gravacao::Ok(inserir_validado(&self.conn, v, novo.meta_id)?))
    }

    pub fn atualizar_investimento(
        &self,
        id: i64,
        novo: &NovoInvestimento,
        hoje: NaiveDate,
    ) -> Result<Gravacao, CoreError> {
        let v = match validar_com_meta(&self.conn, novo, hoje)? {
            Ok(v) => v,
            Err(erros) => return Ok(Gravacao::Invalido(erros)),
        };
        let agora = agora();
        let c = Colunas::de(v, novo.meta_id);
        let n = self
            .conn
            .execute(
                "UPDATE investimentos SET tipo = :tipo, instituicao = :instituicao, \
                 emissor = :emissor, ticker = :ticker, indexador = :indexador, taxa = :taxa, \
                 benchmark = :benchmark, data_aplicacao = :data_aplicacao, \
                 vencimento = :vencimento, valor_aplicado_centavos = :valor, \
                 quantidade = :quantidade, preco_medio_centavos = :preco_medio, \
                 liquidez = :liquidez, data_carencia = :data_carencia, ir = :ir, \
                 observacao = :observacao, \
                 codigo_b3 = CASE WHEN :e_fundo THEN :cnpj ELSE codigo_b3 END, \
                 meta_id = :meta_id, atualizado_em = :agora WHERE id = :id",
                parametros!(c, agora, ":id": id, ":e_fundo": c.e_fundo, ":cnpj": c.cnpj),
            )
            .map_err(classifica)?;
        if n == 0 {
            return Err(CoreError::NaoEncontrado(id));
        }
        Ok(Gravacao::Ok(id))
    }

    pub fn apagar_investimento(&self, id: i64) -> Result<(), CoreError> {
        apagar_em(&self.conn, id)
    }

    pub fn investimento(&self, id: i64, hoje: NaiveDate) -> Result<Investimento, CoreError> {
        let sql = format!("{SELECT} WHERE i.id = :id");
        let l = self
            .conn
            .query_row(&sql, named_params! {":id": id}, linha)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CoreError::NaoEncontrado(id),
                outro => classifica(outro),
            })?;
        let mut inv = montar(l)?;
        let series = Series::carregar(self, hoje)?;
        self.preencher_valor(&series, &mut inv, hoje)?;
        Ok(inv)
    }

    /// Investimentos como gravados, sem valorização (`valor_atual_centavos` = aplicado).
    pub(crate) fn listar_investimentos_brutos(&self) -> Result<Vec<Investimento>, CoreError> {
        let mut stmt = self.conn.prepare(SELECT).map_err(classifica)?;
        let linhas = stmt
            .query_map([], linha)
            .map_err(classifica)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(classifica)?;
        linhas.into_iter().map(montar).collect()
    }

    /// Ordenada pela classe (ordem do catálogo) e depois pelo nome, sem diferenciar maiúsculas.
    /// O valor atual de cada um é o de `hoje`; as séries são lidas uma vez só.
    pub fn listar_investimentos(&self, hoje: NaiveDate) -> Result<Vec<Investimento>, CoreError> {
        let mut lista = self.listar_investimentos_brutos()?;
        let series = Series::carregar(self, hoje)?;
        for inv in &mut lista {
            self.preencher_valor(&series, inv, hoje)?;
        }
        lista.sort_by_cached_key(|i| (ordem_classe(i.classe), i.nome.to_lowercase(), i.id));
        Ok(lista)
    }

    fn preencher_valor(
        &self,
        series: &Series,
        inv: &mut Investimento,
        hoje: NaiveDate,
    ) -> Result<(), CoreError> {
        let v = valorizar_com(self, series, inv, hoje)?;
        inv.valor_atual_centavos = v.atual_centavos;
        inv.rentabilidade_decimos = v.rentabilidade_decimos;
        inv.origem_valor = v.origem;
        inv.data_valor = v.data_referencia;
        Ok(())
    }

    /// Totais e percentuais pelo valor atual de `hoje`; o aplicado vai junto.
    pub fn resumo_carteira(&self, hoje: NaiveDate) -> Result<Resumo, CoreError> {
        let lista = self.listar_investimentos(hoje)?;
        let alvo = self.alocacao_alvo()?;
        let mut classes: Vec<ResumoClasse> = Vec::new();
        for i in &lista {
            match classes.iter_mut().find(|c| c.classe == i.classe) {
                Some(c) => {
                    c.total_aplicado_centavos =
                        soma(c.total_aplicado_centavos, i.valor_aplicado_centavos)?;
                    c.total_atual_centavos = soma(c.total_atual_centavos, i.valor_atual_centavos)?;
                    c.quantidade += 1;
                }
                None => classes.push(ResumoClasse {
                    classe: i.classe,
                    total_aplicado_centavos: soma(0, i.valor_aplicado_centavos)?,
                    total_atual_centavos: soma(0, i.valor_atual_centavos)?,
                    quantidade: 1,
                    percentual_decimos: 0,
                    alvo_percentual: None,
                }),
            }
        }
        if let Some(alvo) = &alvo {
            for (classe, pct) in alvo {
                if *pct > 0 && !classes.iter().any(|c| c.classe == *classe) {
                    classes.push(ResumoClasse {
                        classe: *classe,
                        total_aplicado_centavos: 0,
                        total_atual_centavos: 0,
                        quantidade: 0,
                        percentual_decimos: 0,
                        alvo_percentual: None,
                    });
                }
            }
        }
        classes.sort_by_key(|c| ordem_classe(c.classe));
        let total_aplicado = classes
            .iter()
            .try_fold(0_i64, |acc, c| soma(acc, c.total_aplicado_centavos))?;
        let total_atual = classes
            .iter()
            .try_fold(0_i64, |acc, c| soma(acc, c.total_atual_centavos))?;
        for c in &mut classes {
            c.percentual_decimos = decimos(c.total_atual_centavos, total_atual);
            c.alvo_percentual = alvo.as_ref().and_then(|a| {
                a.iter()
                    .find(|(classe, _)| *classe == c.classe)
                    .map(|(_, pct)| i64::from(*pct))
            });
        }
        Ok(Resumo {
            total_aplicado_centavos: total_aplicado,
            total_atual_centavos: total_atual,
            classes,
        })
    }
}

/// `round(parte × 1000 / total)` meio para cima; 0 com total não positivo.
pub(crate) fn decimos(parte: i64, total: i64) -> i64 {
    if total <= 0 {
        return 0;
    }
    let (p, t) = (i128::from(parte), i128::from(total));
    ((p * 2000 + t) / (2 * t)) as i64
}

fn estouro() -> CoreError {
    CoreError::DbInvalid("a soma dos valores da carteira excede o limite suportado".to_string())
}

fn soma(total: i64, valor: i64) -> Result<i64, CoreError> {
    total.checked_add(valor).ok_or_else(estouro)
}

fn ordem_classe(classe: Classe) -> usize {
    Classe::TODOS
        .iter()
        .position(|c| *c == classe)
        .unwrap_or(Classe::TODOS.len())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn hoje() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()
    }

    fn inserir(db: &Db, tipo: &str, valor: i64) {
        db.conn
            .execute(
                "INSERT INTO investimentos (tipo, instituicao, emissor, data_aplicacao, \
                 valor_aplicado_centavos, liquidez, criado_em, atualizado_em) \
                 VALUES (?1, 'X', 'Y', '2025-01-01', ?2, 'diaria', 'a', 'a')",
                rusqlite::params![tipo, valor],
            )
            .unwrap();
    }

    #[test]
    fn valor_aplicado_nulo_ou_zero_e_recusado_pelo_banco() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        for valor in ["NULL", "0"] {
            let r = db.conn.execute(
                &format!(
                    "INSERT INTO investimentos (tipo, instituicao, data_aplicacao, \
                     valor_aplicado_centavos, criado_em, atualizado_em) \
                     VALUES ('fundo', 'X', '2025-01-01', {valor}, 'a', 'a')"
                ),
                [],
            );
            assert!(r.is_err(), "valor {valor} deveria ser recusado");
        }
    }

    #[test]
    fn resumo_com_estouro_da_soma_da_erro_claro() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        inserir(&db, "fundo", i64::MAX);
        inserir(&db, "prev", i64::MAX);
        let e = db.resumo_carteira(hoje()).unwrap_err();
        assert!(matches!(&e, CoreError::DbInvalid(m) if m.contains("excede")));
    }

    #[test]
    fn resumo_com_estouro_entre_classes_da_erro_claro() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        inserir(&db, "fundo", i64::MAX);
        inserir(&db, "rv-acao", 1);
        assert!(matches!(
            db.resumo_carteira(hoje()),
            Err(CoreError::DbInvalid(_))
        ));
    }
}
