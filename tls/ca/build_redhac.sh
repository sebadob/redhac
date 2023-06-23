#!/bin/bash

DOMAIN=redhac.local
COUNTY_CODE=DE
COUNTRY=Gernmany

# Without password for the key
openssl genrsa -out intermediate/private/$DOMAIN.key.pem 2048
# With password for the key
#openssl genrsa -aes256 -out intermediate/private/www.example.com.key.pem 2048
chmod 400 intermediate/private/$DOMAIN.key.pem

# Create the CSR
#openssl req -config openssl_intermediate.cnf \
openssl req -config openssl.cnf \
      -subj "/C=$COUNTY_CODE/L=$COUNTRY/O=redhac/CN=$DOMAIN" \
      -key intermediate/private/$DOMAIN.key.pem \
      -new -sha256 -out intermediate/csr/$DOMAIN.csr.pem

# Sign the certificate
#openssl ca -config openssl_intermediate.cnf \
openssl ca -config openssl.cnf \
      -extensions redhac_cert -days 375 -notext -md sha256 \
      -in intermediate/csr/$DOMAIN.csr.pem \
      -out intermediate/certs/$DOMAIN.cert.pem
chmod 444 intermediate/certs/$DOMAIN.cert.pem

# Verify the certificate
openssl x509 -noout -text -in intermediate/certs/$DOMAIN.cert.pem

# Verify the certificate chain of trust
openssl verify -CAfile intermediate/certs/ca-chain.cert.pem intermediate/certs/$DOMAIN.cert.pem

# Copy certs in place
cp intermediate/certs/ca-chain.cert.pem ..
cp intermediate/certs/redhac.local.cert.pem ..
cp intermediate/private/redhac.local.key.pem ..
