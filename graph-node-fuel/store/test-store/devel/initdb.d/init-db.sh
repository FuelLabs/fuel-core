#!/bin/bash
set -e

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    CREATE USER graph WITH ENCRYPTED PASSWORD 'graph';

    CREATE DATABASE "graph-sgd";
    CREATE DATABASE "graph-test";

    GRANT ALL PRIVILEGES ON DATABASE "graph-sgd" TO graph;
    GRANT ALL PRIVILEGES ON DATABASE "graph-test" TO graph;

    ALTER ROLE graph SUPERUSER;
EOSQL