//! Acesso à rede restrito à lista fixa de destinos oficiais.

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use chrono::NaiveDate;

use super::ErroFonte;

const PREFIXO_SGS: &str = "https://api.bcb.gov.br/dados/serie/bcdata.sgs.";
const MEIO_SGS: &str = "/dados?formato=json&dataInicial=";
const FIM_SGS: &str = "&dataFinal=";
const SERIES_SGS: [&str; 3] = ["12", "11", "433"];
const URL_TESOURO: &str = "https://www.tesourotransparente.gov.br/ckan/dataset/df56aa42-484a-4a59-8184-7676580c81e3/resource/796d2059-14e9-44e3-80c9-2d9e30b405c1/download/PrecoTaxaTesouroDireto.csv";
const PREFIXO_B3: &str = "https://bvmf.bmfbovespa.com.br/InstDados/SerHist/COTAHIST_D";
const SUFIXO_B3: &str = ".ZIP";
const PREFIXO_CVM: &str = "https://dados.cvm.gov.br/dados/FI/DOC/INF_DIARIO/DADOS/inf_diario_fi_";
const SUFIXO_CVM: &str = ".zip";
/// Prazo para resolver, conectar, enviar o pedido e receber o início da resposta.
const CONECTAR: Duration = Duration::from_secs(60);
/// Prazo para receber o corpo inteiro (o CSV completo do Tesouro tem ~15 MB).
const CORPO: Duration = Duration::from_secs(300);
/// O histórico do Tesouro tem ~15 MB; o limite protege contra respostas absurdas.
const LIMITE_CORPO: u64 = 64 * 1024 * 1024;

/// Baixa o conteúdo de uma URL. Em produção é o `BaixadorHttp`; nos testes, uma pasta de arquivos.
pub trait Baixador {
    fn baixar(&self, url: &str) -> Result<Vec<u8>, ErroFonte>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Destino {
    Sgs { serie: String },
    Tesouro,
    B3 { data: String },
    Cvm { mes: String },
}

pub(crate) fn url_sgs(serie: &str, de: NaiveDate, ate: NaiveDate) -> String {
    format!(
        "{PREFIXO_SGS}{serie}{MEIO_SGS}{}{FIM_SGS}{}",
        de.format("%d/%m/%Y"),
        ate.format("%d/%m/%Y")
    )
}

pub(crate) fn url_tesouro() -> String {
    URL_TESOURO.to_string()
}

pub(crate) fn url_b3(data: NaiveDate) -> String {
    format!("{PREFIXO_B3}{}{SUFIXO_B3}", data.format("%d%m%Y"))
}

// O atualizador (Task 2) passa a usar; até lá só os testes chamam.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn url_cvm(ano: i32, mes: u32) -> String {
    format!("{PREFIXO_CVM}{ano:04}{mes:02}{SUFIXO_CVM}")
}

fn data_br_exata(s: &str) -> bool {
    s.len() == 10 && NaiveDate::parse_from_str(s, "%d/%m/%Y").is_ok()
}

/// Reconhece só as URLs que o atualizador monta; qualquer outra coisa é `None`.
pub(crate) fn classificar(url: &str) -> Option<Destino> {
    if url == URL_TESOURO {
        return Some(Destino::Tesouro);
    }
    if let Some(resto) = url.strip_prefix(PREFIXO_SGS) {
        let (serie, resto) = resto.split_once(MEIO_SGS)?;
        let (de, ate) = resto.split_once(FIM_SGS)?;
        if SERIES_SGS.contains(&serie) && data_br_exata(de) && data_br_exata(ate) {
            return Some(Destino::Sgs {
                serie: serie.to_string(),
            });
        }
        return None;
    }
    if let Some(resto) = url.strip_prefix(PREFIXO_B3) {
        let data = resto.strip_suffix(SUFIXO_B3)?;
        if data.len() == 8 && NaiveDate::parse_from_str(data, "%d%m%Y").is_ok() {
            return Some(Destino::B3 {
                data: data.to_string(),
            });
        }
    }
    if let Some(resto) = url.strip_prefix(PREFIXO_CVM) {
        let mes = resto.strip_suffix(SUFIXO_CVM)?;
        if mes.len() == 6
            && mes.bytes().all(|b| b.is_ascii_digit())
            && NaiveDate::parse_from_str(&format!("{mes}01"), "%Y%m%d").is_ok()
        {
            return Some(Destino::Cvm {
                mes: mes.to_string(),
            });
        }
    }
    None
}

pub struct BaixadorHttp {
    agente: ureq::Agent,
}

impl BaixadorHttp {
    pub fn novo() -> Self {
        let config = ureq::Agent::config_builder()
            // Sem prazo global: o corpo de ~15 MB do Tesouro leva ~50 s e tem orçamento próprio.
            .timeout_resolve(Some(CONECTAR))
            .timeout_connect(Some(CONECTAR))
            .timeout_send_request(Some(CONECTAR))
            .timeout_recv_response(Some(CONECTAR))
            .timeout_recv_body(Some(CORPO))
            .user_agent(format!("GuruDeInvestimentos/{}", env!("CARGO_PKG_VERSION")))
            .http_status_as_error(false)
            .https_only(true)
            // Um redirecionamento poderia levar para fora da lista de destinos.
            .max_redirects(0)
            .max_redirects_will_error(false)
            .build();
        Self {
            agente: ureq::Agent::new_with_config(config),
        }
    }
}

impl Default for BaixadorHttp {
    fn default() -> Self {
        Self::novo()
    }
}

impl Baixador for BaixadorHttp {
    fn baixar(&self, url: &str) -> Result<Vec<u8>, ErroFonte> {
        if classificar(url).is_none() {
            return Err(ErroFonte::Rede("destino não permitido".into()));
        }
        let mut resposta = self
            .agente
            .get(url)
            .call()
            .map_err(|e| ErroFonte::Rede(e.to_string()))?;
        let status = resposta.status().as_u16();
        if status != 200 {
            return Err(ErroFonte::Status(status));
        }
        let mut corpo = Vec::new();
        resposta
            .body_mut()
            .as_reader()
            .take(LIMITE_CORPO)
            .read_to_end(&mut corpo)
            .map_err(|e| ErroFonte::Rede(e.to_string()))?;
        Ok(corpo)
    }
}

/// Modo de teste: cada URL vira um arquivo da pasta (`sgs-<série>.json`, `tesouro.csv`,
/// `COTAHIST_D<ddMMaaaa>.ZIP`, `inf_diario_fi_<AAAAMM>.zip`); arquivo ausente responde 404.
pub struct BaixadorPasta {
    pub pasta: PathBuf,
}

impl Baixador for BaixadorPasta {
    fn baixar(&self, url: &str) -> Result<Vec<u8>, ErroFonte> {
        let nome = match classificar(url) {
            Some(Destino::Sgs { serie }) => format!("sgs-{serie}.json"),
            Some(Destino::Tesouro) => "tesouro.csv".to_string(),
            Some(Destino::B3 { data }) => format!("COTAHIST_D{data}.ZIP"),
            Some(Destino::Cvm { mes }) => format!("inf_diario_fi_{mes}.zip"),
            None => return Err(ErroFonte::Rede("destino não permitido".into())),
        };
        std::fs::read(self.pasta.join(nome)).map_err(|_| ErroFonte::Status(404))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(a: i32, m: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(a, m, dia).unwrap()
    }

    #[test]
    fn urls_montadas_sao_permitidas() {
        assert!(classificar(&url_sgs("12", d(2014, 3, 5), d(2024, 3, 4))).is_some());
        assert!(classificar(&url_sgs("433", d(2025, 1, 1), d(2026, 10, 28))).is_some());
        assert!(classificar(&url_tesouro()).is_some());
        assert_eq!(
            classificar(&url_b3(d(2026, 10, 28))),
            Some(Destino::B3 {
                data: "28102026".into()
            })
        );
    }

    #[test]
    fn url_cvm_e_permitida() {
        assert_eq!(
            url_cvm(2026, 11),
            "https://dados.cvm.gov.br/dados/FI/DOC/INF_DIARIO/DADOS/inf_diario_fi_202611.zip"
        );
        assert_eq!(
            classificar(&url_cvm(2026, 11)),
            Some(Destino::Cvm {
                mes: "202611".into()
            })
        );
    }

    #[test]
    fn url_sgs_tem_o_formato_oficial() {
        assert_eq!(
            url_sgs("12", d(2026, 9, 1), d(2026, 10, 28)),
            "https://api.bcb.gov.br/dados/serie/bcdata.sgs.12/dados?formato=json&dataInicial=01/09/2026&dataFinal=28/10/2026"
        );
    }

    #[test]
    fn recusa_destinos_fora_da_lista() {
        let ruins = [
            "http://api.bcb.gov.br/dados/serie/bcdata.sgs.12/dados?formato=json&dataInicial=01/09/2026&dataFinal=28/10/2026",
            "https://exemplo.com/dados",
            "https://api.bcb.gov.br/dados/serie/bcdata.sgs.99/dados?formato=json&dataInicial=01/09/2026&dataFinal=28/10/2026",
            "https://api.bcb.gov.br/dados/serie/bcdata.sgs.12/dados?formato=json&dataInicial=01/09/2026&dataFinal=28/10/2026&x=1",
            "https://api.bcb.gov.br.evil.com/dados/serie/bcdata.sgs.12/dados?formato=json&dataInicial=01/09/2026&dataFinal=28/10/2026",
            "https://bvmf.bmfbovespa.com.br/InstDados/SerHist/COTAHIST_D../../x.ZIP",
            "https://bvmf.bmfbovespa.com.br/InstDados/SerHist/COTAHIST_D99999999.ZIP",
            "https://bvmf.bmfbovespa.com.br/InstDados/SerHist/COTAHIST_D28102026.ZIP?a=b",
            "https://www.tesourotransparente.gov.br/ckan/outro.csv",
            "https://dados.cvm.gov.br/dados/FI/DOC/INF_DIARIO/DADOS/inf_diario_fi_202613.zip",
            "https://dados.cvm.gov.br/dados/FI/DOC/INF_DIARIO/DADOS/inf_diario_fi_2026.zip",
            "https://dados.cvm.gov.br/dados/FI/DOC/INF_DIARIO/DADOS/HIST/inf_diario_fi_2020.zip",
            "https://dados.cvm.gov.br/dados/FI/DOC/INF_DIARIO/DADOS/inf_diario_fi_202611.zip?x=1",
            "http://dados.cvm.gov.br/dados/FI/DOC/INF_DIARIO/DADOS/inf_diario_fi_202611.zip",
            "",
        ];
        for u in ruins {
            assert!(classificar(u).is_none(), "deveria recusar: {u}");
        }
    }

    #[test]
    fn prazos_do_baixador_http() {
        let t = BaixadorHttp::novo().agente.config().timeouts();
        assert_eq!(t.global, None);
        assert_eq!(t.connect, Some(Duration::from_secs(60)));
        assert_eq!(t.recv_response, Some(Duration::from_secs(60)));
        assert_eq!(t.recv_body, Some(Duration::from_secs(300)));
    }

    #[test]
    fn baixador_http_recusa_sem_chamar_a_rede() {
        let r = BaixadorHttp::novo().baixar("https://exemplo.com/qualquer");
        assert_eq!(r, Err(ErroFonte::Rede("destino não permitido".into())));
    }
}
