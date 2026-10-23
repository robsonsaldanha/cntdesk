<p align="center">
  <img src="res/logo-header.svg" alt="CNT Desk - Remote Desktop"><br>
  <a href="#raw-steps-to-build">Build</a> •
  <a href="#how-to-build-with-docker">Docker</a> •
  <a href="#file-structure">Structure</a> •
  <a href="#screenshots">Screenshots</a>
</p>

> [!NOTE]
> **Baseado em código aberto:** CNT Desk é um fork do projeto open-source [RustDesk](https://github.com/rustdesk/rustdesk), licenciado sob AGPL-3.0. Agradecemos à comunidade RustDesk por disponibilizar o código-fonte que torna este projeto possível.

> [!Caution]
> **Aviso de Uso:** Os desenvolvedores do CNT Desk não concordam nem apoiam qualquer uso antiético ou ilegal deste software. Uso indevido, como acesso não autorizado, controle ou invasão de privacidade, é estritamente contra nossas diretrizes. Os autores não são responsáveis por qualquer uso indevido do aplicativo.

Solução de acesso remoto baseada no código-fonte aberto do RustDesk, escrita em Rust. Funciona imediatamente sem configuração necessária. Você tem controle total dos seus dados, sem preocupações com segurança. Utilizamos nosso próprio servidor rendezvous/relay hospedado em nossa infraestrutura (VPS).

![image](https://user-images.githubusercontent.com/71636191/171661982-430285f0-2e12-4b1d-9957-4a58e375304d.png)

## Servidor

O CNT Desk conecta-se exclusivamente ao nosso servidor rendezvous/relay próprio, garantindo que todo o tráfego passe por nossa infraestrutura. Não utilizamos os servidores públicos do RustDesk.

## Dependências

Versões desktop usam Flutter para GUI. Consulte nosso [CI](.github/workflows/build-windows.yml) para detalhes sobre o build.

## Raw Steps to build

- Prepare seu ambiente de desenvolvimento Rust e C++

- Instale [vcpkg](https://github.com/microsoft/vcpkg) e configure a variável `VCPKG_ROOT` corretamente

  - Windows: vcpkg install libvpx:x64-windows-static libyuv:x64-windows-static opus:x64-windows-static aom:x64-windows-static
  - Linux/macOS: vcpkg install libvpx libyuv opus aom

- Execute `cargo run`

## How to Build on Linux

### Ubuntu 18 (Debian 10)

```sh
sudo apt install -y zip g++ gcc git curl wget nasm yasm libgtk-3-dev clang libxcb-randr0-dev libxdo-dev \
        libxfixes-dev libxcb-shape0-dev libxcb-xfixes0-dev libasound2-dev libpulse-dev cmake make \
        libclang-dev ninja-build libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

### openSUSE Tumbleweed

```sh
sudo zypper install gcc-c++ git curl wget nasm yasm gcc gtk3-devel clang libxcb-devel libXfixes-devel cmake alsa-lib-devel gstreamer-devel gstreamer-plugins-base-devel xdotool-devel
```

### Fedora 28 (CentOS 8)

```sh
sudo yum -y install gcc-c++ git curl wget nasm yasm gcc gtk3-devel clang libxcb-devel libxdo-devel libXfixes-devel pulseaudio-libs-devel cmake alsa-lib-devel gstreamer1-devel gstreamer1-plugins-base-devel
```

### Arch (Manjaro)

```sh
sudo pacman -Syu --needed unzip git cmake gcc curl wget yasm nasm zip make pkg-config clang gtk3 xdotool libxcb libxfixes alsa-lib pipewire
```

### Install vcpkg

```sh
git clone https://github.com/microsoft/vcpkg
cd vcpkg
git checkout 2023.04.15
cd ..
vcpkg/bootstrap-vcpkg.sh
export VCPKG_ROOT=$HOME/vcpkg
vcpkg/vcpkg install libvpx libyuv opus aom
```

### Build

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
git clone --recurse-submodules https://github.com/robsonsaldanha/cntdesk
cd cntdesk
mkdir -p target/debug
VCPKG_ROOT=$HOME/vcpkg cargo run
```

## How to build with Docker

```sh
git clone https://github.com/robsonsaldanha/cntdesk
cd cntdesk
git submodule update --init --recursive
docker build -t "cntdesk-builder" .
```

Então, sempre que precisar compilar:

```sh
docker run --rm -it -v $PWD:/home/user/cntdesk -v cntdesk-git-cache:/home/user/.cargo/git -v cntdesk-registry-cache:/home/user/.cargo/registry -e PUID="$(id -u)" -e PGID="$(id -g)" cntdesk-builder
```

## File Structure

- **[libs/hbb_common](libs/hbb_common)**: video codec, config, tcp/udp wrapper e funções utilitárias compartilhadas com o servidor
- **[libs/base](libs/base)**: protobuf, funções fs para transferência de arquivos, teclado e código de plataforma usado apenas por este app
- **[libs/scrap](libs/scrap)**: captura de tela
- **[libs/enigo](libs/enigo)**: controle de teclado/mouse específico por plataforma
- **[libs/clipboard](libs/clipboard)**: implementação de cópia e cola de arquivos para Windows, Linux, macOS
- **[src/server](src/server)**: serviços de áudio/clipboard/input/video e conexões de rede
- **[src/client.rs](src/client.rs)**: inicia conexão peer
- **[src/rendezvous_mediator.rs](src/rendezvous_mediator.rs)**: Comunica com nosso servidor rendezvous, aguarda conexão direta (TCP hole punching) ou relayed
- **[src/platform](src/platform)**: código específico por plataforma
- **[flutter](flutter)**: Código Flutter para desktop e mobile

## Screenshots

![Connection Manager](https://github.com/rustdesk/rustdesk/assets/28412477/db82d4e7-c4bc-4823-8e6f-6af7eadf7651)

![Connected to a Windows PC](https://github.com/rustdesk/rustdesk/assets/28412477/9baa91e9-3362-4d06-aa1a-7518edcbd7ea)

![File Transfer](https://github.com/rustdesk/rustdesk/assets/28412477/39511ad3-aa9a-4f8c-8947-1cce286a46ad)

![TCP Tunneling](https://github.com/rustdesk/rustdesk/assets/28412477/78e8708f-e87e-4570-8373-1360033ea6c5)

## Licença

Este projeto é licenciado sob AGPL-3.0, herdado do projeto original RustDesk. Consulte o arquivo LICENSE para detalhes.

## Atribuição

CNT Desk é baseado no projeto open-source [RustDesk](https://github.com/rustdesk/rustdesk). Agradecemos aos contribuidores do RustDesk por tornarem este projeto possível através da disponibilização do código-fonte sob licença AGPL-3.0.