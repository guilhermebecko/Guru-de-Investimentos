# Guru de Investimentos

Um app para computador que conhece a sua carteira, o seu perfil de investidor e o seu objetivo, e usa a IA que você escolher para responder a pergunta de todo mês: **onde coloco o próximo dinheiro?**

> **Status:** fases 0 a 7 concluídas; os textos legais aguardam revisão jurídica antes da 1.0. A ordem das fases está em [`docs/roadmap.md`](docs/roadmap.md); o escopo do MVP e o protótipo das telas estão em [`docs/`](docs/).

## Ideia

- **Sabe o que você tem.** A carteira é montada pela planilha da Área do Investidor da B3, pelo extrato em PDF da BTG ou por lançamento manual, com gráficos que mostram para onde o dinheiro está indo.
- **Sabe por que você investe.** O perfil segue o método da ANBIMA, e o objetivo em texto livre e as metas com prazo viram contexto para cada análise.
- **Mostra de onde tirou cada coisa.** A IA cita as fontes de mercado com link, e você vê e edita o contexto que ela recebe.
- **Traga a sua IA.** Você conecta a sua própria chave (Claude primeiro, depois OpenAI, Gemini e modelos locais). O app só integra a IA ao seu contexto de investimentos e não cobra pelo uso dela.

## Princípios

- **Programa instalado.** A carteira e a chave da IA ficam no seu computador. Não há servidor no MVP.
- **A IA apresenta, você decide.** O app mostra opções, prós e contras e as fontes. A escolha é sempre sua. Nada aqui é recomendação individual de investimento.
- **Indicadores de fonte oficial.** Selic, CDI, IPCA e taxas do Tesouro são buscados direto nas fontes oficiais. A busca na web da IA é usada só para notícias.

## Documentação

- [`docs/mvp.md`](docs/mvp.md): telas, escopo do MVP e decisões tomadas.
- [`docs/arquitetura.md`](docs/arquitetura.md): Electron, núcleo em Rust e como os dois conversam.
- [`docs/roadmap.md`](docs/roadmap.md): ordem de construção, fase a fase.
- [`docs/avisos.md`](docs/avisos.md): os avisos legais do app, os mesmos da tela "Sobre e avisos".
- [`docs/prototipo/index.html`](docs/prototipo/index.html): protótipo navegável das telas. Abra o arquivo no navegador. Os números são fictícios.

## Instalar

Os instaladores ficam na [página de versões](https://github.com/guilhermebecko/Guru-de-Investimentos/releases) do GitHub. Esta é uma versão de pré-lançamento: os avisos legais ainda vão passar por revisão jurídica antes da versão 1.0.

### Qual arquivo baixar

| Sistema | Arquivo |
|---------|---------|
| Windows | o que termina em `.exe` |
| Mac com Apple Silicon (M1 ou mais novo) | o que termina em `mac-arm64.dmg` |
| Mac com Intel | o que termina em `mac-x64.dmg` |
| Ubuntu e Debian | o que termina em `.deb` |
| Outros Linux | o que termina em `.AppImage` |

O app ainda não tem assinatura digital, porque o projeto não tem certificado. Por isso o Windows e o macOS avisam na primeira abertura. Os passos abaixo explicam o que fazer.

### Windows

Abra o `.exe`. O aviso "O Windows protegeu o computador" aparece porque o app não tem assinatura: clique em "Mais informações" e depois em "Executar assim mesmo". O app se instala na sua pasta de usuário, sem pedir senha de administrador.

### macOS

Abra o `.dmg` e arraste o Guru de Investimentos para a pasta Aplicativos. Na primeira abertura, o macOS bloqueia o app de desenvolvedor não identificado: vá em Ajustes do Sistema → Privacidade e Segurança e clique em "Abrir Mesmo Assim".

Se o aviso disser que o app está danificado, rode no Terminal:

```bash
xattr -dr com.apple.quarantine "/Applications/Guru de Investimentos.app"
```

### Linux

No Ubuntu e no Debian, instale o `.deb`:

```bash
sudo apt install ./<caminho da pasta>/app/dist/Guru-de-Investimentos-0.9.0-linux-amd64.deb
```

Nos outros Linux, use o AppImage: `chmod +x <arquivo>.AppImage` e abra o arquivo. No Ubuntu 24.04, o AppImage esbarra na restrição de namespaces de usuário do AppArmor; nesse sistema, use o `.deb`. O app nunca é distribuído com `--no-sandbox`.

### Atualizações

Ao abrir, o app procura uma versão nova na página de versões do GitHub. Nada é baixado sem o seu clique.

- **Windows e AppImage:** quando há versão nova, a barra lateral mostra "Baixar e instalar"; depois do download, "Reiniciar e atualizar" instala a versão nova.
- **macOS e `.deb`:** o app não se atualiza sozinho. A barra lateral mostra a versão nova e o link "Ver no GitHub", onde você baixa o instalador novo.

Para ligar ou desligar a procura, use "Procurar atualizações ao abrir o app" na tela "Sobre e avisos". Ao lado fica o botão "Procurar agora". Desligar vale a partir da próxima abertura.

### Os seus dados

A carteira, o perfil, as metas e as conversas ficam na pasta de dados do app:

- **Windows:** `%APPDATA%\Guru de Investimentos`
- **macOS:** `~/Library/Application Support/Guru de Investimentos`
- **Linux:** `~/.config/Guru de Investimentos`

Desinstalar o app não apaga essa pasta. Para apagar os dados, apague a pasta.

### Conferir o download

Cada versão traz o `SHA256SUMS.txt` com o hash de cada instalador. Calcule o hash do arquivo baixado e compare com a linha dele:

```bash
sha256sum <arquivo>                       # Linux
shasum -a 256 <arquivo>                   # macOS
```

```powershell
Get-FileHash <arquivo> -Algorithm SHA256  # Windows (PowerShell)
```

## Rodando localmente

Pré-requisitos: [Rust estável](https://rustup.rs) (via `rustup`) e Node 24.

```bash
# 1. Construir o núcleo em Rust como addon do Node
cd crates/guru-node && npm install && npm run build

# 2. Rodar o app
cd ../../app && npm install && npm start
```

Testes, a partir da raiz do repositório:

```bash
cargo fmt --check && cargo clippy --workspace -- -D warnings   # formatação e lint
cargo test --workspace                                         # núcleo em Rust
(cd crates/guru-node && npm test)                              # addon
(cd app && npm test)                                           # unitários do app
(cd app && npx playwright test)                                # E2E (abre o Electron)
```

O E2E roda com as janelas escondidas (`GURU_JANELA_OCULTA=1`, definido em `app/playwright.config.js`; só vale fora do app empacotado), então nada aparece na sua tela.

No Linux sem tela, rode o E2E com `xvfb-run -a npx playwright test`.

No Ubuntu 24.04, se o Electron falhar com erro de sandbox, libere os namespaces de usuário: `sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`.

## Licença

[MIT](LICENSE) © 2026 Guilherme Becker
