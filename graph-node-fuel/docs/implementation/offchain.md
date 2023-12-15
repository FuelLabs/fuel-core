# Offchain data sources

### Summary

Graph Node supports syncing offchain data sources in a subgraph, such as IPFS files. The documentation for subgraph developers can be found in the official docs. This document describes the implementation of offchain data sources and how support for a new kinds offchain data source can be added.

### Implementation Overview

The implementation of offchain data sources has multiple reusable components and data structures, seeking to simplify the addition of new kinds of file data sources. The initially supported data source kind is `file/ipfs`, so in particular any new file kind should be able to reuse a lot the existing code.

The data structures that represent an offchain data source, along with the code that parses it from the manifest or creates it as a dynamic data source, lives in the `graph` crate, in `data_source/offchain.rs`.  A new file kind would probably only need a new `enum Source` variant, and the kind would need to be added to `const OFFCHAIN_KINDS`.

The `OffchainMonitor` is responsible for tracking and fetching the offchain data. It currently lives in `subgraph/context.rs`. When an offchain data source is created from a template, `fn add_source` is called. It is expected that a background task will monitor the source for relevant events, in the case of a file that means the file becoming available and the event is the file content. To process these events, the subgraph runner calls `fn ready_offchain_events`  periodically.

If the data source kind being added relies on polling to check the availability of the monitored object, the generic `PollingMonitor` component can be used. Then the only implementation work is implementing the polling logic itself, as a `tower` service. The `IpfsService` serves as an example of how to do that.

### Testing

Automated testing for this functionality can be tricky, and will need to be discussed in each case, but the `file_data_sources` test in the `runner_tests.rs` can serve as a starting point of how to write an integration test using offchain data source.

### Notes

- Offchain data sources currently can only exist as dynamic data sources, instantiated from templates, and not as static data sources configured in the manifest.
- Some parts of the existing support for offchain data sources assumes they are 'one shot', meaning only a single trigger is ever handled by each offchain data source. This works well for files, the file is found, handled, and that's it. More complex offchain data sources will require additional planning.