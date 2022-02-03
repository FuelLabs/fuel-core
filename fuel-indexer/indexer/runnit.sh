#!/bin/bash

if [ "x$1" == "x1" ]
then
    curl localhost:29899/graph/demo_namespace -XPOST -H 'content-type: application/json' -d '{"query": "query { thing1 { id count } }", "params": "b"}'
elif [ "x$1" == "x2" ]
then
    curl localhost:29899/graph/demo_namespace -XPOST -H 'content-type: application/json' -d '{"query": "query { thing1 { id account count } }", "params": "b"}'
elif [ "x$1" == "x3" ]
then
    curl localhost:29899/graph/demo_namespace -XPOST -H 'content-type: application/json' -d '{"query": "query { thing1 { count } }", "params": "b"}'
else
    echo "NOPE!!"
fi
