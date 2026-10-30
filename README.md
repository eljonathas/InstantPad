<p align="center">
  <img src="assets/social-preview.png" alt="InstantPad" width="640">
</p>

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

1. Baixe o `InstantPad.exe` da [última versão](https://github.com/eljonathas/InstantPad/releases/latest).
   Ele não precisa de instalação nem do .NET.
2. Na primeira abertura, o Windows pode avisar sobre um app desconhecido, porque o exe não tem
   certificado de assinatura. Use **Mais informações > Executar assim mesmo**.
3. Em **Ajustes**, clique em **Instalar VB-CABLE** e depois em **Install Driver**.
4. Reinicie o PC.
5. Confira em **Ajustes**:
   - **Seu microfone**: o microfone real
   - **Saída para o jogo**: `CABLE Input` (já vem selecionado)
   - **Onde você ouve**: seus fones

Deixe o exe em uma pasta sua, como Documentos, para que a atualização automática consiga trocá-lo.

## No jogo

Nas opções de voz do jogo ou do Discord, escolha **`CABLE Output`** como microfone.

Se o jogo usa aperte-para-falar, segure a tecla enquanto o som toca, ou use voz aberta.

## Usando

| Ação | Como |
| --- | --- |
| Tocar um som | Clique no pad ou use o atalho dele |
| Encher um pad | Clique no pad vazio e depois em **+ Pad** num resultado, ou arraste o resultado até o pad |
| Ouvir um áudio online só nos fones | **▶** no resultado |
| Tocar um áudio online no jogo sem salvar em pad | **Mic** no resultado |
| Baixar um áudio no computador | **↓** no resultado ou nos favoritos, ou botão direito no pad |
| Favoritar | **☆** no resultado, ou botão direito no pad |
| Usar arquivo próprio | Arraste do Explorer até o pad, ou botão direito > Carregar arquivo |
| Atalho, volume, cor, nome | Botão direito no pad |
| Reorganizar pads | Ctrl + arrastar |
| Ajustar um knob | Arraste para cima/baixo ou use a roda do mouse. Duplo clique volta ao padrão |

Os knobs são **Mic** (sua voz no jogo), **Jogo** (sons no jogo) e **Fones** (sons para você).

### Interfaces

Crie quantas quiser com o **+**. Cada uma tem nome, grade própria (até 8 linhas por 10 colunas)
e cor. O **⚙** configura a interface atual, e o botão direito na aba move ou exclui.

### Atalhos numéricos

Vêm ligados, e funcionam com o jogo em primeiro plano:

| Tecla | Ação |
| --- | --- |
| `Num1` a `Num9`, `Num0` | Os 10 primeiros pads da interface ativa |
| `Num+` / `Num-` | Próxima / interface anterior |
| `Num,` | Para tudo |

Em **Ajustes** dá para trocar por `Ctrl` + teclado numérico, `Alt` + `1` a `0`, ou desligar.
Deixe o NumLock ligado. Um atalho definido à mão em um pad tem prioridade sobre os numéricos.

## Atualização automática

Ao abrir, o app procura uma versão nova e a baixa em segundo plano, conferindo o SHA-256. Ela passa
a valer na próxima abertura, ou na hora pelo botão **Reiniciar agora** em **Ajustes**. O app nunca
reinicia sozinho, para não interromper uma partida.

## Arquitetura

Clean Architecture, com as dependências sempre apontando para dentro:

```
InstantPad.Desktop         WPF, ViewModels e composição das dependências
   └─► InstantPad.Infrastructure   NAudio, MyInstants, GitHub, arquivos JSON, driver
          └─► InstantPad.Application   casos de uso e portas (interfaces)
                 └─► InstantPad.Domain     regras do quadro de pads e atalhos
```

`Domain` e `Application` não conhecem WPF, NAudio ou rede. Toda a montagem acontece em
[`CompositionRoot.cs`](src/InstantPad.Desktop/CompositionRoot.cs).

## Desenvolvimento

Requer o .NET 8 SDK.

```bash
dotnet test InstantPad.sln
```

```bash
dotnet run --project src/InstantPad.Desktop
```

Builds locais têm versão `0.0.0` e não se atualizam sozinhos.

## Releases

Cada push na `main` roda os testes, calcula a versão pelos commits e publica o exe em um
[Release](https://github.com/eljonathas/InstantPad/releases), seguindo Conventional Commits:

| Commit | Versão |
| --- | --- |
| `fix:`, `docs:`, `chore:` e outros | patch (1.0.**1**) |
| `feat:` | minor (1.**1**.0) |
| `feat!:` ou `BREAKING CHANGE` | major (**2**.0.0) |

## Limitações

- O driver virtual é o VB-CABLE, da VB-Audio. O Windows só carrega driver de áudio assinado pela
  Microsoft, então o app não consegue criar o próprio. Ele baixa o instalador do site oficial e só
  executa se a assinatura digital for da VB-Audio. O VB-CABLE é gratuito, com doação opcional.
- O MyInstants não tem API pública, e o app lê o HTML do site. Se o site mudar, ajuste
  [`MyInstantsPageParser.cs`](src/InstantPad.Infrastructure/Catalog/MyInstantsPageParser.cs).
- Jogos executados como administrador podem bloquear os atalhos globais. Nesse caso, abra o
  InstantPad como administrador também.

## Créditos

Fonte [Lexend](https://www.lexend.com/), sob a [SIL Open Font License](src/InstantPad.Desktop/Assets/Fonts/OFL.txt).
