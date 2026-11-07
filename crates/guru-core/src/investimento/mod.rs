mod catalogo;
mod cnpj;
mod modelo;
mod repositorio;
mod texto;
mod validacao;

pub(crate) use catalogo::enum_catalogo;
pub use catalogo::{
    Benchmark, Classe, Forma, Garantia, INSTITUICOES, Indexador, Ir, Liquidez, TipoInfo, tipo,
    tipos,
};
pub use cnpj::{formatar_cnpj, normalizar_cnpj};
pub use modelo::{ErroCampo, InvestimentoValido, NovoInvestimento};
pub use repositorio::{Gravacao, Investimento, Resumo, ResumoClasse};
pub(crate) use repositorio::{decimos, inserir_validado, validar_com_meta};
pub use texto::{nome, rendimento};
pub use validacao::{MAX_CENTAVOS, validar};
