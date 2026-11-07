//! Montagem do contexto (carteira, perfil, metas e mercado) que vai para a IA, com as chaves
//! de privacidade e as partes que o usuário liga, desliga ou edita.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use rusqlite::params;

use crate::db::{Db, classifica};
use crate::error::CoreError;
use crate::investimento::{Classe, ErroCampo, Investimento};
use crate::metas::{Meta, formatar_reais};
use crate::painel::{Referencia, acumulado_benchmark, datas_painel};

use super::config::ConfigIa;

const MAX_TEXTO_EDITADO: usize = 20_000;
const DIAS_PARA_ANTIGO: i64 = 7;
/// Quantas notícias guardadas vão no fim da parte "Mercado".
const NOTICIAS_NO_MERCADO: usize = 3;

/// (código, rótulo), na ordem em que entram no contexto.
const PARTES: [(&str, &str); 5] = [
    ("instrucoes", "Suas instruções"),
    ("perfil", "Perfil e objetivo"),
    ("carteira", "Carteira"),
    ("mercado", "Mercado"),
    ("metas", "Metas"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParteContexto {
    pub parte: &'static str,
    pub rotulo: &'static str,
    pub ligada: bool,
    pub editado: bool,
    /// O texto que iria, mesmo com a parte desligada.
    pub texto: String,
    /// A linha cinza do bloco, como "automático · 27 investimentos".
    pub resumo: String,
    pub tokens: u32,
    pub data_dados: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FonteInterna {
    pub parte: String,
    pub rotulo: String,
    pub data_dados: Option<NaiveDate>,
    pub antigo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextoMontado {
    pub texto: String,
    pub fontes_internas: Vec<FonteInterna>,
    pub tokens: u32,
}

/// `ceil(caracteres / 4)`.
pub fn estimar_tokens(texto: &str) -> u32 {
    texto.chars().count().div_ceil(4) as u32
}

fn data_br(d: NaiveDate) -> String {
    format!("{:02}/{:02}/{}", d.day(), d.month(), d.year())
}

fn data_iso(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn data_iso_br(s: &str) -> String {
    data_iso(s).map_or_else(|| s.to_string(), data_br)
}

/// Décimos de ponto percentual: `365` vira `36,5%`.
pub(crate) fn pct_decimos(d: i64) -> String {
    let sinal = if d < 0 { "-" } else { "" };
    let a = d.unsigned_abs();
    format!("{sinal}{},{}%", a / 10, a % 10)
}

/// Centésimos de ponto percentual: `1234` vira `12,34%`.
fn pct_centesimos(c: i64) -> String {
    let sinal = if c < 0 { "-" } else { "" };
    let a = c.unsigned_abs();
    format!("{sinal}{},{:02}%", a / 100, a % 100)
}

fn decimos(parte: i64, total: i64) -> i64 {
    if total <= 0 {
        return 0;
    }
    let (p, t) = (i128::from(parte), i128::from(total));
    ((p * 2000 + t) / (2 * t)) as i64
}

struct Gerada {
    texto: String,
    resumo: String,
    data_dados: Option<NaiveDate>,
}

fn plural(n: usize, singular: &str, plural: &str) -> String {
    format!("{n} {}", if n == 1 { singular } else { plural })
}

/// Uma meta numa linha, sem o marcador: "Nome · prazo 12/2030 · 27,5% atingido", com os valores
/// em reais só se "Apenas percentuais" estiver desligada.
pub(crate) fn linha_meta(m: &Meta, cfg: &ConfigIa) -> String {
    let mut p = vec![m.nome.clone()];
    let prazo = m
        .prazo
        .split_once('-')
        .map_or_else(|| m.prazo.clone(), |(a, mes)| format!("{mes}/{a}"));
    p.push(format!("prazo {prazo}"));
    p.push(format!("{} atingido", pct_decimos(m.percentual_decimos)));
    if !cfg.apenas_percentuais {
        p.push(format!("alvo {}", formatar_reais(m.valor_alvo_centavos)));
        p.push(format!(
            "acumulado {}",
            formatar_reais(m.acumulado_centavos)
        ));
        if let Some(a) = m.aporte_necessario_centavos {
            p.push(format!("aporte mensal necessário {}", formatar_reais(a)));
        }
    }
    p.join(" · ")
}

impl Db {
    fn parte_perfil(&self, hoje: NaiveDate) -> Result<Gerada, CoreError> {
        let mut linhas = Vec::new();
        let mut data_dados = None;
        match self.perfil_atual(hoje)? {
            Some(p) => {
                data_dados = data_iso(&p.feito_em);
                let vencido = if p.vencido { ", já vencido" } else { "" };
                linhas.push(format!(
                    "Perfil de investidor: {} (teste feito em {}{vencido}).",
                    p.perfil.rotulo(),
                    data_iso_br(&p.feito_em)
                ));
            }
            None => linhas.push("Perfil de investidor: ainda não definido.".into()),
        }
        match self.alocacao_alvo()? {
            Some(alvo) => {
                let itens: Vec<String> = alvo
                    .iter()
                    .map(|(c, pct)| format!("{} {pct}%", c.rotulo()))
                    .collect();
                linhas.push(format!("Alocação-alvo: {}.", itens.join(", ")));
            }
            None => linhas.push("Alocação-alvo: não definida.".into()),
        }
        match self.objetivo()? {
            Some(o) if !o.trim().is_empty() => linhas.push(format!("Objetivo: {}", o.trim())),
            _ => linhas.push("Objetivo: não informado.".into()),
        }
        Ok(Gerada {
            texto: linhas.join("\n"),
            resumo: "automático".into(),
            data_dados,
        })
    }

    fn linha_investimento(i: &Investimento, total: i64, cfg: &ConfigIa) -> String {
        let mut p = vec![i.tipo_rotulo.clone()];
        if let Some(t) = &i.ticker {
            p.push(t.clone());
        } else if let Some(r) = &i.rendimento {
            p.push(r.clone());
        }
        if cfg.nomes_instituicoes {
            let mut onde = format!("instituição {}", i.instituicao);
            if let Some(e) = i.emissor.as_deref().filter(|e| *e != i.instituicao) {
                onde.push_str(&format!(", emissor {e}"));
            }
            p.push(onde);
        }
        if let Some(v) = &i.vencimento {
            p.push(format!("vence em {}", data_iso_br(v)));
        }
        if let Some(l) = i.liquidez {
            p.push(format!("liquidez: {}", l.rotulo()));
        }
        p.push(format!(
            "{} da carteira",
            pct_decimos(decimos(i.valor_atual_centavos, total))
        ));
        if !cfg.apenas_percentuais {
            p.push(format!(
                "valor atual {}",
                formatar_reais(i.valor_atual_centavos)
            ));
        }
        if let Some(d) = i.data_valor {
            p.push(format!("valores de {}", data_br(d)));
        }
        format!("- {}", p.join(" · "))
    }

    fn parte_carteira(&self, hoje: NaiveDate, cfg: &ConfigIa) -> Result<Gerada, CoreError> {
        let lista = self.listar_investimentos(hoje)?;
        let resumo = self.resumo_carteira(hoje)?;
        let mut linhas = Vec::new();
        if lista.is_empty() {
            linhas.push("Nenhum investimento cadastrado.".to_string());
        } else {
            linhas.push("Classes:".to_string());
            for c in &resumo.classes {
                let mut l = format!(
                    "- {}: {} da carteira",
                    c.classe.rotulo(),
                    pct_decimos(c.percentual_decimos)
                );
                if let Some(a) = c.alvo_percentual {
                    l.push_str(&format!(" (alvo {a}%)"));
                }
                if !cfg.apenas_percentuais {
                    l.push_str(&format!(", {}", formatar_reais(c.total_atual_centavos)));
                }
                linhas.push(l);
            }
            if !cfg.apenas_percentuais {
                linhas.push(format!(
                    "Total atual: {}.",
                    formatar_reais(resumo.total_atual_centavos)
                ));
            }
            linhas.push("Investimentos:".to_string());
            for i in &lista {
                linhas.push(Self::linha_investimento(
                    i,
                    resumo.total_atual_centavos,
                    cfg,
                ));
            }
        }
        Ok(Gerada {
            texto: linhas.join("\n"),
            resumo: format!(
                "automático · {}",
                plural(lista.len(), "investimento", "investimentos")
            ),
            data_dados: lista.iter().filter_map(|i| i.data_valor).max(),
        })
    }

    /// Com a parte "Carteira" desligada (`carteira_ligada` falso), as notícias com um ticker
    /// na etiqueta ficam de fora; as de uma classe ou de "Mercado" continuam.
    fn parte_mercado(&self, hoje: NaiveDate, carteira_ligada: bool) -> Result<Gerada, CoreError> {
        let datas = datas_painel(hoje, 12)?;
        let mut acumulados = Vec::new();
        for r in [Referencia::Cdi, Referencia::Selic, Referencia::Ipca] {
            let valor = acumulado_benchmark(self, r, &datas)?
                .and_then(|v| v.last().copied().flatten())
                .map_or_else(|| "sem dado".to_string(), pct_centesimos);
            acumulados.push(format!("{} {valor}", r.rotulo()));
        }
        let mut linhas = vec![format!("Acumulado em 12 meses: {}.", acumulados.join("; "))];
        for ticker in ["BOVA11", "XFIX11"] {
            linhas.push(match self.cotacao_em(ticker, hoje)? {
                Some(c) => format!(
                    "Última cotação de {ticker}: {},{:02} reais em {}.",
                    c.fechamento_centavos / 100,
                    c.fechamento_centavos % 100,
                    data_br(c.data)
                ),
                None => format!("Última cotação de {ticker}: sem dado."),
            });
        }
        let mut datas_fontes = Vec::new();
        let mut recentes = Vec::new();
        for s in self.status_fontes()? {
            let rotulo = match s.fonte.as_str() {
                "cdi" => "CDI",
                "selic" => "Selic",
                "ipca" => "IPCA",
                "tesouro" => "Tesouro Direto",
                "b3" => "B3",
                "cvm" => "CVM (cotas de fundos)",
                outro => outro,
            };
            let data = s.ultima_data.as_deref().and_then(data_iso);
            datas_fontes.extend(data);
            recentes.push(format!(
                "{rotulo} {}",
                data.map_or_else(|| "sem dado".to_string(), data_br)
            ));
        }
        linhas.push(format!(
            "Dado mais recente de cada fonte: {}.",
            recentes.join("; ")
        ));
        let mut resumo = "automático".to_string();
        let sem_ticker = |etiqueta: &str| {
            etiqueta == "Mercado" || Classe::TODOS.iter().any(|c| c.rotulo() == etiqueta)
        };
        let noticias = self.noticias_guardadas()?;
        let primeiras: Vec<_> = noticias
            .iter()
            .flat_map(|g| g.itens.iter())
            .filter(|n| carteira_ligada || sem_ticker(&n.etiqueta))
            .take(NOTICIAS_NO_MERCADO)
            .collect();
        if let Some(g) = &noticias
            && !primeiras.is_empty()
        {
            linhas.push(format!(
                "Notícias da carteira (buscadas em {}):",
                data_br(g.gerado_em)
            ));
            for n in &primeiras {
                let onde = match &n.data {
                    Some(d) => format!("{}, {d}", n.dominio),
                    None => n.dominio.clone(),
                };
                linhas.push(format!("- {}: {} ({onde})", n.etiqueta, n.titulo));
            }
            resumo = format!(
                "automático · {}",
                plural(primeiras.len(), "notícia", "notícias")
            );
        }
        Ok(Gerada {
            texto: linhas.join("\n"),
            resumo,
            data_dados: datas_fontes.into_iter().max(),
        })
    }

    fn parte_metas(&self, hoje: NaiveDate, cfg: &ConfigIa) -> Result<Gerada, CoreError> {
        let metas = self.listar_metas(hoje)?;
        let mut linhas = Vec::new();
        if metas.is_empty() {
            linhas.push("Nenhuma meta cadastrada.".to_string());
        }
        for m in &metas {
            linhas.push(format!("- {}", linha_meta(m, cfg)));
        }
        Ok(Gerada {
            texto: linhas.join("\n"),
            resumo: format!("automático · {}", plural(metas.len(), "meta", "metas")),
            data_dados: None,
        })
    }

    pub(crate) fn edicoes_de_partes(
        &self,
    ) -> Result<HashMap<String, (bool, Option<String>)>, CoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT parte, ligada, texto_editado FROM ia_contexto_parte")
            .map_err(classifica)?;
        let linhas = stmt
            .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))
            .map_err(classifica)?
            .collect::<Result<HashMap<_, _>, _>>()
            .map_err(classifica)?;
        Ok(linhas)
    }

    /// As cinco partes, na ordem do contexto, com o texto que iria (editado ou automático).
    pub fn partes_contexto(&self, hoje: NaiveDate) -> Result<Vec<ParteContexto>, CoreError> {
        let cfg = self.config_ia()?;
        let edicoes = self.edicoes_de_partes()?;
        let carteira_ligada = edicoes.get("carteira").is_none_or(|(l, _)| *l);
        let mut out = Vec::new();
        for (codigo, rotulo) in PARTES {
            let gerada = match codigo {
                "instrucoes" => Gerada {
                    texto: cfg.instrucoes.clone(),
                    resumo: "campo de instruções".into(),
                    data_dados: None,
                },
                "perfil" => self.parte_perfil(hoje)?,
                "carteira" => self.parte_carteira(hoje, &cfg)?,
                "mercado" => self.parte_mercado(hoje, carteira_ligada)?,
                _ => self.parte_metas(hoje, &cfg)?,
            };
            let (ligada, editado) = edicoes
                .get(codigo)
                .map_or((true, None), |(l, t)| (*l, t.clone()));
            // As instruções nunca têm texto editado: edita-se o campo.
            let editado = editado.filter(|_| codigo != "instrucoes");
            let foi_editado = editado.is_some();
            let (texto, resumo) = match editado {
                Some(t) => (t, "editado por você".to_string()),
                None => (gerada.texto, gerada.resumo),
            };
            out.push(ParteContexto {
                parte: codigo,
                rotulo,
                ligada,
                editado: foi_editado,
                tokens: estimar_tokens(&texto),
                texto,
                resumo,
                data_dados: gerada.data_dados,
            });
        }
        Ok(out)
    }

    /// Liga ou desliga a parte e guarda (ou, com `None`, apaga) o texto editado.
    pub fn salvar_parte_contexto(
        &self,
        parte: &str,
        ligada: bool,
        texto_editado: Option<&str>,
    ) -> Result<Vec<ErroCampo>, CoreError> {
        if !PARTES.iter().any(|(c, _)| *c == parte) {
            return Ok(vec![ErroCampo {
                campo: "parte".into(),
                mensagem: "Parte do contexto desconhecida.".into(),
            }]);
        }
        let texto_editado = texto_editado.filter(|_| parte != "instrucoes");
        if texto_editado.is_some_and(|t| t.chars().count() > MAX_TEXTO_EDITADO) {
            return Ok(vec![ErroCampo {
                campo: "textoEditado".into(),
                mensagem: "Use no máximo 20.000 caracteres.".into(),
            }]);
        }
        self.conn
            .execute(
                "INSERT INTO ia_contexto_parte (parte, ligada, texto_editado) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (parte) DO UPDATE SET ligada = excluded.ligada, \
                 texto_editado = excluded.texto_editado",
                params![parte, ligada, texto_editado],
            )
            .map_err(classifica)?;
        Ok(Vec::new())
    }

    /// O contexto reduzido das notícias: só a lista de ativos, sem valor nem `nome`, as
    /// instruções (se ligadas e não vazias) e a data. Instituição e emissor só com "Nome das
    /// instituições"; com a parte "Carteira" desligada, só as classes, sem tickers.
    pub fn contexto_noticias(&self, hoje: NaiveDate) -> Result<String, CoreError> {
        let cfg = self.config_ia()?;
        let edicoes = self.edicoes_de_partes()?;
        let ligada = |parte: &str| edicoes.get(parte).is_none_or(|(l, _)| *l);
        let lista = self.listar_investimentos(hoje)?;
        let presentes: Vec<Classe> = Classe::TODOS
            .iter()
            .copied()
            .filter(|c| lista.iter().any(|i| i.classe == *c))
            .collect();
        let mut ativos = Vec::new();
        if lista.is_empty() {
            ativos.push("Nenhum investimento cadastrado.".to_string());
        } else if !ligada("carteira") {
            let rotulos: Vec<&str> = presentes.iter().map(|c| c.rotulo()).collect();
            ativos.push(format!("Classes na carteira: {}.", rotulos.join(", ")));
        } else {
            for c in &presentes {
                let mut itens: Vec<String> = Vec::new();
                for i in lista.iter().filter(|i| i.classe == *c) {
                    let item = Self::ativo_noticia(i, &cfg);
                    if !itens.contains(&item) {
                        itens.push(item);
                    }
                }
                ativos.push(format!("{}: {}.", c.rotulo(), itens.join(", ")));
            }
        }
        let mut blocos = vec![format!("## Ativos da carteira\n{}", ativos.join("\n"))];
        if ligada("instrucoes") && !cfg.instrucoes.trim().is_empty() {
            blocos.push(format!("## Suas instruções\n{}", cfg.instrucoes));
        }
        blocos.push(format!("Hoje é {}.", data_br(hoje)));
        Ok(blocos.join("\n\n"))
    }

    /// O ticker, ou o tipo com o indexador ("CDB 102% do CDI"); a instituição e o emissor só
    /// com "Nome das instituições".
    fn ativo_noticia(i: &Investimento, cfg: &ConfigIa) -> String {
        if let Some(t) = &i.ticker {
            return t.clone();
        }
        let mut item = match &i.rendimento {
            Some(r) => format!("{} {r}", i.tipo_rotulo),
            None => i.tipo_rotulo.clone(),
        };
        if cfg.nomes_instituicoes {
            let mut onde = format!("instituição {}", i.instituicao);
            if let Some(e) = i.emissor.as_deref().filter(|e| *e != i.instituicao) {
                onde.push_str(&format!(", emissor {e}"));
            }
            item.push_str(&format!(" ({onde})"));
        }
        item
    }

    /// Só as partes ligadas (e com texto), cada uma sob `## <rótulo>`, e a data de hoje no fim.
    pub fn contexto_montado(&self, hoje: NaiveDate) -> Result<ContextoMontado, CoreError> {
        self.contexto_com(hoje, None)
    }

    /// O `contexto_montado` com um bloco a mais antes de "Hoje é" e a fonte dele no fim.
    pub(crate) fn contexto_com(
        &self,
        hoje: NaiveDate,
        extra: Option<(String, FonteInterna)>,
    ) -> Result<ContextoMontado, CoreError> {
        let mut blocos = Vec::new();
        let mut fontes = Vec::new();
        for p in self.partes_contexto(hoje)? {
            if !p.ligada || p.texto.trim().is_empty() {
                continue;
            }
            blocos.push(format!("## {}\n{}", p.rotulo, p.texto));
            fontes.push(FonteInterna {
                parte: p.parte.to_string(),
                rotulo: p.rotulo.to_string(),
                data_dados: p.data_dados,
                antigo: p.parte == "mercado"
                    && p.data_dados
                        .is_some_and(|d| (hoje - d).num_days() > DIAS_PARA_ANTIGO),
            });
        }
        if let Some((bloco, fonte)) = extra {
            blocos.push(bloco);
            fontes.push(fonte);
        }
        blocos.push(format!("Hoje é {}.", data_br(hoje)));
        let texto = blocos.join("\n\n");
        Ok(ContextoMontado {
            tokens: estimar_tokens(&texto),
            texto,
            fontes_internas: fontes,
        })
    }
}
