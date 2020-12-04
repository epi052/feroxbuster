FROM alpine:latest
LABEL maintainer="wfnintr@null.net"

RUN sed -i -e 's/v[[:digit:]]\..*\//edge\//g' /etc/apk/repositories && apk upgrade --update-cache --available

# download default wordlists 
RUN apk add --no-cache --virtual .depends subversion font-noto-emoji && \
	svn export https://github.com/danielmiessler/SecLists/trunk/Discovery/Web-Content /usr/share/seclists/Discovery/Web-Content && \
	apk del .depends

# install latest release
RUN wget https://github.com/epi052/feroxbuster/releases/latest/download/x86_64-linux-feroxbuster.zip -qO feroxbuster.zip && unzip -d /usr/local/bin/ feroxbuster.zip feroxbuster && rm feroxbuster.zip && chmod +x /usr/local/bin/feroxbuster

ENTRYPOINT ["feroxbuster"]
