#!/bin/bash

save_coredumps() {
    graph_dir=/var/lib/graph
    datestamp=$(date +"%Y-%m-%dT%H:%M:%S")
    ls /core.* >& /dev/null && have_cores=yes || have_cores=no
    if [ -d "$graph_dir" -a "$have_cores" = yes ]
    then
        core_dir=$graph_dir/cores
        mkdir -p $core_dir
        exec >> "$core_dir"/messages 2>&1
        echo "${HOSTNAME##*-} Saving core dump on ${HOSTNAME} at ${datestamp}"

        dst="$core_dir/$datestamp-${HOSTNAME}"
        mkdir "$dst"
        cp /usr/local/bin/graph-node "$dst"
        cp /proc/loadavg "$dst"
        [ -f /Dockerfile ] && cp /Dockerfile "$dst"
        tar czf "$dst/etc.tgz" /etc/
        dmesg -e > "$dst/dmesg"
        # Capture environment variables, but filter out passwords
        env | sort | sed -r -e 's/^(postgres_pass|ELASTICSEARCH_PASSWORD)=.*$/\1=REDACTED/' > "$dst/env"

        for f in /core.*
        do
            echo "${HOSTNAME##*-} Found core dump $f"
            mv "$f" "$dst"
        done
        echo "${HOSTNAME##*-} Saving done"
    fi
}

wait_for_ipfs() {
    # Take the IPFS URL in $1 apart and extract host and port. If no explicit
    # port is given, use 443 for https, and 80 otherwise
    if [[ "$1" =~ ^((https?)://)?((.*)@)?([^:/]+)(:([0-9]+))? ]]
    then
        proto=${BASH_REMATCH[2]:-http}
        host=${BASH_REMATCH[5]}
        port=${BASH_REMATCH[7]}
        if [ -z "$port" ]
        then
            [ "$proto" = "https" ] && port=443 || port=80
        fi
        echo "Waiting for IPFS ($host:$port)"
        wait_for "$host:$port" -t 120
    else
        echo "invalid IPFS URL: $1"
        exit 1
    fi
}

run_graph_node() {
    if [ -n "$GRAPH_NODE_CONFIG" ]
    then
        # Start with a configuration file; we don't do a check whether
        # postgres is up in this case, though we probably should
        wait_for_ipfs "$ipfs"
        sleep 5
        exec graph-node \
            --node-id "$node_id" \
            --config "$GRAPH_NODE_CONFIG" \
            --ipfs "$ipfs" \
            ${fork_base:+ --fork-base "$fork_base"}
    else
        unset GRAPH_NODE_CONFIG
        postgres_port=${postgres_port:-5432}
        postgres_url="postgresql://$postgres_user:$postgres_pass@$postgres_host:$postgres_port/$postgres_db?$postgres_args"

        wait_for_ipfs "$ipfs"
        echo "Waiting for Postgres ($postgres_host:$postgres_port)"
        wait_for "$postgres_host:$postgres_port" -t 120
        sleep 5

        exec graph-node \
            --node-id "$node_id" \
            --postgres-url "$postgres_url" \
            --ethereum-rpc $ethereum \
            --ipfs "$ipfs" \
            ${fork_base:+ --fork-base "$fork_base"}
    fi
}

start_query_node() {
    # Query nodes are never the block ingestor
    export DISABLE_BLOCK_INGESTOR=true
    run_graph_node
}

start_index_node() {
    run_graph_node
}

start_combined_node() {
    # No valid reason to disable the block ingestor in this case.
    unset DISABLE_BLOCK_INGESTOR
    run_graph_node
}

# Only the index node with the name set in BLOCK_INGESTOR should ingest
# blocks. For historical reasons, that name is set to the unmangled version
# of `node_id` and we need to check whether we are the block ingestor
# before possibly mangling the node_id.
if [[ ${node_id} != "${BLOCK_INGESTOR}" ]]; then
    export DISABLE_BLOCK_INGESTOR=true
fi

# Allow operators to opt out of legacy character
# restrictions on the node ID by setting enablement
# variable to a non-zero length string:
if [ -z "$GRAPH_NODE_ID_USE_LITERAL_VALUE" ]
then
	node_id="${node_id//-/_}"
fi

if [ -n "$disable_core_dumps" ]
then
    ulimit -c 0
fi

trap save_coredumps EXIT

export PGAPPNAME="${node_id:-$HOSTNAME}"

# Set custom poll interval
if [ -n "$ethereum_polling_interval" ]; then
    export ETHEREUM_POLLING_INTERVAL=$ethereum_polling_interval
fi

case "${node_role:-combined-node}" in
    query-node)
        start_query_node
        ;;
    index-node)
        start_index_node
        ;;
    combined-node)
        start_combined_node
        ;;
    *)
        echo "Unknown mode for start-node: $1"
        echo "usage: start (combined-node|query-node|index-node)"
        exit 1
esac
