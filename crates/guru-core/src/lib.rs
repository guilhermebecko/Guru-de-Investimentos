mod aporte;
mod db;
mod error;
mod ia;
mod importacao;
mod investimento;
mod mercado;
mod metas;
mod painel;
mod perfil;

pub use db::{Db, SCHEMA_VERSION};
pub use error::CoreError;
pub use investimento::{
    Benchmark, Classe, ErroCampo, Forma, Garantia, Gravacao, INSTITUICOES, Indexador, Investimento,
    InvestimentoValido, Ir, Liquidez, MAX_CENTAVOS, NovoInvestimento, Resumo, ResumoClasse,
    TipoInfo, formatar_cnpj, nome, normalizar_cnpj, rendimento, tipo, tipos, validar,
};

pub use importacao::pdf;
pub use importacao::{
    Aba, AbaRv, Acao, AtivoBtg, Celula, Comparacao, Decisao, Estado, ExtratoBtg,
    Fonte as FonteImportacao, Item, LinhaB3, Posicao, ResultadoImportacao, aplicar_importacao,
    comparar, comparar_btg, ler_extrato, ler_extrato_linhas, ler_posicao, ler_posicao_abas,
    ler_xlsx, ultima_importacao,
};

pub use perfil::{
    AVISO_FIXO, Opcao, Perfil, PerfilGravado, Pergunta, ResultadoPerfil, alocacao_sugerida,
    calcular, perguntas,
};

pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub use metas::{Meta, NovaMeta, Progresso, Situacao, formatar_reais, progresso};

pub use mercado::{
    B3Alvo, Baixador, BaixadorHttp, BaixadorPasta, CotaFundo, Cotacao, ErroFonte, Origem,
    PontoDiario, PrecoTesouro, ResultadoAtualizacao, StatusFonte, Valor, atualizar,
    ler_cotahist_txt, ler_cotahist_zip, ler_inf_diario_csv, ler_inf_diario_zip, ler_sgs,
    ler_tesouro, titulo_tesouro, valorizar,
};

pub use painel::{
    Historico, Instituicao, Numeros, Painel, PontoHistorico, Referencia, Variacao, Vencimento,
    acumulado_benchmark, benchmark_padrao, datas_painel, historico, painel,
};

pub use aporte::{
    Aporte, ParteAporte, ResultadoAporte, Situacao as SituacaoAporte, dividir_aporte,
};

pub use ia::{
    Acumulador, ClienteFixture, ClienteHttp, ClienteIa, Cofre, CofreMemoria, CofreSistema,
    ConfigIa, ConfigSalva, ContextoMontado, Conversa, ConversaResumo, DIRECOES, ErroIa, FocoAporte,
    Fonte, FonteInterna, MODELOS, MensagemGravada, ParteContexto, Progresso as ProgressoIa, UsoMes,
    corpo_pedido, estimar_tokens, perguntar, perguntar_aporte, remover_chave, salvar_chave,
    validar_formato_chave,
};
pub use ia::{
    Gerados, Insight, Noticia, PainelIa, buscar_noticias, corpo_insights, gerar_insights,
    ler_insights, ler_noticias, rotulo_insight,
};
