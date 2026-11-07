// Gera build/icon.png (1024 × 1024): o verde da marca com a escadinha em pixel branco, a mesma
// marca que fica antes de "Guru" no menu lateral (`.side .logo::before` em base.css).
// Gera também build/icons/<N>x<N>.png nos tamanhos padrão: o tema hicolor do Linux não lista a
// pasta 1024x1024, e sem esses tamanhos o menu de aplicativos mostra o ícone genérico. Sem dependência: o PNG é montado à mão e comprimido com node:zlib.
// Uso: node scripts/icone.mjs
import { writeFileSync, mkdirSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const LADO = 1024;
const TAMANHOS_LINUX = [16, 24, 32, 48, 64, 128, 256, 512];
const FUNDO = [0x04, 0x32, 0x1f]; // #04321F, --brand
const BRANCO = [0xff, 0xff, 0xff]; // #fff, a cor da marca no menu

// A escadinha do menu numa grade de 4 × 3 pixels; cada pixel vira um bloco de 160 px, centrado.
const MARCA = [
  "..##",
  ".##.",
  "##..",
];
// Proporção do bloco no ícone de 1024 px (160 / 1024); os outros tamanhos escalam igual.
const BLOCO_RELATIVO = 160 / LADO;

function corEm(x, y, lado) {
  const bloco = lado * BLOCO_RELATIVO;
  const margemX = (lado - MARCA[0].length * bloco) / 2;
  const margemY = (lado - MARCA.length * bloco) / 2;
  // Centro do pixel, para os tamanhos em que o bloco não é inteiro.
  const c = Math.floor((x + 0.5 - margemX) / bloco);
  const l = Math.floor((y + 0.5 - margemY) / bloco);
  const dentro = c >= 0 && l >= 0 && l < MARCA.length && c < MARCA[0].length;
  return dentro && MARCA[l][c] === "#" ? BRANCO : FUNDO;
}

const TABELA_CRC = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(bytes) {
  let c = 0xffffffff;
  for (const b of bytes) c = TABELA_CRC[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function bloco(tipo, dados) {
  const tamanho = Buffer.alloc(4);
  tamanho.writeUInt32BE(dados.length);
  const corpo = Buffer.concat([Buffer.from(tipo, "ascii"), dados]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(corpo));
  return Buffer.concat([tamanho, corpo, crc]);
}

function png(lado) {
  const cabecalho = Buffer.alloc(13);
  cabecalho.writeUInt32BE(lado, 0);
  cabecalho.writeUInt32BE(lado, 4);
  cabecalho[8] = 8; // bits por canal
  cabecalho[9] = 2; // RGB
  const linha = 1 + lado * 3;
  const cru = Buffer.alloc(linha * lado);
  for (let y = 0; y < lado; y++) {
    cru[y * linha] = 0; // filtro "nenhum"
    for (let x = 0; x < lado; x++) {
      const [r, g, b] = corEm(x, y, lado);
      const i = y * linha + 1 + x * 3;
      cru[i] = r;
      cru[i + 1] = g;
      cru[i + 2] = b;
    }
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    bloco("IHDR", cabecalho),
    bloco("IDAT", deflateSync(cru, { level: 9 })),
    bloco("IEND", Buffer.alloc(0)),
  ]);
}

function gravar(caminho, lado) {
  mkdirSync(dirname(caminho), { recursive: true });
  writeFileSync(caminho, png(lado));
  console.log(`Ícone gravado em ${caminho}`);
}

const build = join(dirname(fileURLToPath(import.meta.url)), "..", "build");
gravar(join(build, "icon.png"), LADO);
for (const n of TAMANHOS_LINUX) gravar(join(build, "icons", `${n}x${n}.png`), n);
