> **Build sem assinatura.** O projeto ainda não tem certificado, então o Windows e o macOS avisam na primeira abertura. Os passos para abrir o app estão na seção [Instalar](https://github.com/guilhermebecko/Guru-de-Investimentos#instalar) do README.

## Qual arquivo baixar

| Sistema | Arquivo |
|---------|---------|
| Windows | o que termina em `.exe` |
| Mac com Apple Silicon (M1 ou mais novo) | o que termina em `mac-arm64.dmg` |
| Mac com Intel | o que termina em `mac-x64.dmg` |
| Ubuntu e Debian | o que termina em `.deb` |
| Outros Linux | o que termina em `.AppImage` |

Os arquivos `latest.yml`, `latest-linux.yml` e `.blockmap` são usados pela atualização do app; não é preciso baixá-los.

## Conferir o download

O `SHA256SUMS.txt` traz o hash de cada instalador. Calcule o hash do arquivo baixado e compare com a linha dele:

- **Linux:** `sha256sum <arquivo>`, ou `sha256sum -c SHA256SUMS.txt --ignore-missing` na pasta do download;
- **macOS:** `shasum -a 256 <arquivo>`;
- **Windows (PowerShell):** `Get-FileHash <arquivo> -Algorithm SHA256`.

## Revisão jurídica

Os avisos legais estão na tela "Sobre e avisos" do app e em [`docs/avisos.md`](https://github.com/guilhermebecko/Guru-de-Investimentos/blob/main/docs/avisos.md).

> Versão de pré-lançamento: estes avisos ainda vão passar por revisão jurídica antes da versão 1.0.
