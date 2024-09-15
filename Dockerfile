FROM alpine:3.17.1 AS build
LABEL maintainer="wfnintr@null.net"

RUN apk upgrade --update-cache --available && apk add --update openssl

# Download latest release
RUN wget https://github.com/epi052/feroxbuster/releases/latest/download/x86_64-linux-feroxbuster.zip -qO feroxbuster.zip \
    && unzip -d /tmp/ feroxbuster.zip feroxbuster \
    && chmod +x /tmp/feroxbuster \
    && wget https://raw.githubusercontent.com/danielmiessler/SecLists/master/Discovery/Web-Content/raft-medium-directories.txt -O /tmp/raft-medium-directories.txt

FROM alpine:3.17.1 AS release
COPY --from=build /tmp/raft-medium-directories.txt /usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt
COPY --from=build /tmp/feroxbuster /usr/local/bin/feroxbuster

RUN adduser \
    --gecos "" \
    --disabled-password \
    feroxbuster

USER feroxbuster

ENTRYPOINT ["feroxbuster"]
