# Reel do InstantPad para o TikTok

`instantpad_reel_tiktok.mp4`: 1080×1920, 30 fps, 41 s, áudio AAC normalizado para redes sociais.

## Roteiro

Estrutura: gancho (0–3 s), demonstração, benefícios e chamada para ação. Legendas queimadas ajudam quem assiste sem som.

| Tempo | Fala | Na tela |
| --- | --- | --- |
| 0–7 s | "Sabe aquele momento na call que tá pedindo um meme... e você não tem nenhum pra soltar? Pois é. Agora tem." | Título "FALTOU MEME NA CALL?" e câmera sobre os pads |
| 7–11 s | (sem fala) | Vine boom, Airhorn e Sad trombone tocados nos pads, com o som de cada um |
| 11–17 s | "Esse aqui é o InstantPad. Ele tem cara de mesa de som, e já vem com toda a biblioteca do MyInstants dentro." | Logo e depois a câmera vai até a lista "Em alta: Brasil" |
| 17–21.6 s | "Você procura o som, arrasta pro pad... e pronto." | Busca por "Applause", arrasta até o pad 09 e o toca (aplausos) |
| 21.6–26 s | "Apertou, seus amigos ouvem, junto com a sua voz, no Discord ou no jogo." | Diagrama montado no ritmo da fala |
| 26–29.6 s | "E dá até pra deixar a voz grossa... ou fininha." | Tela Voice: Voz grossa e depois Voz fina. A fala passa pelo efeito de voz do próprio InstantPad |
| 29.6–35 s | "Ah, e o melhor: é de graça. Código aberto, roda no Windows, no Mac e no Linux." | "É DE GRAÇA", "Código aberto", Windows, macOS, Linux |
| 35–41 s | "Baixa aí, e depois me conta: qual vai ser o primeiro som que você vai soltar na call?" | Logo, "Baixe grátis", link do GitHub e um Vine boom final |

## Como foi feito

- Narração: ElevenLabs, voz "Gabriel – Shorts & Reels" (pt-BR nativo), modelo `eleven_v4`.
- Efeitos dos pads e trilha instrumental: ElevenLabs Sound Effects e Music.
- Voz grossa e voz fina: `source/vfx` roda a mesma biblioteca e os mesmos parâmetros de `crates/audio/src/voice.rs` (signalsmith-stretch, janela de 40 ms, valores padrão dos knobs: −5,5 / +5,5 semitons e formante −2,5 / +2,5).
- Imagem: a interface real (o mock de navegador do app, em pt-BR) dentro de um palco 1080×1920 (`source/stage.html`) com câmera animada. `source/perform.cjs` toca os pads, faz a busca, arrasta o som e troca os efeitos numa tomada contínua, gravada com o screencast do Chromium.
- `source/build.py` mixa narração, efeitos e trilha (com ducking), sincroniza os sons com os cliques medidos na gravação e queima as legendas.

Os scripts são a referência de como o vídeo foi gerado. Eles usam caminhos de uma pasta de trabalho temporária e os áudios gerados no ElevenLabs, que não estão no repositório.
