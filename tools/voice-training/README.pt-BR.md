# Treinar uma voz

*[Read in English](README.md)*

Estes scripts treinam uma voz de IA com o [RVC](https://github.com/RVC-Project/Retrieval-based-Voice-Conversion-WebUI)
e a exportam do jeito que o InstantPad carrega. São eles que fazem a voz feminina do app, e
qualquer outra voz a partir das suas gravações.

Rodam no Linux e no Windows pelo WSL 2. Com uma placa de vídeo NVIDIA recente a voz feminina
treina em cerca de uma hora; sem ela o treino roda no processador, o que funciona, mas leva mais
de um dia.

## Antes de começar

No WSL, instale o driver da NVIDIA no **Windows**, não dentro do Linux, e confira que o
`nvidia-smi` funciona no terminal do Linux. Depois, no Linux:

```bash
sudo apt install git ffmpeg
```

```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
```

## Treinar a voz feminina

```bash
tools/voice-training/setup.sh
```

```bash
~/instantpad-voice-training/venv/bin/python tools/voice-training/voicekit.py all --name feminine
```

`setup.sh --cpu` prepara um computador sem placa de vídeo.

O `all` roda três etapas, que também rodam uma a uma, e cada uma pode ser rodada de novo depois
de uma parada:

| Etapa | O que faz |
| --- | --- |
| `dataset` | Baixa 400 MB do [Hi-Fi TTS](https://www.openslr.org/109/) e separa 40 minutos da leitora 92 (`--minutes` muda isso). |
| `train` | Corta a fala em pedaços, acha o tom e as palavras de cada um e treina a partir do modelo base do RVC: 200 épocas (`--epochs`), 8 pedaços por vez (`--batch`; diminua se faltar memória na placa). Um treino interrompido continua do último ponto salvo. |
| `export` | Grava a voz para o app em `~/instantpad-voice-training/voices/feminine/app` e confere os arquivos ONNX contra o PyTorch. |

Para ouvir, converta uma gravação sua:

```bash
~/instantpad-voice-training/venv/bin/python tools/voice-training/voicekit.py try --name feminine eu.wav
```

Ele grava `eu.feminine.wav` ao lado, com o seu tom levado ao da voz. `--shift` define os
semitons em vez disso.

## Treinar uma voz com as suas gravações

Ponha de 10 a 40 minutos de uma pessoa falando numa pasta, em qualquer formato que o ffmpeg leia,
sem música e com o mínimo de ruído possível, e então:

```bash
~/instantpad-voice-training/venv/bin/python tools/voice-training/voicekit.py all --name minha-voz --folder ~/gravacoes --credit "Gravações da Ana"
```

## O que o app recebe

| Arquivo | O que é |
| --- | --- |
| `voice.json` | A voz: a taxa de amostragem, o tom em que ela costuma falar, os arquivos abaixo com o SHA-256 de cada um, e o crédito. |
| `synthesizer.onnx` | A voz em si, uns 110 MB. |
| `content.onnx` | O HuBERT, que entende o que é dito. É o mesmo para todas as vozes. |

O formato das entradas e saídas está no [README em inglês](README.md#what-the-app-gets).

## Fontes e licenças

- RVC, seus scripts e modelos base: MIT. Os modelos base foram treinados no VCTK (CC BY 4.0).
- HuBERT base como o RVC o distribui: MIT.
- Hi-Fi TTS: CC BY 4.0, Evelina Bakhturina, Vitaly Lavrukhin, Boris Ginsburg e Yang Zhang,
  *Hi-Fi Multi-Speaker English TTS Dataset*, 2021. As gravações vêm do LibriVox, em domínio
  público.

Cada download é fixado pelo tamanho e pelo SHA-256, e o RVC pelo commit, então o que roda é o
que foi conferido.
