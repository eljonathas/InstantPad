# InstantPad

Soundboard nativo para Windows, com cara de sampler físico e o MyInstants embutido.
Seu microfone e os sons saem misturados por um dispositivo virtual, que o jogo usa como microfone.

```
microfone ─┐
           ├─► InstantPad ─► CABLE Input ═ driver ═ CABLE Output ─► jogo / Discord
sons ──────┤
           └─► seus fones (só os sons)
```

## Instalar

1. Abra `dist/InstantPad.exe`. Não precisa instalar nada além do driver.
2. Em **SETUP**, clique em **INSTALAR VB-CABLE** e depois em **Install Driver**.
3. Reinicie o PC.
4. Abra o app e confira em **SETUP**:
   - **Seu microfone**: o microfone real
   - **Saída para o jogo**: `CABLE Input` (já vem selecionado)
   - **Onde você ouve**: seus fones

## No jogo

Nas opções de voz do jogo ou do Discord, escolha **`CABLE Output`** como microfone.

Se o jogo usa aperte-para-falar, segure a tecla enquanto o som toca, ou use voz aberta.

## Usando

| Ação | Como |
| --- | --- |
| Tocar um som | Clique no pad |
| Encher um pad | Clique no pad vazio e depois em **+ PAD** num resultado, ou arraste o resultado até o pad |
| Usar arquivo próprio | Arraste do Explorer até o pad, ou botão direito > Carregar arquivo |
| Atalho global, volume, cor, nome | Botão direito no pad |
| Reorganizar pads | Ctrl + arrastar |
| Ouvir antes, só nos fones | ▶ no resultado |
| Tocar no jogo sem salvar | **MIC** no resultado |
| Ajustar um knob | Arraste para cima/baixo ou use a roda do mouse. Duplo clique volta ao padrão |

Os knobs são **MIC** (sua voz no jogo), **JOGO** (sons no jogo) e **FONES** (sons para você).

São 4 bancos com 24 pads cada. Sons e configurações ficam em `%APPDATA%\InstantPad`.

## Limitações

- O driver virtual é o VB-CABLE, da VB-Audio. O Windows só carrega driver de áudio assinado
  pela Microsoft, então o app não consegue criar o próprio. Ele baixa o instalador do site
  oficial e só executa se a assinatura digital for da VB-Audio. O VB-CABLE é gratuito, com
  doação opcional.
- O MyInstants não tem API pública, e o app lê o HTML do site. Se o site mudar, ajuste
  `Parse` em `Services/MyInstantsClient.cs`.
- Jogos executados como administrador podem bloquear os atalhos globais. Nesse caso, abra o
  InstantPad como administrador também.

## Compilar

Requer o .NET 8 SDK.

```bash
dotnet publish InstantPad -c Release -o dist
```

`InstantPad.exe --selftest "parte do nome de uma saída"` testa busca, download, decodificação,
mixer e atalhos, e grava o relatório em `selftest.txt`, na pasta de dados.
