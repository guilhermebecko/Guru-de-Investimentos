//! Leitura do streaming SSE da Messages API e o acumulador, que remonta os blocos completos
//! (para reenviar no histórico sem tirar nada) e numera as fontes da web.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::cliente::ErroIa;

/// Junta os pedaços do corpo em linhas e parseia só as linhas `data:`. Linhas `event:`,
/// comentários e linhas vazias são ignorados; um `data:` com JSON inválido é `Indisponivel`.
#[derive(Default)]
pub(crate) struct LeitorSse {
    pendente: Vec<u8>,
    /// Até onde `pendente` já foi procurado sem achar fim de linha.
    procurado: usize,
}

impl LeitorSse {
    pub(crate) fn alimentar(&mut self, bytes: &[u8], saida: &mut Vec<Value>) -> Result<(), ErroIa> {
        self.pendente.extend_from_slice(bytes);
        let mut inicio = 0;
        let mut i = self.procurado;
        while let Some(p) = self.pendente[i..].iter().position(|&b| b == b'\n') {
            let fim = i + p;
            let resultado = linha_sse(&self.pendente[inicio..fim], saida);
            inicio = fim + 1;
            i = inicio;
            if resultado.is_err() {
                self.pendente.drain(..inicio);
                self.procurado = 0;
                return resultado;
            }
        }
        self.pendente.drain(..inicio);
        self.procurado = self.pendente.len();
        Ok(())
    }

    /// O resto sem `\n` no fim do corpo ainda é uma linha.
    pub(crate) fn terminar(&mut self, saida: &mut Vec<Value>) -> Result<(), ErroIa> {
        let resto = std::mem::take(&mut self.pendente);
        self.procurado = 0;
        if resto.is_empty() {
            Ok(())
        } else {
            linha_sse(&resto, saida)
        }
    }
}

fn linha_sse(linha: &[u8], saida: &mut Vec<Value>) -> Result<(), ErroIa> {
    let linha = linha.strip_suffix(b"\r").unwrap_or(linha);
    let Some(dados) = linha.strip_prefix(b"data:") else {
        return Ok(());
    };
    let dados = dados.strip_prefix(b" ").unwrap_or(dados);
    let evento = serde_json::from_slice(dados).map_err(|_| ErroIa::Indisponivel)?;
    saida.push(evento);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fonte {
    pub numero: u32,
    pub url: String,
    pub titulo: String,
    pub dominio: String,
    /// O `page_age` do resultado da busca com a mesma URL, como veio.
    pub data: Option<String>,
    /// O trecho da primeira citação da URL.
    pub trecho: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Progresso {
    pub texto: String,
    pub fontes: Vec<Fonte>,
    pub buscando: bool,
    pub terminado: bool,
}

/// Tokens de uma resposta (somados entre as partes de um `pause_turn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Uso {
    pub entrada: i64,
    pub saida: i64,
    pub cache_leitura: i64,
    pub cache_escrita: i64,
}

impl Uso {
    fn atualizar(&mut self, u: &Value) {
        let campos = [
            ("input_tokens", &mut self.entrada),
            ("output_tokens", &mut self.saida),
            ("cache_read_input_tokens", &mut self.cache_leitura),
            ("cache_creation_input_tokens", &mut self.cache_escrita),
        ];
        for (nome, alvo) in campos {
            if let Some(n) = u[nome].as_i64() {
                *alvo = n;
            }
        }
    }
}

struct Citada {
    url: String,
    titulo: String,
    trecho: String,
}

/// Remonta a resposta evento a evento. Aceita várias mensagens seguidas (as partes de um
/// `pause_turn`): os blocos de cada uma vão para o fim da lista, e a numeração das fontes continua.
#[derive(Default)]
pub struct Acumulador {
    blocos: Vec<Value>,
    /// Posição do primeiro bloco da mensagem atual (os `index` recomeçam em cada mensagem).
    base: usize,
    json_parcial: HashMap<usize, String>,
    /// Números das fontes citadas em cada bloco de texto ainda aberto, sem repetir.
    marcadores: HashMap<usize, Vec<u32>>,
    texto: String,
    citadas: Vec<Citada>,
    idades: HashMap<String, String>,
    stop_reason: Option<String>,
    parou: bool,
    buscando: bool,
    usos: Vec<Uso>,
}

fn anexar(bloco: &mut Value, campo: &str, pedaco: &str) {
    if let Some(obj) = bloco.as_object_mut() {
        let atual = obj
            .entry(campo)
            .or_insert_with(|| Value::String(String::new()));
        match atual {
            Value::String(s) => s.push_str(pedaco),
            outro => *outro = Value::String(pedaco.to_string()),
        }
    }
}

/// `https://www.bcb.gov.br/x` vira `bcb.gov.br`.
fn dominio(url: &str) -> String {
    let resto = url.split_once("://").map_or(url, |(_, r)| r);
    let host = resto.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.split(':').next().unwrap_or(host);
    let host = host.to_lowercase();
    host.strip_prefix("www.")
        .map(str::to_string)
        .unwrap_or(host)
}

impl Acumulador {
    pub fn novo() -> Self {
        Self::default()
    }

    pub fn evento(&mut self, e: &Value) -> Result<(), ErroIa> {
        match e["type"].as_str().unwrap_or("") {
            "message_start" => {
                self.base = self.blocos.len();
                self.stop_reason = None;
                self.parou = false;
                let mut uso = Uso::default();
                uso.atualizar(&e["message"]["usage"]);
                self.usos.push(uso);
            }
            "content_block_start" => self.inicio_bloco(e)?,
            "content_block_delta" => self.delta(e),
            "content_block_stop" => self.fim_bloco(e)?,
            "message_delta" => {
                if let Some(s) = e["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(s.to_string());
                }
                if let Some(uso) = self.usos.last_mut() {
                    uso.atualizar(&e["usage"]);
                }
            }
            "message_stop" => {
                self.parou = true;
                self.buscando = false;
            }
            "error" => return Err(ErroIa::Indisponivel),
            // `ping` e eventos novos que este app ainda não conhece.
            _ => {}
        }
        Ok(())
    }

    fn posicao(&self, e: &Value) -> Option<usize> {
        e["index"]
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .map(|i| self.base + i)
    }

    fn inicio_bloco(&mut self, e: &Value) -> Result<(), ErroIa> {
        let pos = self.posicao(e).ok_or(ErroIa::Indisponivel)?;
        if pos != self.blocos.len() || !e["content_block"].is_object() {
            return Err(ErroIa::Indisponivel);
        }
        let bloco = e["content_block"].clone();
        match bloco["type"].as_str().unwrap_or("") {
            "server_tool_use" => {
                self.buscando = true;
                self.json_parcial.insert(pos, String::new());
            }
            "web_search_tool_result" => {
                self.buscando = false;
                if let Some(resultados) = bloco["content"].as_array() {
                    for r in resultados {
                        if let (Some(url), Some(idade)) =
                            (r["url"].as_str(), r["page_age"].as_str())
                        {
                            self.idades.insert(url.to_string(), idade.to_string());
                        }
                    }
                }
            }
            "text" => self.buscando = false,
            _ => {}
        }
        self.blocos.push(bloco);
        Ok(())
    }

    fn delta(&mut self, e: &Value) {
        let Some(pos) = self.posicao(e) else { return };
        let d = &e["delta"];
        match d["type"].as_str().unwrap_or("") {
            "text_delta" => {
                let Some(pedaco) = d["text"].as_str() else {
                    return;
                };
                if let Some(b) = self.blocos.get_mut(pos) {
                    anexar(b, "text", pedaco);
                    self.texto.push_str(pedaco);
                }
            }
            "thinking_delta" => {
                if let (Some(b), Some(p)) = (self.blocos.get_mut(pos), d["thinking"].as_str()) {
                    anexar(b, "thinking", p);
                }
            }
            "signature_delta" => {
                if let (Some(b), Some(p)) = (self.blocos.get_mut(pos), d["signature"].as_str()) {
                    anexar(b, "signature", p);
                }
            }
            "input_json_delta" => {
                if let (Some(s), Some(p)) =
                    (self.json_parcial.get_mut(&pos), d["partial_json"].as_str())
                {
                    s.push_str(p);
                }
            }
            "citations_delta" => self.citacao(pos, &d["citation"]),
            _ => {}
        }
    }

    fn citacao(&mut self, pos: usize, citacao: &Value) {
        let Some(b) = self.blocos.get_mut(pos).and_then(Value::as_object_mut) else {
            return;
        };
        let lista = b
            .entry("citations")
            .or_insert_with(|| Value::Array(Vec::new()));
        if !lista.is_array() {
            *lista = Value::Array(Vec::new());
        }
        if let Value::Array(l) = lista {
            l.push(citacao.clone());
        }
        if citacao["type"] != "web_search_result_location" {
            return;
        }
        let Some(url) = citacao["url"].as_str() else {
            return;
        };
        let numero = match self.citadas.iter().position(|c| c.url == url) {
            Some(i) => i + 1,
            None => {
                self.citadas.push(Citada {
                    url: url.to_string(),
                    titulo: citacao["title"].as_str().unwrap_or("").to_string(),
                    trecho: citacao["cited_text"].as_str().unwrap_or("").to_string(),
                });
                self.citadas.len()
            }
        } as u32;
        let marcas = self.marcadores.entry(pos).or_default();
        if !marcas.contains(&numero) {
            marcas.push(numero);
        }
    }

    fn fim_bloco(&mut self, e: &Value) -> Result<(), ErroIa> {
        let Some(pos) = self.posicao(e) else {
            return Ok(());
        };
        if let Some(parcial) = self.json_parcial.remove(&pos)
            && let Some(b) = self.blocos.get_mut(pos)
        {
            if !parcial.trim().is_empty() {
                b["input"] = serde_json::from_str(&parcial).map_err(|_| ErroIa::Indisponivel)?;
            } else if !b["input"].is_object() {
                // Sem pedaços, o input é o que veio no `content_block_start` (as buscas chamadas
                // de dentro da execução de código chegam assim, já completas); só falta o objeto.
                b["input"] = Value::Object(Default::default());
            }
        }
        if let Some(marcas) = self.marcadores.remove(&pos) {
            for n in marcas {
                self.texto.push_str(&format!("[{n}]"));
            }
        }
        Ok(())
    }

    pub fn progresso(&self) -> Progresso {
        Progresso {
            texto: self.texto.clone(),
            fontes: self.fontes(),
            buscando: self.buscando,
            terminado: self.parou && self.stop_reason.as_deref() != Some("pause_turn"),
        }
    }

    fn fontes(&self) -> Vec<Fonte> {
        self.citadas
            .iter()
            .enumerate()
            .map(|(i, c)| Fonte {
                numero: i as u32 + 1,
                url: c.url.clone(),
                titulo: c.titulo.clone(),
                dominio: dominio(&c.url),
                data: self.idades.get(&c.url).cloned(),
                trecho: c.trecho.clone(),
            })
            .collect()
    }

    pub fn blocos(&self) -> &[Value] {
        &self.blocos
    }

    pub fn stop_reason(&self) -> Option<&str> {
        self.stop_reason.as_deref()
    }

    /// A mensagem atual chegou até o `message_stop`.
    pub(crate) fn parou(&self) -> bool {
        self.parou
    }

    /// Chegou pelo menos um `message_start`: a partir daí a resposta já gastou tokens.
    pub(crate) fn comecou(&self) -> bool {
        !self.usos.is_empty()
    }

    pub(crate) fn uso(&self) -> Uso {
        self.usos.iter().fold(Uso::default(), |a, u| Uso {
            entrada: a.entrada + u.entrada,
            saida: a.saida + u.saida,
            cache_leitura: a.cache_leitura + u.cache_leitura,
            cache_escrita: a.cache_escrita + u.cache_escrita,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    fn fixture(nome: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/tests/fixtures/ia/{nome}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn eventos_em_pedacos(bytes: &[u8], tamanho: usize) -> Result<Vec<Value>, ErroIa> {
        let mut leitor = LeitorSse::default();
        let mut saida = Vec::new();
        for p in bytes.chunks(tamanho) {
            leitor.alimentar(p, &mut saida)?;
        }
        leitor.terminar(&mut saida)?;
        Ok(saida)
    }

    #[test]
    fn pedacos_arbitrarios_dao_os_mesmos_eventos() {
        let bytes = fixture("busca.sse");
        let inteiro = eventos_em_pedacos(&bytes, bytes.len()).unwrap();
        assert_eq!(inteiro.len(), 24);
        // Cortes no meio de linhas e de caracteres UTF-8 de vários bytes.
        for tamanho in [1, 2, 3, 7, 64, 1000] {
            assert_eq!(eventos_em_pedacos(&bytes, tamanho).unwrap(), inteiro);
        }
        let crlf = String::from_utf8(bytes).unwrap().replace('\n', "\r\n");
        assert_eq!(
            eventos_em_pedacos(crlf.as_bytes(), 5).unwrap(),
            inteiro,
            "CRLF também"
        );
    }

    #[test]
    fn ultima_linha_sem_quebra_ainda_conta() {
        let r = eventos_em_pedacos(b"event: x\ndata: {\"type\":\"message_stop\"}", 4).unwrap();
        assert_eq!(r, vec![json!({"type": "message_stop"})]);
    }

    #[test]
    fn sse_quebrado_ignora_ping_e_desconhecido_e_json_invalido_e_indisponivel() {
        let bytes = fixture("quebrado.sse");
        let mut leitor = LeitorSse::default();
        let mut saida = Vec::new();
        let r = leitor.alimentar(&bytes, &mut saida);
        assert_eq!(r, Err(ErroIa::Indisponivel));
        let tipos: Vec<&str> = saida.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(
            tipos,
            [
                "message_start",
                "ping",
                "algo_novo",
                "content_block_start",
                "content_block_delta"
            ]
        );
        let mut a = Acumulador::novo();
        for e in &saida {
            a.evento(e).unwrap();
        }
        assert_eq!(a.progresso().texto, "Começo da resposta ");
    }

    #[test]
    fn evento_error_e_indisponivel() {
        let mut a = Acumulador::novo();
        let e = json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}});
        assert_eq!(a.evento(&e), Err(ErroIa::Indisponivel));
    }

    #[test]
    fn eventos_fora_de_ordem_nao_entram_em_panico() {
        let mut a = Acumulador::novo();
        for e in [
            json!({"type": "content_block_delta", "index": 7, "delta": {"type": "text_delta", "text": "x"}}),
            json!({"type": "content_block_stop", "index": 3}),
            json!({"type": "content_block_delta", "delta": {}}),
            json!({"type": "message_delta", "delta": {}}),
            json!({}),
            json!([1, 2]),
        ] {
            a.evento(&e).unwrap();
        }
        assert_eq!(
            a.evento(&json!({"type": "content_block_start", "index": 5, "content_block": {"type": "text", "text": ""}})),
            Err(ErroIa::Indisponivel)
        );
        assert!(a.blocos().is_empty());
    }

    #[test]
    fn input_completo_no_inicio_do_bloco_fica_como_veio() {
        // Busca chamada de dentro da execução de código: o input chega inteiro no
        // `content_block_start`, sem `input_json_delta`. Zerá-lo muda o histórico e a API
        // recusa o pensamento que veio depois dele.
        let bloco = json!({
            "type": "server_tool_use",
            "id": "srvtoolu_1",
            "name": "web_search",
            "input": {"query": "Copom Selic"},
            "caller": {"type": "code_execution_20260120", "tool_id": "srvtoolu_0"}
        });
        let mut a = Acumulador::novo();
        for e in [
            json!({"type": "message_start", "message": {"usage": {}}}),
            json!({"type": "content_block_start", "index": 0, "content_block": bloco}),
            json!({"type": "content_block_stop", "index": 0}),
        ] {
            a.evento(&e).unwrap();
        }
        assert_eq!(a.blocos(), [bloco]);
    }

    #[test]
    fn input_em_pedacos_substitui_o_vazio_do_inicio() {
        let mut a = Acumulador::novo();
        for e in [
            json!({"type": "message_start", "message": {"usage": {}}}),
            json!({"type": "content_block_start", "index": 0, "content_block":
                {"type": "server_tool_use", "id": "s", "name": "web_search", "input": {}}}),
            json!({"type": "content_block_delta", "index": 0, "delta":
                {"type": "input_json_delta", "partial_json": "{\"query\": \"Se"}}),
            json!({"type": "content_block_delta", "index": 0, "delta":
                {"type": "input_json_delta", "partial_json": "lic\"}"}}),
            json!({"type": "content_block_stop", "index": 0}),
        ] {
            a.evento(&e).unwrap();
        }
        assert_eq!(a.blocos()[0]["input"], json!({"query": "Selic"}));
    }

    #[test]
    fn dominio_sem_www() {
        assert_eq!(dominio("https://www.bcb.gov.br/a/b?c"), "bcb.gov.br");
        assert_eq!(dominio("https://Valor.Globo.com"), "valor.globo.com");
        assert_eq!(dominio("https://u@exemplo.com:8443/x"), "exemplo.com");
    }
}
