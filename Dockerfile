FROM debian:12

RUN apt-get -y update && \
    apt-get -y install curl gnupg procps && \
    curl -s https://arkanosis.net/jroquet.pub.asc | tee /usr/share/keyrings/arkanosis.asc && \
    echo 'deb [arch=amd64 signed-by=/usr/share/keyrings/arkanosis.asc] https://apt.arkanosis.net/ stable main' | tee /etc/apt/sources.list.d/arkanosis.list && \
    apt-get -y update && \
    apt-get -y install bamrescue && \
    apt-get -y clean

CMD bamrescue --help
