use chrono::NaiveDate;
use rusqlite::{OptionalExtension, params};
use rust_decimal::Decimal;

use super::{CotaFundo, Cotacao, PontoDiario, PrecoTesouro};
use crate::db::{Db, classifica};
use crate::error::CoreError;

const FONTES: [&str; 6] = ["cdi", "selic", "ipca", "tesouro", "b3", "cvm"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusFonte {
    pub fonte: String,
    pub ultima_data: Option<String>,
    pub atualizado_em: Option<String>,
    pub erro: Option<String>,
}

/// Cobertura da B3 de uma data-alvo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct B3Alvo {
    pub alvo: NaiveDate,
    /// Pregão achado; `None` se as tentativas se esgotaram sem arquivo.
    pub pregao: Option<NaiveDate>,
    /// Tickers pedidos naquela vez, ordenados.
    pub tickers: Vec<String>,
}

fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn data_de(s: String) -> Result<NaiveDate, CoreError> {
    NaiveDate::parse_from_str(&s, "%Y-%m-%d")
        .map_err(|_| CoreError::DbInvalid(format!("data inválida no banco: {s}")))
}

fn decimal_de(s: String) -> Result<Decimal, CoreError> {
    s.parse()
        .map_err(|_| CoreError::DbInvalid(format!("número inválido no banco: {s}")))
}

fn dec(d: Decimal) -> String {
    d.normalize().to_string()
}

impl Db {
    /// Upsert dos pontos diários da série (`cdi` ou `selic`); devolve quantos eram novos.
    pub fn gravar_indice(&self, serie: &str, pontos: &[PontoDiario]) -> Result<usize, CoreError> {
        let tx = self.conn.unchecked_transaction().map_err(classifica)?;
        let mut novos = 0;
        {
            let mut existe = tx
                .prepare("SELECT 1 FROM indice_diario WHERE serie = ?1 AND data = ?2")
                .map_err(classifica)?;
            let mut grava = tx
                .prepare(
                    "INSERT INTO indice_diario (serie, data, valor) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (serie, data) DO UPDATE SET valor = excluded.valor",
                )
                .map_err(classifica)?;
            for p in pontos {
                let data = iso(p.data);
                if !existe.exists(params![serie, data]).map_err(classifica)? {
                    novos += 1;
                }
                grava
                    .execute(params![serie, data, dec(p.valor)])
                    .map_err(classifica)?;
            }
        }
        tx.commit().map_err(classifica)?;
        Ok(novos)
    }

    /// Upsert do IPCA mensal; `mes` em `YYYY-MM`. Devolve quantos eram novos.
    pub fn gravar_ipca(&self, meses: &[(String, Decimal)]) -> Result<usize, CoreError> {
        let tx = self.conn.unchecked_transaction().map_err(classifica)?;
        let mut novos = 0;
        {
            let mut existe = tx
                .prepare("SELECT 1 FROM ipca_mensal WHERE mes = ?1")
                .map_err(classifica)?;
            let mut grava = tx
                .prepare(
                    "INSERT INTO ipca_mensal (mes, valor) VALUES (?1, ?2) \
                     ON CONFLICT (mes) DO UPDATE SET valor = excluded.valor",
                )
                .map_err(classifica)?;
            for (mes, valor) in meses {
                if !existe.exists(params![mes]).map_err(classifica)? {
                    novos += 1;
                }
                grava
                    .execute(params![mes, dec(*valor)])
                    .map_err(classifica)?;
            }
        }
        tx.commit().map_err(classifica)?;
        Ok(novos)
    }

    pub fn gravar_tesouro(&self, precos: &[PrecoTesouro]) -> Result<usize, CoreError> {
        let tx = self.conn.unchecked_transaction().map_err(classifica)?;
        let mut novos = 0;
        {
            let mut existe = tx
                .prepare(
                    "SELECT 1 FROM tesouro_preco WHERE tipo = ?1 AND vencimento = ?2 AND data = ?3",
                )
                .map_err(classifica)?;
            let mut grava = tx
                .prepare(
                    "INSERT INTO tesouro_preco \
                     (tipo, vencimento, data, taxa_compra, pu_compra, pu_base) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                     ON CONFLICT (tipo, vencimento, data) DO UPDATE SET \
                     taxa_compra = excluded.taxa_compra, pu_compra = excluded.pu_compra, \
                     pu_base = excluded.pu_base",
                )
                .map_err(classifica)?;
            for p in precos {
                let (venc, data) = (iso(p.vencimento), iso(p.data));
                if !existe
                    .exists(params![p.tipo, venc, data])
                    .map_err(classifica)?
                {
                    novos += 1;
                }
                grava
                    .execute(params![
                        p.tipo,
                        venc,
                        data,
                        dec(p.taxa_compra),
                        dec(p.pu_compra),
                        dec(p.pu_base)
                    ])
                    .map_err(classifica)?;
            }
        }
        tx.commit().map_err(classifica)?;
        Ok(novos)
    }

    pub fn gravar_cotacoes(&self, cotacoes: &[Cotacao]) -> Result<usize, CoreError> {
        let tx = self.conn.unchecked_transaction().map_err(classifica)?;
        let mut novos = 0;
        {
            let mut existe = tx
                .prepare("SELECT 1 FROM cotacao WHERE ticker = ?1 AND data = ?2")
                .map_err(classifica)?;
            let mut grava = tx
                .prepare(
                    "INSERT INTO cotacao (ticker, data, fechamento_centavos) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (ticker, data) DO UPDATE SET \
                     fechamento_centavos = excluded.fechamento_centavos",
                )
                .map_err(classifica)?;
            for c in cotacoes {
                let data = iso(c.data);
                if !existe.exists(params![c.ticker, data]).map_err(classifica)? {
                    novos += 1;
                }
                grava
                    .execute(params![c.ticker, data, c.fechamento_centavos])
                    .map_err(classifica)?;
            }
        }
        tx.commit().map_err(classifica)?;
        Ok(novos)
    }

    /// Upsert das cotas diárias de fundos; devolve quantas eram novas.
    pub fn gravar_cotas_fundo(&self, cotas: &[CotaFundo]) -> Result<usize, CoreError> {
        let tx = self.conn.unchecked_transaction().map_err(classifica)?;
        let mut novas = 0;
        {
            let mut existe = tx
                .prepare("SELECT 1 FROM cota_fundo WHERE cnpj = ?1 AND data = ?2")
                .map_err(classifica)?;
            let mut grava = tx
                .prepare(
                    "INSERT INTO cota_fundo (cnpj, data, cota) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (cnpj, data) DO UPDATE SET cota = excluded.cota",
                )
                .map_err(classifica)?;
            for c in cotas {
                let data = iso(c.data);
                if !existe.exists(params![c.cnpj, data]).map_err(classifica)? {
                    novas += 1;
                }
                grava
                    .execute(params![c.cnpj, data, dec(c.cota)])
                    .map_err(classifica)?;
            }
        }
        tx.commit().map_err(classifica)?;
        Ok(novas)
    }

    /// A última cota do fundo com data `<= data`, se houver.
    pub fn cota_fundo_em(
        &self,
        cnpj: &str,
        data: NaiveDate,
    ) -> Result<Option<(NaiveDate, Decimal)>, CoreError> {
        let linha: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT data, cota FROM cota_fundo WHERE cnpj = ?1 AND data <= ?2 \
                 ORDER BY data DESC LIMIT 1",
                params![cnpj, iso(data)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(classifica)?;
        linha
            .map(|(d, c)| Ok((data_de(d)?, decimal_de(c)?)))
            .transpose()
    }

    /// A maior data de cota gravada, de qualquer fundo.
    pub fn ultima_data_cota_fundo(&self) -> Result<Option<NaiveDate>, CoreError> {
        let d: Option<String> = self
            .conn
            .query_row("SELECT MAX(data) FROM cota_fundo", [], |r| r.get(0))
            .map_err(classifica)?;
        d.map(data_de).transpose()
    }

    /// Pontos da série com `de <= data <= ate`, em ordem de data.
    pub fn indice_entre(
        &self,
        serie: &str,
        de: NaiveDate,
        ate: NaiveDate,
    ) -> Result<Vec<PontoDiario>, CoreError> {
        let mut st = self
            .conn
            .prepare(
                "SELECT data, valor FROM indice_diario \
                 WHERE serie = ?1 AND data >= ?2 AND data <= ?3 ORDER BY data",
            )
            .map_err(classifica)?;
        let linhas = st
            .query_map(params![serie, iso(de), iso(ate)], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(classifica)?;
        let mut out = Vec::new();
        for l in linhas {
            let (d, v) = l.map_err(classifica)?;
            out.push(PontoDiario {
                data: data_de(d)?,
                valor: decimal_de(v)?,
            });
        }
        Ok(out)
    }

    pub fn primeira_data_indice(&self, serie: &str) -> Result<Option<NaiveDate>, CoreError> {
        let d: Option<String> = self
            .conn
            .query_row(
                "SELECT MIN(data) FROM indice_diario WHERE serie = ?1",
                [serie],
                |r| r.get(0),
            )
            .map_err(classifica)?;
        d.map(data_de).transpose()
    }

    /// Data mais antiga já pedida com sucesso para a série (`cdi`, `selic`, `ipca`).
    pub fn cobertura_desde(&self, serie: &str) -> Result<Option<NaiveDate>, CoreError> {
        let d: Option<String> = self
            .conn
            .query_row(
                "SELECT desde FROM cobertura_serie WHERE serie = ?1",
                [serie],
                |r| r.get(0),
            )
            .optional()
            .map_err(classifica)?;
        d.map(data_de).transpose()
    }

    /// Guarda `desde` se for mais antiga que a já registrada.
    pub fn gravar_cobertura(&self, serie: &str, desde: NaiveDate) -> Result<(), CoreError> {
        self.conn
            .execute(
                "INSERT INTO cobertura_serie (serie, desde) VALUES (?1, ?2) \
                 ON CONFLICT (serie) DO UPDATE SET desde = MIN(desde, excluded.desde)",
                params![serie, iso(desde)],
            )
            .map_err(classifica)?;
        Ok(())
    }

    /// Guarda `data` como a marca da chave, mesmo que seja mais nova que a registrada (a
    /// `gravar_cobertura` mantém a mais antiga; estas marcas dizem "procurado em", não "desde").
    pub fn gravar_marca_recente(&self, chave: &str, data: NaiveDate) -> Result<(), CoreError> {
        self.conn
            .execute(
                "INSERT INTO cobertura_serie (serie, desde) VALUES (?1, ?2) \
                 ON CONFLICT (serie) DO UPDATE SET desde = MAX(desde, excluded.desde)",
                params![chave, iso(data)],
            )
            .map_err(classifica)?;
        Ok(())
    }

    pub fn b3_alvos(&self) -> Result<Vec<B3Alvo>, CoreError> {
        let mut st = self
            .conn
            .prepare("SELECT alvo, pregao, tickers FROM b3_alvo ORDER BY alvo")
            .map_err(classifica)?;
        let linhas = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(classifica)?;
        let mut out = Vec::new();
        for l in linhas {
            let (alvo, pregao, tickers) = l.map_err(classifica)?;
            out.push(B3Alvo {
                alvo: data_de(alvo)?,
                pregao: pregao.map(data_de).transpose()?,
                tickers: tickers
                    .split(',')
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
                    .collect(),
            });
        }
        Ok(out)
    }

    pub fn gravar_b3_alvo(&self, a: &B3Alvo) -> Result<(), CoreError> {
        self.conn
            .execute(
                "INSERT INTO b3_alvo (alvo, pregao, tickers) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (alvo) DO UPDATE SET pregao = excluded.pregao, \
                 tickers = excluded.tickers",
                params![iso(a.alvo), a.pregao.map(iso), a.tickers.join(",")],
            )
            .map_err(classifica)?;
        Ok(())
    }

    pub fn ultima_data_indice(&self, serie: &str) -> Result<Option<NaiveDate>, CoreError> {
        let d: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(data) FROM indice_diario WHERE serie = ?1",
                [serie],
                |r| r.get(0),
            )
            .map_err(classifica)?;
        d.map(data_de).transpose()
    }

    /// Meses (`YYYY-MM`) com `de_mes <= mes <= ate_mes`, em ordem.
    pub fn ipca_meses(
        &self,
        de_mes: &str,
        ate_mes: &str,
    ) -> Result<Vec<(String, Decimal)>, CoreError> {
        let mut st = self
            .conn
            .prepare(
                "SELECT mes, valor FROM ipca_mensal WHERE mes >= ?1 AND mes <= ?2 ORDER BY mes",
            )
            .map_err(classifica)?;
        let linhas = st
            .query_map(params![de_mes, ate_mes], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(classifica)?;
        let mut out = Vec::new();
        for l in linhas {
            let (m, v) = l.map_err(classifica)?;
            out.push((m, decimal_de(v)?));
        }
        Ok(out)
    }

    /// Último preço do título com data `<= data`.
    pub fn preco_tesouro_em(
        &self,
        tipo: &str,
        vencimento: NaiveDate,
        data: NaiveDate,
    ) -> Result<Option<PrecoTesouro>, CoreError> {
        let linha: Option<(String, String, String, String)> = self
            .conn
            .query_row(
                "SELECT data, taxa_compra, pu_compra, pu_base FROM tesouro_preco \
                 WHERE tipo = ?1 AND vencimento = ?2 AND data <= ?3 \
                 ORDER BY data DESC LIMIT 1",
                params![tipo, iso(vencimento), iso(data)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(classifica)?;
        linha
            .map(|(d, taxa, compra, base)| {
                Ok(PrecoTesouro {
                    tipo: tipo.to_string(),
                    vencimento,
                    data: data_de(d)?,
                    taxa_compra: decimal_de(taxa)?,
                    pu_compra: decimal_de(compra)?,
                    pu_base: decimal_de(base)?,
                })
            })
            .transpose()
    }

    /// Última cotação do ticker com data `<= data`.
    pub fn cotacao_em(&self, ticker: &str, data: NaiveDate) -> Result<Option<Cotacao>, CoreError> {
        let linha: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT data, fechamento_centavos FROM cotacao \
                 WHERE ticker = ?1 AND data <= ?2 ORDER BY data DESC LIMIT 1",
                params![ticker, iso(data)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(classifica)?;
        linha
            .map(|(d, c)| {
                Ok(Cotacao {
                    ticker: ticker.to_string(),
                    data: data_de(d)?,
                    fechamento_centavos: c,
                })
            })
            .transpose()
    }

    /// Datas de pregão com cotação guardada, em ordem.
    pub fn datas_cotacao(&self) -> Result<Vec<NaiveDate>, CoreError> {
        let mut st = self
            .conn
            .prepare("SELECT DISTINCT data FROM cotacao ORDER BY data")
            .map_err(classifica)?;
        let linhas = st
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(classifica)?;
        let mut out = Vec::new();
        for l in linhas {
            out.push(data_de(l.map_err(classifica)?)?);
        }
        Ok(out)
    }

    pub fn registrar_status(&self, s: &StatusFonte) -> Result<(), CoreError> {
        self.conn
            .execute(
                "INSERT INTO fonte_status (fonte, ultima_data, atualizado_em, erro) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT (fonte) DO UPDATE SET ultima_data = excluded.ultima_data, \
                 atualizado_em = excluded.atualizado_em, erro = excluded.erro",
                params![s.fonte, s.ultima_data, s.atualizado_em, s.erro],
            )
            .map_err(classifica)?;
        Ok(())
    }

    /// As seis fontes, na ordem fixa, mesmo as nunca atualizadas.
    pub fn status_fontes(&self) -> Result<Vec<StatusFonte>, CoreError> {
        let mut out = Vec::new();
        for fonte in FONTES {
            let s = self
                .conn
                .query_row(
                    "SELECT ultima_data, atualizado_em, erro FROM fonte_status WHERE fonte = ?1",
                    [fonte],
                    |r| {
                        Ok(StatusFonte {
                            fonte: fonte.to_string(),
                            ultima_data: r.get(0)?,
                            atualizado_em: r.get(1)?,
                            erro: r.get(2)?,
                        })
                    },
                )
                .optional()
                .map_err(classifica)?
                .unwrap_or(StatusFonte {
                    fonte: fonte.to_string(),
                    ultima_data: None,
                    atualizado_em: None,
                    erro: None,
                });
            out.push(s);
        }
        Ok(out)
    }
}
