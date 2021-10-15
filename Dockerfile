# Image: alpine:3.14.2
FROM alpine@sha256:69704ef328d05a9f806b6b8502915e6a0a4faa4d72018dc42343f511490daf8a as build
LABEL maintainer="wfnintr@null.net"

RUN sed -i -e 's/v[[:digit:]]\..*\//edge\//g' /etc/apk/repositories \
    && apk upgrade --update-cache --available && apk add --update openssl


# Download latest release
RUN wget https://github.com/epi052/feroxbuster/releases/latest/download/x86_64-linux-feroxbuster.zip -qO feroxbuster.zip \
    && unzip -d /tmp/ feroxbuster.zip feroxbuster \
    && chmod +x /tmp/feroxbuster \
    && wget https://raw.githubusercontent.com/danielmiessler/SecLists/master/Discovery/Web-Content/raft-medium-directories.txt -O /tmp/raft-medium-directories.txt

# Image: alpine:3.14.2
FROM alpine@sha256:69704ef328d05a9f806b6b8502915e6a0a4faa4d72018dc42343f511490daf8a as release

COPY --from=build /tmp/raft-medium-directories.txt /usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt
COPY --from=build /tmp/feroxbuster /usr/local/bin/feroxbuster

RUN adduser \
    --gecos "" \
    --disabled-password \
    feroxbuster

USER feroxbuster

ENTRYPOINT ["feroxbuster"]
