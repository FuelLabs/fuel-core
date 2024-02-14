package main

import (
	"github.com/FuelLabs/firehose-fuel/codec"
	pbfuel "github.com/FuelLabs/firehose-fuel/pb/sf/fuel/type/v1"
	firecore "github.com/streamingfast/firehose-core"
)

func main() {
	firecore.Main(&firecore.Chain[*pbfuel.Block]{
		ShortName:            "fuel",
		LongName:             "Fuel",
		ExecutableName:       "fuel-todo",
		FullyQualifiedModule: "github.com/FuelLabs/firehose-fuel",
		Version:              version,

		Protocol:        "FLC",
		ProtocolVersion: 1,

		FirstStreamableBlock: 1,

		BlockFactory:         func() firecore.Block { return new(pbfuel.Block) },
		ConsoleReaderFactory: codec.NewConsoleReader,

		Tools: &firecore.ToolsConfig[*pbfuel.Block]{
			BlockPrinter: printBlock,
		},
	})
}

// Version value, injected via go build `ldflags` at build time, **must** not be removed or inlined
var version = "dev"
