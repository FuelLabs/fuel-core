#!/bin/bash

curl localhost:29899/graph/$1 -XPOST -H 'content-type: application/json' -d '{"query": "query { thing1 { id account } }", "params": "b"}'
