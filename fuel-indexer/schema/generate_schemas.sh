#!/bin/bash

export $(cat .env | xargs)


function setup() {
	rm migrations 2>&1 > /dev/null || true
	ln -s migrations_${1} migrations
}


setup pg
cat diesel.toml.template | sed 's:DB:pg:g' > diesel.toml
diesel --database-url="${PG_DATABASE_URL}"  migration run

setup sqlite
cat diesel.toml.template | grep -v graph_registry | sed 's:DB:sqlite:g' > diesel.toml
diesel --database-url="${SQLITE_DATABASE_URL}"  migration run
