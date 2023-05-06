#!/bin/bash

# Create server and client certificate directories
mkdir -p certs/server
mkdir -p certs/client

# Generate server key
openssl genrsa -out certs/server/server.key 2048

# Generate a Certificate Signing Request (CSR) for the server key
openssl req -new -key certs/server/server.key -out certs/server/server.csr -subj "/CN=localhost"

# Self-sign the server CSR to create the server certificate
openssl x509 -req -in certs/server/server.csr -signkey certs/server/server.key -out certs/server/server.crt -days 3650

# Generate server-side Certificate Authority (CA) file
openssl req -x509 -nodes -new -key certs/server/server.key -sha256 -days 3650 -out certs/server/ca.crt -subj "/CN=ServerCA"

# Generate client key
openssl genrsa -out certs/client/client.key 2048

# Generate a Certificate Signing Request (CSR) for the client key
openssl req -new -key certs/client/client.key -out certs/client/client.csr -subj "/CN=Client"

# Sign the client CSR with the server CA to create the client certificate
openssl x509 -req -in certs/client/client.csr -CA certs/server/ca.crt -CAkey certs/server/server.key -CAcreateserial -out certs/client/client.crt -days 365

# Cleanup
rm -f certs/server/server.csr
rm -f certs/client/client.csr

