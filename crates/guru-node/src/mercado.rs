//! Tipos da ponte para os dados de mercado e a tarefa assíncrona de atualização.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::NaiveDate;
use guru_core::{BaixadorHttp, BaixadorPasta, CoreError, Db, ResultadoAtualizacao, StatusFonte};
use napi::JsValue;
use napi::bindgen_prelude::{AsyncTask, Env, Task};
use napi_derive::napi;

/// Modo de teste de `Core.open`: o main só passa isto com `GURU_MERCADO_FIXTURES` e fora do app
/// empacotado. `hoje` (`YYYY-MM-DD`) só vale junto com `pasta_fixtures`.
#[napi(object)]
pub struct OpcoesCore {
    pub pasta_fixtures: Option<String>,
    pub hoje: Option<String>,
    /// Só teste: pasta com respostas `.sse` no lugar da API da Anthropic.
    pub pasta_ia: Option<String>,
    /// Só teste: cofre da chave em memória no lugar do cofre do sistema.
    pub cofre_memoria: Option<bool>,
}

#[napi(object)]
pub struct StatusFonteJs {
    pub fonte: String,
    pub rotulo: String,
    pub ultima_data: Option<String>,
    pub atualizado_em: Option<String>,
    pub erro: Option<String>,
}

#[napi(object)]
pub struct ResultadoMercadoJs {
    pub fontes: Vec<StatusFonteJs>,
    pub novos: u32,
}

/// Modo de teste já validado: pasta de arquivos no lugar da rede e data fixa no lugar de "hoje".
#[derive(Clone)]
pub struct ModoTeste {
    pub pasta: PathBuf,
    pub hoje: Option<NaiveDate>,
}

pub fn modo_teste(opcoes: Option<&OpcoesCore>) -> napi::Result<Option<ModoTeste>, String> {
    let Some(o) = opcoes else { return Ok(None) };
    let invalido = |m: &str| napi::Error::new("MERCADO_CONFIG_INVALIDA".to_string(), m.to_string());
    match (o.pasta_fixtures.clone(), o.hoje.clone()) {
        (None, None) => Ok(None),
        (None, Some(_)) => Err(invalido("`hoje` só vale junto com `pastaFixtures`.")),
        (Some(pasta), _) if pasta.trim().is_empty() => {
            Err(invalido("`pastaFixtures` não pode ser vazia."))
        }
        (Some(pasta), hoje) => {
            let hoje = hoje
                .map(|h| NaiveDate::parse_from_str(&h, "%Y-%m-%d"))
                .transpose()
                .map_err(|_| invalido("`hoje` deve estar no formato AAAA-MM-DD."))?;
            Ok(Some(ModoTeste {
                pasta: PathBuf::from(pasta),
                hoje,
            }))
        }
    }
}

fn rotulo(fonte: &str) -> &'static str {
    match fonte {
        "cdi" => "CDI",
        "selic" => "Selic",
        "ipca" => "IPCA",
        "tesouro" => "Tesouro Direto",
        "b3" => "Cotações da B3",
        "cvm" => "Cotas de fundos (CVM)",
        _ => "Fonte desconhecida",
    }
}

pub fn status(lista: Vec<StatusFonte>) -> Vec<StatusFonteJs> {
    lista
        .into_iter()
        .map(|s| StatusFonteJs {
            rotulo: rotulo(&s.fonte).to_string(),
            fonte: s.fonte,
            ultima_data: s.ultima_data,
            atualizado_em: s.atualizado_em,
            erro: s.erro,
        })
        .collect()
}

fn resultado(r: ResultadoAtualizacao) -> ResultadoMercadoJs {
    ResultadoMercadoJs {
        fontes: status(r.fontes),
        novos: u32::try_from(r.novos).unwrap_or(u32::MAX),
    }
}

/// Atualização na threadpool do libuv, sem travar o processo principal.
///
/// Escolha: a tarefa abre a sua própria conexão (`Db::open`, com busy timeout de 5 s) em vez de
/// usar a `Db` atrás do `Mutex` de `Core`. Assim o `Mutex` nunca fica travado durante os downloads
/// (que levam segundos) e as demais chamadas da interface seguem respondendo; o SQLite serializa
/// as gravações das duas conexões. `atualizar` mantém a assinatura.
pub struct AtualizarMercado {
    pub pasta_dados: PathBuf,
    pub modo_teste: Option<ModoTeste>,
    pub hoje: NaiveDate,
}

impl Task for AtualizarMercado {
    type Output = Result<ResultadoAtualizacao, (String, String)>;
    type JsValue = ResultadoMercadoJs;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        Ok(self
            .executar()
            .map_err(|e| (e.code().to_string(), e.to_string())))
    }

    // `Task` só rejeita com `Error<Status>`; para o `code` do núcleo (SCREAMING_SNAKE_CASE)
    // chegar à ponte, a rejeição é montada aqui como um `Error` de JS com `code`.
    fn resolve(&mut self, env: Env, saida: Self::Output) -> napi::Result<Self::JsValue> {
        match saida {
            Ok(r) => Ok(resultado(r)),
            Err((code, mensagem)) => {
                let mut erro =
                    env.create_error(napi::Error::new(napi::Status::GenericFailure, mensagem))?;
                erro.set("code", code)?;
                Err(napi::Error::from(erro.to_unknown()))
            }
        }
    }
}

impl AtualizarMercado {
    fn executar(&self) -> Result<ResultadoAtualizacao, CoreError> {
        let db = Db::open(Path::new(&self.pasta_dados))?;
        let espera = |d: Duration| std::thread::sleep(d);
        match &self.modo_teste {
            // Nos testes não há o que esperar entre tentativas.
            Some(m) => guru_core::atualizar(
                &db,
                &BaixadorPasta {
                    pasta: m.pasta.clone(),
                },
                self.hoje,
                &|_| {},
            ),
            None => guru_core::atualizar(&db, &BaixadorHttp::novo(), self.hoje, &espera),
        }
    }
}

pub fn tarefa(
    pasta_dados: PathBuf,
    modo_teste: Option<ModoTeste>,
    hoje: NaiveDate,
) -> AsyncTask<AtualizarMercado> {
    AsyncTask::new(AtualizarMercado {
        pasta_dados,
        modo_teste,
        hoje,
    })
}
