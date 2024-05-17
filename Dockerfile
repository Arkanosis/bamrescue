FROM debian:latest

RUN apt-get -y update && \
	apt-get -y install curl apt-transport-https gnupg && \
	curl -s https://arkanosis.net/jroquet.pub.asc | apt-key add - && \
	echo "deb https://apt.arkanosis.net/ software stable" | tee /etc/apt/sources.list.d/arkanosis.list && \
	apt-get -y update && \
	apt-get -y install bamrescue && \
	apt-get -y clean

CMD bamrescue --help
