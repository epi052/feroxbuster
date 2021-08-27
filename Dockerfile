# Image: alpine:3.14.2
FROM alpine@sha256:69704ef328d05a9f806b6b8502915e6a0a4faa4d72018dc42343f511490daf8a as build
LABEL maintainer="wfnintr@null.net"

RUN sed -i -e 's/v[[:digit:]]\..*\//edge\//g' /etc/apk/repositories \
    && apk upgrade --update-cache --available

# Download default wordlists 
RUN apk add --no-cache --virtual .depends subversion font-noto-emoji \
    && svn export https://github.com/danielmiessler/SecLists/trunk/Discovery/Web-Content /usr/share/seclists/Discovery/Web-Content

# Download latest release
RUN wget https://github.com/epi052/feroxbuster/releases/latest/download/x86_64-linux-feroxbuster.zip -qO feroxbuster.zip \
    && unzip -d /usr/local/bin/ feroxbuster.zip feroxbuster \
    && chmod +x /usr/local/bin/feroxbuster

# Image: alpine:3.14.2
FROM alpine@sha256:69704ef328d05a9f806b6b8502915e6a0a4faa4d72018dc42343f511490daf8a as release

COPY --from=build /usr/share/seclists/Discovery/Web-Content /usr/share/seclists/Discovery/Web-Content
COPY --from=build /usr/local/bin/feroxbuster /usr/local/bin/feroxbuster

RUN adduser \
    --gecos "" \
    --disabled-password \
    feroxbuster

USER feroxbuster

ENTRYPOINT ["feroxbuster"]
