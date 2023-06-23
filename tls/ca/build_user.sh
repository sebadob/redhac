#!/bin/bash

USER=sebastiandobe@mailbox.org

# Without password for the key
openssl genrsa -out intermediate/private/$USER.key.pem 2048
# With password for the key
#openssl genrsa -aes256 -out intermediate/private/www.example.com.key.pem 2048
chmod 400 intermediate/private/$USER.key.pem

# Create the CSR
openssl req -config openssl_intermediate.cnf \
      -key intermediate/private/$USER.key.pem \
      -new -sha256 -out intermediate/csr/$USER.csr.pem

# Sign the certificate
openssl ca -config openssl_intermediate.cnf \
      -extensions usr_cert -days 375 -notext -md sha256 \
      -in intermediate/csr/$USER.csr.pem \
      -out intermediate/certs/$USER.cert.pem
chmod 444 intermediate/certs/$USER.cert.pem

# Verify the certificate
openssl x509 -noout -text -in intermediate/certs/$USER.cert.pem

# Verify the certificate chain of trust
openssl verify -CAfile intermediate/certs/ca-chain.cert.pem intermediate/certs/$USER.cert.pem