# Testing mTLS

- run `gen-certs.sh`
- run `sudo /path/to/caddy run`
- expect listener on port 8001
- run `feroxbuster -u https://localhost:8001 --client-key certs/client/client.key --client-cert certs/client/client.crt`
