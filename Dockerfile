FROM rust:latest

ENV TERM=xterm-256color

COPY target/release/flik /usr/local/bin/flik

WORKDIR /workspace

ENTRYPOINT ["flik"]
CMD ["--help"]
