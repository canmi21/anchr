#!/bin/bash

openssl req -new -x509 -nodes \
  -days 365 \
  -subj "/C=CN/ST=GD/L=SZ/O=Acme, Inc./CN=localhost" \
  -keyout cert.key \
  -out cert.crt