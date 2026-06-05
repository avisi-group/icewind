FROM rust:latest

RUN apt-get update
RUN apt-get install -y build-essential device-tree-compiler opam mold fish neovim syncthing gcc-aarch64-linux-gnu gcc-arm-none-eabi z3 bison flex bc strace less qemu-system qemu-user screen

RUN opam init --disable-sandboxing --bare -y
RUN opam switch create 4.14.1+options --packages ocaml-variants.4.14.1+options,ocaml-option-flambda

RUN opam install -y sail=0.17.1 gmp dune ocaml-lsp-server

RUN mkdir -p ~/.config/fish
RUN echo '# Environment variables\n\
set fish_greeting\n\
set -x EDITOR vi\n\
set -x RUST_MIN_STACK 1073741824\n\
alias cargo="mold -run cargo"\n\
# PATH\n\
set PATH /usr/local/cargo/bin $PATH\n\
set PATH $HOME/.local/bin $PATH\n\
# opam configuration\n\
source /root/.opam/opam-init/init.fish > /dev/null 2> /dev/null; or true\n\
eval (opam env 2> /dev/null)' > ~/.config/fish/config.fish

RUN echo '[build]\n\
target-dir = "/usr/local/cargo/target"' > /usr/local/cargo/config.toml

RUN cargo install dua-cli bottom

RUN git config --global user.email "fm208@st-andrews.ac.uk"
RUN git config --global user.name "Ferdia McKeogh"

RUN ulimit -s 65536

RUN chsh -s /usr/bin/fish
ENV SHELL /usr/bin/fish
ENV LANG=C.UTF-8 LANGUAGE=C.UTF-8 LC_ALL=C.UTF-8
ENTRYPOINT [ "fish" ]
