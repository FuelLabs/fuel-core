## Substreams example

1. Set environmental variables
```bash
$> export SUBSTREAMS_API_TOKEN=your_sf_token
$> export SUBSTREAMS_ENDPOINT=your_sf_endpoint # you can also not define this one and use the default specified endpoint
$> export SUBSTREAMS_PACKAGE=path_to_your_spkg
```

2. Run `substreams` example
```bash
cargo run -p graph-chain-substreams --example substreams [module_name] # for graph entities run `graph_out`
```
