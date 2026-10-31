[English](README.md) | Português

<p align="center">
  <img src="assets/social-preview.png" alt="InstantPad" width="640">
</p>

# InstantPad

O InstantPad é um soundboard gratuito e de código aberto para Windows, macOS e Linux. Ele tem cara de sampler de hardware e traz a biblioteca do MyInstants embutida. O app mistura o seu microfone com os sons que você dispara e manda o resultado para um cabo de áudio virtual, que o jogo ou o Discord usa como microfone. Seus amigos ouvem a sua voz e o som juntos.

![InstantPad mostrando resultados do MyInstants ao lado da grade de pads](apps/desktop/tests/e2e/screenshots/myinstants.png)

## Instalação

### Por que você precisa de um cabo de áudio virtual

Jogos e Discord só escutam microfones. Um cabo de áudio virtual funciona como um microfone falso: tudo que um app toca numa ponta sai pela outra ponta como entrada de microfone. O InstantPad toca a sua voz e os seus sons no cabo, e você escolhe o cabo como microfone no jogo. No Windows e no macOS o cabo é um driver pequeno, instalado uma vez. No Linux o InstantPad cria o cabo para você.

```
microfone ─┐
           ├─► InstantPad ─► cabo virtual ─► jogo / Discord
sons ──────┤
           └─► seus fones (só os sons)
```

Baixe o arquivo do seu sistema na [última versão](https://github.com/eljonathas/InstantPad/releases/latest).

### Windows

1. Baixe o instalador `.exe` e execute. Ele instala só para o seu usuário e não pede senha de administrador. O app ainda não tem assinatura de código, então o Windows pode exibir um aviso na primeira vez. Clique em **Mais informações** e depois em **Executar assim mesmo**.
2. Abra o InstantPad, vá em **Ajustes** e clique em **Instalar VB-CABLE**. O InstantPad baixa o driver do site oficial da VB-Audio e abre o instalador dele. Nessa janela, clique em **Install Driver**.
3. Reinicie o PC. Antes disso o Windows já mostra o driver como funcionando, mas ele só funciona direito depois da reinicialização.
4. O VB-CABLE se define como dispositivo de saída do Windows ao instalar. Abra as configurações de som do Windows e escolha seus fones como saída de novo. Enquanto a saída continuar no cabo, a tela **Ajustes** mostra um aviso com o botão **Abrir configurações de som**.

Se preferir não instalar o app, baixe o `InstantPad-portable.exe`. Ele roda sozinho e não se atualiza. Você ainda precisa do VB-CABLE, então siga os passos 2 a 4.

### macOS

1. Baixe o `.dmg` que combina com o seu Mac: o de Apple silicon para chips M1 e mais novos, o de Intel para Macs mais antigos. Abra e arraste o InstantPad para Aplicativos.
2. O app ainda não é notarizado, então o macOS o bloqueia na primeira vez. Tente abrir uma vez, depois abra os **Ajustes do Sistema**, vá em **Privacidade e Segurança** e clique em **Abrir Mesmo Assim**.
3. Permita o acesso ao microfone quando o macOS perguntar.
4. Instale o [BlackHole 2ch](https://existential.audio/blackhole/), o cabo virtual. O botão **Instalar BlackHole** em **Ajustes** abre a página de download dele.

### Linux

1. Baixe o `.AppImage`, dê permissão de execução (`chmod +x` no arquivo, ou a opção de permissões do seu gerenciador de arquivos) e execute.
2. O InstantPad cria o próprio cabo virtual, chamado `InstantPad_Mic`, pelo PulseAudio ou PipeWire. Ele usa o comando `pactl`, então o `pactl` precisa estar instalado.

### Atualizações

O InstantPad procura uma versão nova ao abrir e baixa em segundo plano, conferindo a assinatura. A versão nova é aplicada quando você fecha o app, ou na hora com **Reiniciar agora** em **Ajustes**. Ele nunca reinicia sozinho, então uma atualização não interrompe sua partida.

## Configure o jogo ou o Discord

1. Abra o InstantPad e vá em **Ajustes**. A seção **No jogo** mostra o microfone a usar: `CABLE Output` no Windows, `BlackHole 2ch` no macOS e `InstantPad_Mic` no Linux.
2. Em **1. Seu microfone**, escolha o seu microfone de verdade. Em **3. Onde você ouve**, escolha seus fones. **2. Saída para o jogo** já deve apontar para o cabo.
3. Nas configurações de voz do jogo ou do Discord, selecione o cabo como microfone.
4. Toque um pad e pergunte a um amigo se ele ouviu, ou acompanhe o nível de entrada nas configurações de voz.

Se o jogo usa push-to-talk, segure a tecla enquanto o som toca, ou troque para microfone aberto.

No Discord, use o **Input Profile** **Studio**. O Voice Isolation remove tudo que não é voz, inclusive os seus sons. Se você usa um perfil personalizado, desligue a supressão de ruído, o cancelamento de eco e o controle automático de ganho.

## Como usar

| Para | Faça isto |
| --- | --- |
| Tocar um som | Clique no pad ou aperte o atalho dele |
| Encontrar sons | Busque no MyInstants ou abra **Em alta** |
| Preencher um pad | Clique num pad vazio e depois em **+ Pad** num resultado, ou arraste o resultado até o pad. O som é baixado e fica salvo no seu computador |
| Testar um som online nos fones | O botão de play do resultado (**Ouvir só nos fones**) |
| Tocar um som online no jogo sem salvar | **Mic** no resultado (**Tocar no jogo agora**) |
| Favoritar | A estrela no resultado, ou botão direito num pad e **Favoritar** |
| Usar um arquivo seu | Arraste do gerenciador de arquivos até um pad, ou clique com o botão direito no pad e escolha **Carregar arquivo de áudio...** |
| Guardar uma cópia do som em outro lugar | Botão direito no pad e **Baixar no computador...** |
| Mudar atalho, volume, cor ou nome de um pad | Botão direito no pad |
| Reorganizar os pads | Segure Ctrl e arraste |
| Parar tudo | **Parar** |
| Girar um knob | Arraste para cima ou para baixo, ou use a roda do mouse. Clique duplo volta ao padrão |

Os knobs ajustam a sua voz no jogo (**Mic**), os sons no jogo (**Jogo**) e os sons que você ouve (**Fones**).

Os sons que estão em pads e nos favoritos ficam salvos no seu computador. Os sons que você só ouviu vão para uma pasta temporária, que o InstantPad esvazia quando abre e quando fecha. Seus próprios arquivos podem ser `.mp3`, `.wav`, `.m4a`, `.aac` ou `.flac`. O formato `.wma` não é suportado.

Só uma cópia do app roda por vez. Abrir de novo traz a janela que já existe para a frente.

### Interfaces

Crie quantas interfaces quiser com **+**. Cada uma tem nome, cor e grade próprios, de até 8 linhas por 10 colunas. A engrenagem (**Configurar esta interface**) configura a interface atual, e o botão direito numa aba move ou exclui.

### Efeitos de voz

A tecla **Voice** abaixo do logo abre o InstantVoice, onde você muda a própria voz em chamadas e jogos. Clique num efeito para ligá-lo, e os knobs dele aparecem à direita. **Natural** desliga os efeitos. O efeito continua ligado quando você volta para **Pad**, e o LED da tecla **Voice** mostra que ele está ativo.

| Efeito | Knobs |
| --- | --- |
| Voz fina | **Intensidade** e **Formante** |
| Voz grossa | **Intensidade** e **Formante** |
| Variação de pitch, que sobe até um tom alto e desce até um baixo | **Tempo**, **Alto** e **Baixo** |

Ligue **Ouvir minha voz** para testar um efeito nos fones. Use fones nesse teste, porque caixas de som realimentam o microfone.

Um efeito atrasa sua voz em cerca de 40 ms. **Natural** não acrescenta atraso.

### Um som por vez

Em **Ajustes** há a opção **Um som por vez**. Com ela ligada, um som novo para o que estava tocando.

### Atalhos numéricos

Vêm ligados por padrão e funcionam com o jogo em foco:

| Tecla | Ação |
| --- | --- |
| `Num1` a `Num9`, `Num0` | Os 10 primeiros pads da interface ativa |
| `Num+` / `Num-` | Próxima / anterior interface |
| `Num,` | Parar tudo |
| `Num*` | Ligar ou desligar o efeito de voz |
| `Num/` | Próximo efeito de voz |

Em **Ajustes** você pode trocar por `Ctrl` mais o teclado numérico, ou `Alt` mais `1` a `0`, ou desligar. Deixe o NumLock ligado. Um atalho definido num pad tem prioridade sobre esses.

### Idioma

O app segue o idioma do sistema e vem em português e inglês. Dá para mudar em **Ajustes**, em **Idioma**.

## Solução de problemas

Se o som está robótico, distorcido ou não aparece no Windows, primeiro veja se ele chega ao cabo. Abra o painel de som do Windows, vá em **Gravação** e observe a barra de nível do `CABLE Output` enquanto um pad toca. Se a barra se mexe, o problema está nas configurações do jogo ou do Discord. Se fica parada, o problema está no PC.

Confira nesta ordem:

1. Você reiniciou o PC depois de instalar o VB-CABLE? O Windows mostra o driver como funcionando antes da reinicialização, mas ele não está.
2. No Gerenciador de Dispositivos, procure uma entrada `VB-Audio Virtual Cable` com erro. Se houver, ou se o serviço de áudio do Windows fica parando, reinstale o driver: execute `VBCABLE_Setup_x64.exe` como administrador, clique em **Remove Driver**, reinicie, execute de novo, clique em **Install Driver** e reinicie.
3. O dispositivo de saída do sistema precisa ser os seus fones, não o cabo. Caso contrário, todos os apps do PC entram no seu microfone.
4. Fones Bluetooth caem para qualidade de chamada sempre que um app usa o microfone deles. Use um microfone com fio ou USB e confira se o Discord e o jogo não estão lendo o mic do fone.
5. Clique com o botão direito em `CABLE Input` e `CABLE Output` no painel de som do Windows e veja se os dois usam a mesma taxa de amostragem. O VB-CABLE distorce quando são diferentes. 48000 Hz funciona bem.
6. Abaixe os knobs **Mic** e **Jogo** até o medidor ficar fora do vermelho.

Outros casos:

- Os **Ajustes** dizem que nenhum cabo virtual foi encontrado: no Windows e no macOS, instale como descrito acima. No Linux, confira se o `pactl` está instalado.
- Um dispositivo que você acabou de conectar não aparece nas listas: clique em **Atualizar dispositivos** em **Ajustes**.
- Um atalho não faz nada enquanto um jogo roda como administrador: execute o InstantPad como administrador também.

## Desenvolvimento

Você precisa de Rust, Node 22 e pnpm. No Linux, também precisa das [bibliotecas de sistema do Tauri](https://tauri.app/start/prerequisites/).

```bash
pnpm install
pnpm dev
```

O [CONTRIBUTING.md](CONTRIBUTING.md) explica a estrutura do projeto, os testes e o padrão de commits. A pasta [docs/architecture](docs/architecture/README.md) descreve como o código é organizado.

## Limitações

No Windows o dispositivo virtual é o VB-CABLE, da VB-Audio. O Windows só carrega drivers de áudio assinados pela Microsoft, então o app não consegue criar o próprio. Ele baixa o instalador do site oficial e só o executa se a assinatura digital for da VB-Audio. O VB-CABLE é gratuito, com doação opcional.

No macOS o app não é notarizado, e você mesmo instala o cabo virtual (BlackHole).

O MyInstants não tem API pública, então o app lê o HTML do site. Se o site mudar, o arquivo a corrigir é o [`parser.rs`](crates/catalog/src/parser.rs).

Um jogo rodando como administrador pode bloquear atalhos globais. Nesse caso, execute o InstantPad como administrador também.

Algumas teclas dos teclados brasileiros ABNT2, como a `/` ao lado do Shift direito, não podem ser atalhos globais no Windows.

No Linux o cabo virtual continua depois que você fecha o app, até você encerrar a sessão, e a próxima abertura reaproveita o mesmo cabo.

## Licença

[MIT](LICENSE). O app usa a fonte [Lexend](https://www.lexend.com/) sob a [SIL Open Font License](apps/desktop/src/shared/styles/fonts/OFL.txt).
