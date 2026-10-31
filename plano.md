# Refactor do InstantPad: C#/WPF → Rust + Tauri 2

> ## Decisões aprovadas em 2026-10-30 (têm prioridade sobre o texto abaixo)
>
> - **Frontend:** React 19 + TS, não Svelte. `shared/ipc` continua sendo a única porta para o backend.
> - **Estado no Rust:** a lógica dos ViewModels mora em `crates/application`. O React só desenha snapshots e envia intenções.
> - **Dados:** formato novo, sem compatibilidade com o JSON do C#, sem migração de bancos e sem versão ponte. A regra 4 e a ponte da Fase 3 não valem mais. Não há usuários ativos.
> - **Atalhos:** salvos como `KeyboardEvent.code` (`Numpad1`, `KeyA`), que funciona nos três SOs.
> - **Áudio:** sem `.wma`. Saída em 48 kHz estéreo, com a conversão do `cpal`/WASAPI.
> - **Interface:** menus e diálogos em HTML. O layout segue a proposta do WPF, sem exigir igualdade de pixels.
> - **Plataformas:** Windows, macOS e Linux geram executáveis agora, e o core é validado primeiro no macOS. Os adaptadores de macOS e Linux entram na Fase 1. O exe portátil continua, com sufixo `-portable`.
> - **Updates:** o `tauri-plugin-updater` cobre busca e instalação pela porta `AppUpdater`, então **não existe `crates/updates`**.
> - **Fluxo:** subagentes Sonnet, cada frente abre PR para `refactor/tauri`, e um revisor independente confere tudo no fim.
> - **Código enxuto:** nada de overengineering nem de métodos sem uso.

## Contexto

Este repositório é o InstantPad, um soundboard para Windows com visual de sampler de hardware e o MyInstants integrado. Ele mistura o microfone com os sons e envia o resultado para um cabo virtual (VB-CABLE), que o jogo ou o Discord usa como microfone. Hoje o app é C#/.NET 8 com WPF, em camadas:

- `src/InstantPad.Domain`: regras puras (PadBoard, PadInterface, Hotkey, HexColor, NumericShortcuts…)
- `src/InstantPad.Application`: casos de uso e portas (`ISoundPlayer`, `IAudioRouter`, `IAudioMix`, `IVirtualCable`, `IHotkeyHost`, repositórios, updates, localização .resx)
- `src/InstantPad.Infrastructure`: NAudio/WASAPI, mixer, MyInstants, persistência JSON, updater, VB-CABLE
- `src/InstantPad.Desktop`: WPF (Views, ViewModels, tema `Themes/Hardware.xaml`, controles `Knob` e `LedMeter`)
- `tests/`, `build/installer.iss`, `.github/workflows/`

Objetivo: reescrever em **Rust + Tauri 2**, com a **interface visualmente idêntica** à atual e **todas as funcionalidades**. O Windows vem primeiro e é a meta de paridade. Linux e macOS entram depois, como adaptadores novos.

## Regras inegociáveis

1. **Não apague nem mova o código C#** (`src/`, `tests/`, `build/`, workflows atuais). Ele continua publicando releases até a versão nova ter paridade. O código novo fica em pastas novas na raiz.
2. **Interface idêntica.** As fontes da verdade são `src/InstantPad.Desktop/Views/MainWindow.xaml`, `Views/InterfaceDialog.xaml`, `Views/BoardMenus.cs`, `Views/SmallDialogs.cs`, `Themes/Hardware.xaml`, `Controls/Knob.cs` e `Controls/LedMeter.cs`. Mantenha cores, gradientes, sombras, espaçamentos, tamanhos, fonte Lexend (em `Assets/Fonts`), textos, menus de contexto e interações. Extraia toda cor e medida para tokens CSS; nada de valor mágico espalhado.
3. **O áudio nunca passa pelo webview.** A interface manda comandos e recebe eventos (níveis dos medidores, ~30 Hz). Na thread de áudio: sem alocação, sem lock bloqueante, sem I/O. Use ring buffer lock-free e sons pré-decodificados.
4. **Compatibilidade de dados.** Leia `src/InstantPad.Infrastructure/Persistence/*.cs` e `Settings/AppSettings.cs` e use **a mesma pasta e o mesmo formato JSON**, para quem já usa não perder boards, favoritos, sons e configurações. Crie fixtures de teste com JSON real gerado pela versão C#.
5. **Preserve o comportamento de áudio.** Leia com atenção `Audio/AudioEngine.cs`, `Mixer.cs`, `MicRingBuffer.cs`, `ClipPlayer.cs` e o histórico do git (por exemplo, o commit "stop boosting the mic"). Ganhos, limites, conversão de formato e reação a dispositivo que some precisam se comportar igual.
6. **Funcionalidades para paridade** (confira no README e no código): pads com atalho, volume, cor e nome; interfaces com abas (até 8×10), criar, mover e excluir; reorganizar com Ctrl+arrastar; arrastar arquivos do Explorer para o pad; busca no MyInstants com ▶ (prévia só no fone), "Mic" (toca no jogo sem salvar), ↓ (baixar), ☆ (favoritar) e "+ Pad"; knobs Mic, Game e Headphones (arrastar, roda do mouse, duplo clique reseta); medidores LED; atalhos numéricos globais e suas variações (Ctrl+Num, Alt+1..0, desligado); Num+ e Num- trocam de interface; Num, para tudo; instância única que traz a janela para frente; pt/en; aviso quando a saída do Windows é o CABLE Input; instalação do VB-CABLE com verificação de assinatura do fornecedor; checagem e instalação de updates.
7. **SOLID**, aplicado assim:
   - **S:** cada crate e cada módulo tem uma responsabilidade. Os commands do Tauri são finos: validam, chamam um caso de uso e devolvem o resultado.
   - **O:** uma plataforma nova é um adaptador novo que implementa as traits existentes, sem mexer em domínio nem em aplicação.
   - **L:** todo adaptador cumpre o contrato da trait. Escreva testes de contrato genéricos sobre a trait e rode-os contra cada implementação, inclusive os fakes.
   - **I:** traits pequenas e específicas, espelhando as portas atuais do C# (`SoundPlayer`, `AudioRouter`, `AudioMix`, `VirtualCable`, `HotkeyHost`, repositórios, `UpdateSource`, `UpdateInstaller`). Nada de trait "Deus".
   - **D:** `domain` e `application` dependem só de abstrações. **Nunca** podem depender de `tauri`, `cpal`, `reqwest` ou `windows`. A ligação das implementações concretas acontece apenas no composition root (`apps/desktop/src-tauri`). Adicione um teste ou checagem no CI que falhe se essas dependências aparecerem nesses crates.
8. Qualidade: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, e no frontend `pnpm check`, `pnpm lint` e `pnpm test`. Faça commits pequenos com mensagem clara e não pule nem desative testes.

## Estrutura de pastas (padrão de monorepo grande)

```
InstantPad/
├─ Cargo.toml                    # [workspace]: membros, versões e lints compartilhados
├─ rust-toolchain.toml           # fixa a versão do Rust
├─ package.json                  # scripts de raiz (dev, build, lint, test)
├─ pnpm-workspace.yaml           # workspaces JS: apps/* e packages/*
│
├─ apps/                         # o que vira executável
│  └─ desktop/                   # o app Tauri
│     ├─ src/                    # frontend (Svelte 5 + TypeScript + Vite)
│     │  ├─ app/                 # bootstrap, layout raiz, providers, janela principal
│     │  ├─ features/            # uma pasta por funcionalidade, com tudo dela junto
│     │  │  ├─ board/            #   pads, grid, abas de interface, menus de contexto do pad
│     │  │  ├─ browser/          #   busca MyInstants e linhas de resultado
│     │  │  ├─ favorites/
│     │  │  ├─ mixer/            #   knobs e medidores
│     │  │  ├─ setup/            #   configurações, VB-CABLE, dispositivos, atalhos
│     │  │  └─ updates/
│     │  │     # cada feature: components/, stores/ (estado), api.ts (chamadas IPC), *.test.ts
│     │  └─ shared/              # só o que mais de uma feature usa
│     │     ├─ ui/               #   Knob, LedMeter, Button, Dialog, ContextMenu, Tabs
│     │     ├─ styles/           #   tokens.css (do Hardware.xaml), fonts/, reset.css
│     │     ├─ i18n/             #   en.json, pt.json (convertidos dos .resx)
│     │     └─ ipc/              #   bindings TS gerados (tauri-specta) + mock p/ rodar no navegador
│     ├─ src-tauri/              # composition root (crate binário)
│     │  ├─ src/
│     │  │  ├─ main.rs           #   só chama lib::run()
│     │  │  ├─ lib.rs            #   monta o app: plugins, estado, commands
│     │  │  ├─ composition.rs    #   instancia os adaptadores concretos de cada SO
│     │  │  ├─ commands/         #   handlers IPC finos, um arquivo por feature
│     │  │  └─ events.rs         #   eventos emitidos para a UI (níveis, falha de dispositivo)
│     │  ├─ capabilities/        #   permissões do Tauri 2 (mínimo necessário)
│     │  ├─ icons/
│     │  └─ tauri.conf.json
│     └─ tests/e2e/              # Playwright contra o frontend com IPC mockado
│
├─ crates/                       # bibliotecas Rust, prefixo instantpad-
│  ├─ domain/                    # entidades e regras puras, sem I/O e sem async
│  ├─ application/               # casos de uso + traits (portas) + erros de aplicação
│  ├─ audio/                     # engine: cpal (E/S), symphonia (decodificação), mixer, ring buffer
│  ├─ catalog/                   # MyInstants: reqwest + scraper
│  ├─ storage/                   # repositórios JSON compatíveis com a versão C#
│  ├─ updates/                   # GitHub Releases (fonte de versões); instalação via tauri-plugin-updater
│  ├─ platform/                  # adaptadores por SO atrás de traits, com #[cfg(target_os)]
│  │  └─ src/{windows,linux,macos}/  # cabo virtual, roteamento, detecção da saída do sistema
│  └─ test-support/              # fakes das traits, fixtures, testes de contrato reutilizáveis
│
├─ packages/                     # código JS compartilhado (se surgir; vazio no começo)
├─ docs/
│  ├─ architecture/              # visão geral, diagrama de dependências entre crates
│  ├─ adr/                       # decisões (0001-tauri.md, 0002-audio-engine.md, …)
│  └─ reference-screenshots/     # prints da versão WPF para comparar a UI
├─ scripts/                      # automações (gerar bindings, converter .resx → json)
│
├─ src/ tests/ build/            # C# atual: intocado até a paridade
└─ .github/workflows/            # CI novo: rust.yml e desktop.yml, sem alterar os atuais
```

Os crates ficam em `crates/<nome>`, mas o nome do pacote é `instantpad-<nome>`.

Direção das dependências (nunca ao contrário):
`domain` ← `application` ← (`audio`, `catalog`, `storage`, `updates`, `platform`) ← `apps/desktop/src-tauri`.
O frontend só conhece `shared/ipc`.

## Como trabalhar em paralelo

Use sua lista de tarefas para acompanhar o andamento, e subagentes com `isolation: "worktree"` para as frentes paralelas. Cada frente roda no próprio worktree e branch (`refactor/tauri-<frente>`), criado a partir de `refactor/tauri`.

### Fase 0: contratos (você mesmo, sequencial, antes de qualquer paralelismo)
1. Crie o branch `refactor/tauri`.
2. Faça o scaffold do workspace Cargo, do pnpm e do app Tauri 2 com Svelte e TS, com toda a estrutura acima (pastas vazias com README de uma linha dizendo o que cada uma guarda).
3. Porte `crates/domain` (é pequeno e tudo depende dele) com os testes equivalentes aos de `tests/`.
4. Escreva **todas as traits** em `crates/application` e os **tipos de IPC** (commands e eventos com `specta`, gerando `shared/ipc`), mais o mock de IPC para o navegador.
5. Extraia os tokens visuais de `Hardware.xaml` e `Controls/*.cs` para `shared/styles/tokens.css`.
6. Configure CI, lints e a checagem de dependências proibidas. Faça commit.

Daqui em diante, **só você altera contratos** (traits, tipos de IPC e tokens). Se uma frente precisar mudar um contrato, ela para e te pede.

### Fase 1: frentes paralelas (subagentes; cada uma mexe só nas suas pastas)
| Frente | Pastas | Pronto quando |
| --- | --- | --- |
| ui | `apps/desktop/src/**` | todas as telas, menus e diálogos reproduzidos e funcionando com o mock de IPC no navegador; screenshots do Playwright conferidos com `docs/reference-screenshots/` |
| audio | `crates/audio` | toca, pré-carrega, para e mistura mic + sons para duas saídas (jogo e fone); medidores; reage a dispositivo que some; testes de contrato passando; benchmark mostrando zero alocação no callback |
| storage | `crates/storage` | lê e grava os JSON da versão C# com as fixtures; migração tolerante a campos ausentes |
| catalog | `crates/catalog` | busca, paginação e download equivalentes a `MyInstantsCatalog`/`MyInstantsPageParser`, com HTML de fixture |
| platform-windows | `crates/platform/src/windows`, `crates/updates` | VB-CABLE (baixar, verificar assinatura com WinVerifyTrust, abrir com elevação), detecção da saída padrão do sistema, fonte de updates |

Cada subagente recebe: o objetivo da frente, as pastas permitidas, os contratos da Fase 0, os arquivos C# de referência e o critério de pronto. Antes de devolver, ele roda `fmt`, `clippy` e os testes do seu escopo. Você revisa o diff e faz o merge em `refactor/tauri` uma frente por vez, rodando a suíte inteira depois de cada merge.

### Fase 2: integração (sequencial)
Ligue tudo em `src-tauri`: `composition.rs`, commands, eventos dos medidores, atalhos globais (`tauri-plugin-global-shortcut`), instância única (`tauri-plugin-single-instance`) e arrastar arquivos (eventos de drag-drop do Tauri). Rode `pnpm tauri dev` no Windows e passe pela lista de funcionalidades da regra 6, item por item.

### Fase 3: distribuição
Configure `tauri-plugin-updater` com assinatura de updates, o instalador NSIS e um workflow de release novo. Planeje a **versão ponte**: uma última release em C# cujo updater leve o usuário para o instalador novo. Documente isso numa ADR.

### Fase 4: Linux e macOS (frentes paralelas)
- `platform/linux`: sink virtual no PipeWire/PulseAudio (`module-null-sink` + `module-remap-source`), criado ao abrir e removido ao fechar; AppImage. Atalhos: X11 funciona; no Wayland, use o portal GlobalShortcuts e documente a limitação.
- `platform/macos`: fluxo de instalação do BlackHole, permissões de Acessibilidade, `.dmg` assinado e notarizado.

Só remova o código C# depois da paridade no Windows e com a minha aprovação explícita.

## Como começar
Leia o README, o CONTRIBUTING e os arquivos citados. Depois me mostre um plano curto da Fase 0, com versões de crates e libs, e as dúvidas que tiver. Comece a executar só depois que eu aprovar.
